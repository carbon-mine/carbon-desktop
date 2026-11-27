#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
// SPDX-License-Identifier: MPL-2.0

mod accessibility;
mod account_panel;
mod artwork;
mod data;
mod dev_fixtures;
mod home_playback;
use carbon_desktop_gpui::scenery;
mod home;
mod icons;
mod popup;
mod progress;
mod shell;
mod theme;
mod view;

use anyhow::Context as _;
use carbon_profiles::Profile;
use carbon_store::{Action, InstanceId, InstanceView, Snapshot};
use carbon_vanilla::{GameFilePlan, GameFilesPrepared};
use data::{Activity, DataRoot, Inspection, size_label};
use gpui::{
    AnyWindowHandle, App, Application, Context, Entity, FocusHandle, KeyBinding, MouseButton,
    Point, ScrollHandle, TitlebarOptions, Window, WindowDecorations, WindowOptions, actions,
    prelude::*, px,
};
use gpui_component::{Root, input::InputState};
use std::{
    collections::HashMap,
    io::Write,
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, RecvTimeoutError, TryRecvError},
    },
    time::{Duration, Instant},
};
use theme::Preference;
use uuid::Uuid;

actions!(
    desktop,
    [
        FindInstance,
        NewInstance,
        ImportPack,
        Back,
        HistoryBack,
        FocusActivity
    ]
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Overview,
    Settings,
    Storage,
}

impl Tab {
    const fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Settings => 1,
            Self::Storage => 2,
        }
    }
}

/// The two persistent top-level destinations. Home is the ordinary first
/// screen; the direct review launch still starts in the Library so the
/// established automation and readiness checks keep their original entry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Surface {
    Home,
    Library,
}

/// The state restored by Back. Only launcher-owned identity and view choices
/// are retained, never a filesystem path or a worker snapshot.
#[derive(Clone, PartialEq, Eq, Debug)]
struct HistoryEntry {
    origin: Option<gpui::ElementId>,
    surface: Surface,
    review: bool,
    personal: Option<Uuid>,
    fixture: InstanceId,
    tab: Tab,
    show_detail: bool,
}
#[derive(Clone, Copy)]
enum Picker {
    Pack,
    Client,
    Java,
}

enum Outcome {
    Review(Inspection, GameFilePlan),
    Files(GameFilesPrepared, Result<Inspection, String>),
    Inspected(Inspection, &'static str),
    LaunchInputs(carbon_setup::LaunchInputReview),
    OpenedFolder,
    Sandbox(carbon_session::SessionReport),
    Fixtures(Snapshot),
    Profiles(Result<Vec<Profile>, String>),
}

struct Setup {
    version: String,
    inspection: Option<Inspection>,
    plan: Option<GameFilePlan>,
    receipt: Option<GameFilesPrepared>,
    launch_input_review: Option<String>,
    error: Option<String>,
}

impl Setup {
    fn game_files_prepared(&self) -> bool {
        self.inspection
            .as_ref()
            .is_some_and(|i| i.game_files_prepared && data::client_verified(i))
    }
}

const LAUNCH_REVIEW_LABEL: &str = "Checking installed launch inputs";

/// The library rail stays narrow enough to leave the detail pane usable at the
/// default 1040 px window and wide enough to show a truncated instance name.
const RAIL_MIN_WIDTH: f32 = 190.;
const RAIL_MAX_WIDTH: f32 = 290.;
const RAIL_DEFAULT_WIDTH: f32 = 236.;
const RAIL_STEP: f32 = 8.;
const RAIL_STEP_LARGE: f32 = 20.;

fn clamp_rail_width(width: f32) -> f32 {
    width.clamp(RAIL_MIN_WIDTH, RAIL_MAX_WIDTH)
}

/// Translate a resize-handle key into the next bounded rail width.
fn rail_width_for_key(width: f32, key: &str, shift: bool) -> Option<f32> {
    let step = if shift { RAIL_STEP_LARGE } else { RAIL_STEP };
    match key {
        "left" => Some(clamp_rail_width(width - step)),
        "right" => Some(clamp_rail_width(width + step)),
        "home" => Some(RAIL_MIN_WIDTH),
        "end" => Some(RAIL_MAX_WIDTH),
        "enter" | "space" => Some(RAIL_DEFAULT_WIDTH),
        _ => None,
    }
}

struct RailDrag {
    start_x: f32,
    start_width: f32,
}

/// Read-only save and destination inventory for one instance. Loading and
/// errors live beside the detail pane so an empty list is never invented.
enum TargetsState {
    Loading(Uuid),
    Ready(carbon_profiles::InstanceTargets),
    Error(String),
}

impl TargetsState {
    fn complete(
        &mut self,
        request: Uuid,
        result: Result<carbon_profiles::InstanceTargets, String>,
    ) -> bool {
        if !matches!(self, Self::Loading(active) if *active == request) {
            return false;
        }
        *self = match result {
            Ok(snapshot) => Self::Ready(snapshot),
            Err(error) => Self::Error(error),
        };
        true
    }
}

struct Task {
    label: &'static str,
    profile: Option<Uuid>,
    fixture: Option<InstanceId>,
    activity: Activity,
    meter: Entity<progress::Meter>,
    can_cancel: bool,
    protects_session: bool,
    sample_run: SampleRun,
}

/// Redacted live state for the protected sample. The view receives only the
/// phase and terminal record from `carbon_session`; no path, process ID or
/// account identity reaches it.
#[derive(Default)]
struct SampleRun {
    /// The run UUID the runner reports at admission. Following this exact ID
    /// means a task never adopts another run that shares its instance.
    admitted: Option<mpsc::Receiver<Uuid>>,
    run: Option<Uuid>,
    phase: Option<carbon_session::RunPhase>,
    outcome: Option<carbon_session::RunOutcome>,
    subscription: Option<carbon_session::RunSubscription>,
    closed: bool,
}

impl SampleRun {
    fn pending(admitted: mpsc::Receiver<Uuid>) -> Self {
        Self {
            admitted: Some(admitted),
            ..Self::default()
        }
    }

    /// Bind once to the admitted run, then drain its queued updates. Returns
    /// true when the visible status changed. A run that never reaches admission
    /// leaves the task with no status line, and a bound run is never replaced.
    fn observe(&mut self) -> bool {
        let mut changed = false;
        if self.subscription.is_none()
            && self.run.is_none()
            && !self.closed
            && let Some(admitted) = self.admitted.as_ref()
        {
            match admitted.try_recv() {
                Ok(run) => {
                    self.run = Some(run);
                    self.admitted = None;
                    changed = true;
                    match carbon_session::subscribe(run) {
                        Ok(subscription) => self.subscription = Some(subscription),
                        Err(_) => {
                            self.closed = true;
                            // The run retired before this observer attached; keep
                            // its retained terminal record when one exists.
                            if let Some(outcome) = carbon_session::outcome(run) {
                                self.outcome = Some(outcome);
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.admitted = None;
                    self.closed = true;
                    changed = true;
                }
            }
        }
        loop {
            let update = {
                let Some(subscription) = self.subscription.as_ref() else {
                    break;
                };
                subscription.recv_timeout(Duration::ZERO)
            };
            match update {
                Ok(update) => {
                    let finished = matches!(update, carbon_session::RunUpdate::Finished(_));
                    changed |= self.record(update);
                    if finished {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    self.subscription = None;
                    self.closed = true;
                    changed = true;
                    if self.outcome.is_none()
                        && let Some(run) = self.run
                        && let Some(outcome) = carbon_session::outcome(run)
                    {
                        self.outcome = Some(outcome);
                    }
                    break;
                }
            }
        }
        changed
    }

    /// Apply one observed update. The session surface sends state changes in
    /// order and exactly one terminal record, so both are idempotent here.
    fn record(&mut self, update: carbon_session::RunUpdate) -> bool {
        match update {
            carbon_session::RunUpdate::State(state) => {
                let changed = self.phase != Some(state.phase);
                self.phase = Some(state.phase);
                changed
            }
            carbon_session::RunUpdate::Finished(outcome) => {
                self.outcome = Some(outcome);
                self.subscription = None;
                self.closed = true;
                true
            }
        }
    }

    /// The line shown while the sample task runs, or `None` until its run is
    /// published.
    fn status(&self) -> Option<String> {
        if let Some(outcome) = &self.outcome {
            return Some(end_label(outcome));
        }
        if let Some(phase) = self.phase {
            return Some(phase_label(phase).to_owned());
        }
        if self.closed && self.run.is_some() {
            return Some("The protected sample ended without a retained outcome.".to_owned());
        }
        None
    }

    /// The terminal record for the completion message. This prefers the
    /// update the task already drained and falls back to the process record.
    fn summary(&self) -> Option<String> {
        if let Some(outcome) = &self.outcome {
            return Some(end_label(outcome));
        }
        let run = self.run?;
        carbon_session::outcome(run).map(|outcome| end_label(&outcome))
    }

    /// The terminal record this task already drained, or the retained record for
    /// the exact run it bound to.
    fn terminal(&self) -> Option<carbon_session::RunOutcome> {
        self.outcome
            .clone()
            .or_else(|| self.run.and_then(carbon_session::outcome))
    }

    /// A requested stop-and-exit may close only when the run's own record
    /// confirms its OS boundary was empty. A lost observation, or a run still
    /// live for this instance, keeps Carbon open so the recovery fence and the
    /// reported state stay visible instead of the launcher implying the sample
    /// stopped cleanly.
    fn exit_blocked(&self, instance: Option<Uuid>) -> bool {
        let unconfirmed = self.run.is_some()
            && !self
                .terminal()
                .is_some_and(|record| record.drained == Some(true));
        unconfirmed
            || instance.is_some_and(|instance| {
                carbon_session::active_runs()
                    .iter()
                    .any(|live| live.instance == instance)
            })
    }
}

fn phase_label(phase: carbon_session::RunPhase) -> &'static str {
    use carbon_session::RunPhase;
    match phase {
        RunPhase::Starting => "Starting the protected sample in its isolated window…",
        RunPhase::Running => "The protected sample is running in its isolated window.",
        RunPhase::Stopping => "Stopping the protected sample…",
        RunPhase::Draining => "Waiting for the protected sample's processes to exit…",
        RunPhase::Finishing => "The protected sample exited. Confirming its save and boundary…",
        RunPhase::Finished => "The protected sample finished.",
    }
}

fn end_label(outcome: &carbon_session::RunOutcome) -> String {
    use carbon_session::RunEnd;
    let mut text = match outcome.end {
        RunEnd::Completed => "The protected sample completed.",
        RunEnd::StopRequested => "The protected sample stopped after your request.",
        RunEnd::StopForced => "The protected sample did not close in time, so Carbon ended it.",
        RunEnd::Unsettled => "The protected sample ended without a confirmed outcome.",
    }
    .to_owned();
    match outcome.drained {
        Some(false) => text.push_str(" Remaining processes were closed."),
        None if outcome.end != RunEnd::Unsettled => {
            text.push_str(" Its process boundary could not be observed.");
        }
        _ => {}
    }
    text
}

#[derive(Clone, Copy)]
enum ActivityTarget {
    Personal(Uuid),
    Fixture(InstanceId),
}

/// Outcome of asking the live-run directory to stop the sample task.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SampleStop {
    /// This launcher has no protected sample task.
    Unavailable,
    /// The task exists but its run has not been published yet.
    Waiting,
    /// This many runs were signalled.
    Requested(usize),
}

impl Task {
    fn target(&self) -> Option<ActivityTarget> {
        self.profile
            .map(ActivityTarget::Personal)
            .or_else(|| self.fixture.map(ActivityTarget::Fixture))
    }
}

struct Launcher {
    #[cfg(target_os = "linux")]
    reduced_motion: bool,
    accessibility: accessibility::Accessibility,
    root: DataRoot,
    account: Entity<account_panel::AccountPanel>,
    profiles: Vec<Profile>,
    profile_issue: Option<String>,
    fixtures: Option<Snapshot>,
    fixture_issue: Option<String>,
    search: Entity<InputState>,
    root_focus: FocusHandle,
    detail_focus: FocusHandle,
    home_focus: FocusHandle,
    rail_resize_focus: FocusHandle,
    activity_focus: FocusHandle,
    task_focus: FocusHandle,
    server_focus: FocusHandle,
    tab_focus: [FocusHandle; 3],
    rail_width: f32,
    rail_drag: Option<RailDrag>,
    world_previews:
        carbon_profiles::world_preview::WorldPreviews<std::sync::Arc<gpui::RenderImage>>,
    targets: HashMap<Uuid, TargetsState>,
    search_back_to_detail: bool,
    review: bool,
    personal: Option<Uuid>,
    fixture: InstanceId,
    fixture_play_checked: Option<InstanceId>,
    tab: Tab,
    surface: Surface,
    history: Vec<HistoryEntry>,
    artwork: artwork::HomeArtwork,
    home_order: home::HomeOrder,
    diagnostics_open: bool,
    show_detail: bool,
    library_scroll: ScrollHandle,
    detail_scroll: ScrollHandle,
    library_scroll_offset: Point<gpui::Pixels>,
    detail_scroll_offset: Point<gpui::Pixels>,
    preference: Preference,
    #[cfg(windows)]
    native_menu: Option<shell::NativeMenu>,
    applied_dark: Option<bool>,
    popup: Option<AnyWindowHandle>,
    main_window: Option<AnyWindowHandle>,
    exit_after_sample: bool,
    pending_sample_stop: bool,
    picker_open: bool,
    task: Option<Task>,
    fixture_task: Option<Task>,
    last_activity: Option<ActivityTarget>,
    setup: HashMap<Uuid, Setup>,
    message: String,
    readiness: Option<(Instant, Option<PathBuf>)>,
}

impl Launcher {
    fn new(
        root: DataRoot,
        readiness: (Instant, Option<PathBuf>),
        start: Surface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let profiles = carbon_profiles::list(&root.path);
        let fixtures = carbon_store::snapshot(&root.path);
        let personal = profiles
            .as_ref()
            .ok()
            .and_then(|profiles| profiles.first())
            .map(|profile| profile.id);
        let profile_issue = profiles.as_ref().err().cloned();
        let fixture_issue = fixtures.as_ref().err().cloned();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Find an instance"));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(
            &search,
            window,
            |this, input, event: &gpui_component::input::InputEvent, window, cx| {
                if matches!(event, gpui_component::input::InputEvent::PressEnter { .. }) {
                    let query = input.read(cx).value().trim().to_lowercase();
                    if this.review {
                        if let Some(id) = this
                            .fixtures
                            .iter()
                            .flat_map(|store| &store.instances)
                            .find(|item| item.name.to_lowercase().contains(&query))
                            .map(|item| item.id)
                        {
                            this.choose_fixture(id);
                        }
                    } else if let Some(id) = this
                        .profiles
                        .iter()
                        .find(|item| {
                            format!("{} {}", item.name, item.version.id())
                                .to_lowercase()
                                .contains(&query)
                        })
                        .map(|item| item.id)
                    {
                        this.select_personal(id, cx);
                    }
                    this.focus_detail(window);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.observe_window_activation(window, |this, win, cx| {
            cx.notify();
            if win.is_window_active() {
                this.account
                    .update(cx, account_panel::AccountPanel::refresh);
                if let Some(child) = this.popup {
                    let _ = child.update(cx, |_, popup, _| popup.activate_window());
                }
            }
        })
        .detach();
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        let root_focus = cx.focus_handle();
        root_focus.focus(window);
        let mut was_narrow = is_narrow(window);
        cx.observe_window_bounds(window, move |this, window, _| {
            let narrow = is_narrow(window);
            if narrow && !was_narrow {
                this.root_focus.focus(window);
            }
            was_narrow = narrow;
        })
        .detach();
        let account = cx.new(|_| account_panel::AccountPanel::new(&root.path));
        cx.observe(&account, |_, _, cx| cx.notify()).detach();
        account.update(cx, account_panel::AccountPanel::refresh);
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.account
                    .update(cx, account_panel::AccountPanel::refresh);
            }
        })
        .detach();
        let preference = shell::preference(&root.path);
        #[cfg(windows)]
        let native_menu = shell::NativeMenu::install(window, cx, preference);
        #[cfg(windows)]
        let initial_message = native_menu.as_ref().err().map_or_else(
            || "No active tasks".to_owned(),
            |err| format!("Application menus could not be attached: {err}"),
        );
        #[cfg(not(windows))]
        let initial_message = "No active tasks".to_owned();
        let home_order = home::HomeOrder::load(&root.path);
        let artwork = artwork::HomeArtwork::new(&root.path, cx);
        let mut launcher = Self {
            #[cfg(target_os = "linux")]
            reduced_motion: true,
            accessibility: accessibility::Accessibility::new(window, cx, "Carbon Desktop".into()),
            root,
            account,
            profiles: profiles.unwrap_or_default(),
            profile_issue,
            fixtures: fixtures.ok(),
            fixture_issue,
            search,
            root_focus,
            detail_focus: cx.focus_handle(),
            home_focus: cx.focus_handle().tab_stop(true),
            rail_resize_focus: cx.focus_handle().tab_stop(true),
            activity_focus: cx.focus_handle(),
            task_focus: cx.focus_handle(),
            server_focus: cx.focus_handle(),
            tab_focus: std::array::from_fn(|_| cx.focus_handle()),
            rail_width: RAIL_DEFAULT_WIDTH,
            rail_drag: None,
            world_previews: Default::default(),
            targets: HashMap::new(),
            search_back_to_detail: false,
            review: false,
            personal,
            fixture: InstanceId::Sample,
            fixture_play_checked: None,
            tab: Tab::Overview,
            surface: start,
            history: Vec::new(),
            home_order,
            artwork,
            diagnostics_open: false,
            show_detail: false,
            library_scroll: ScrollHandle::new(),
            detail_scroll: ScrollHandle::new(),
            library_scroll_offset: Point::default(),
            detail_scroll_offset: Point::default(),
            preference,
            #[cfg(windows)]
            native_menu: native_menu.ok(),
            applied_dark: None,
            popup: None,
            main_window: None,
            exit_after_sample: false,
            pending_sample_stop: false,
            picker_open: false,
            task: None,
            fixture_task: None,
            last_activity: None,
            setup: HashMap::new(),
            message: initial_message,
            readiness: Some(readiness),
        };
        if let Some(id) = personal {
            launcher.choose_personal(id, cx);
            launcher.show_detail = false;
        }
        #[cfg(target_os = "linux")]
        progress::observe_linux_motion(cx);
        launcher
    }

    fn open_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_none() && !self.picker_open {
            self.account
                .update(cx, account_panel::AccountPanel::refresh);
            popup::open(self, popup::Kind::Account(self.account.clone()), window, cx);
        }
    }

    fn busy(&self) -> bool {
        self.task.is_some() || self.picker_open
    }

    fn fixture_busy(&self) -> bool {
        self.fixture_task.is_some() || self.picker_open
    }
    fn selected_profile(&self) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|profile| Some(profile.id) == self.personal)
    }
    fn selected_fixture(&self) -> Option<&InstanceView> {
        self.fixtures
            .as_ref()?
            .instances
            .iter()
            .find(|item| item.id == self.fixture)
    }
    fn switch_view(&mut self, review: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.show_surface(Surface::Library, window, cx);
        self.tab = Tab::Overview;
        self.review = review;
        self.show_detail = false;
        self.search_back_to_detail = false;
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
    }
    fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_library(window, cx, false);
    }

    /// Reveal the searchable library. A keyboard search started from a narrow
    /// detail pane remembers its origin so Escape returns there.
    fn reveal_library(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        return_to_detail: bool,
    ) {
        if self.popup.is_some() || self.picker_open {
            return;
        }
        let switched = self.show_surface(Surface::Library, window, cx);
        let narrow = is_narrow(window);
        self.search_back_to_detail =
            narrow && return_to_detail && (self.show_detail || self.search_back_to_detail);
        let reveal_search = self.show_detail && narrow;
        self.show_detail = false;
        if reveal_search || switched {
            let input = self.search.clone();
            window.on_next_frame(move |window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
            });
        } else {
            self.search.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn focus_detail(&mut self, window: &mut Window) {
        if self.show_detail && is_narrow(window) {
            self.search_back_to_detail = false;
            self.detail_focus.focus(window);
        }
    }

    fn set_rail_width(&mut self, width: f32) {
        self.rail_width = clamp_rail_width(width);
    }

    fn cancel_task(&self) {
        if let Some(task) = &self.task {
            task.activity.cancel.store(true, Ordering::Release);
        }
    }

    /// Run IDs this launcher owns for one instance. The redacted directory is
    /// the only source, so a stop can never target another launcher's run.
    fn owned_sample_runs(runs: &[carbon_session::RunState], instance: Uuid) -> Vec<Uuid> {
        runs.iter()
            .filter(|run| run.instance == instance)
            .map(|run| run.run)
            .collect()
    }

    /// Ask each live run for the running sample instance to drain. The owner
    /// still decides the final outcome; this only requests termination.
    fn stop_sample_runs(&mut self, cx: &mut Context<Self>) -> SampleStop {
        let Some(instance) = self.task.as_ref().and_then(|task| task.profile) else {
            return SampleStop::Unavailable;
        };
        let runs = Self::owned_sample_runs(&carbon_session::active_runs(), instance);
        if runs.is_empty() {
            return SampleStop::Waiting;
        }
        let mut requested = 0usize;
        let mut failure = false;
        for run in runs {
            match carbon_session::stop(run) {
                Ok(_) => requested += 1,
                Err(error) => {
                    eprintln!("Could not stop protected sample: {error}");
                    failure = true;
                }
            }
        }
        self.message = if failure {
            "Could not stop the protected sample. Try again, or close the sample window yourself."
                .into()
        } else if requested == 1 {
            "Stopping the protected sample…".into()
        } else {
            format!("Stopping {requested} protected samples…")
        };
        cx.notify();
        SampleStop::Requested(requested)
    }

    /// Stop sample command for the task row. Waiting is reported rather than
    /// leaving the player unsure whether the request was seen.
    fn stop_sample(&mut self, cx: &mut Context<Self>) {
        if self.stop_sample_runs(cx) == SampleStop::Waiting {
            self.message =
                "The protected sample is still starting. Try again once it is running.".into();
            cx.notify();
        }
    }

    /// The player chose to keep Carbon Desktop open. A previously requested
    /// stop-and-exit must not close the window when the sample later drains.
    fn keep_sample_open(&mut self) {
        self.exit_after_sample = false;
        self.pending_sample_stop = false;
    }

    /// Stop the sample and close Carbon Desktop only after its tree has
    /// drained. The worker completes after the boundary is empty, so the
    /// completion path performs the actual close.
    fn stop_sample_and_exit(&mut self, cx: &mut Context<Self>) {
        self.exit_after_sample = true;
        match self.stop_sample_runs(cx) {
            SampleStop::Unavailable => self.close_after_sample(cx),
            SampleStop::Waiting => {
                self.pending_sample_stop = true;
                self.message =
                    "Carbon Desktop will stop the protected sample as soon as it starts, then close."
                        .into();
                cx.notify();
                self.poll_pending_sample_stop(cx);
            }
            SampleStop::Requested(_) => {}
        }
    }

    /// A sample that was still starting when the player asked to exit is
    /// stopped as soon as its run becomes visible. The instance filter still
    /// comes from the sample task, so no other run is touched.
    fn poll_pending_sample_stop(&mut self, cx: &mut Context<Self>) {
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(150)).await;
                let again = this
                    .update(cx, |this, cx| {
                        if !this.exit_after_sample || !this.pending_sample_stop {
                            return false;
                        }
                        if this.task.is_none() {
                            this.pending_sample_stop = false;
                            return false;
                        }
                        match this.stop_sample_runs(cx) {
                            SampleStop::Waiting => true,
                            SampleStop::Unavailable | SampleStop::Requested(_) => {
                                this.pending_sample_stop = false;
                                false
                            }
                        }
                    })
                    .unwrap_or(false);
                if !again {
                    break;
                }
            }
        })
        .detach();
    }

    fn close_after_sample(&mut self, cx: &mut Context<Self>) {
        self.exit_after_sample = false;
        if let Some(window) = self.main_window.take() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
        cx.notify();
    }

    fn profile_uses_saved_server(&self, id: Uuid) -> bool {
        self.profiles
            .iter()
            .find(|profile| profile.id == id)
            .is_some_and(|profile| profile.server_address.is_some())
    }

    /// Load save and destination inventory once per selection or explicit
    /// refresh. The filesystem walk never runs on the UI thread.
    fn load_targets(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.profile_uses_saved_server(id) {
            self.targets.remove(&id);
            return;
        }
        if matches!(self.targets.get(&id), Some(TargetsState::Loading(_))) {
            return;
        }
        let request = Uuid::new_v4();
        self.targets.insert(id, TargetsState::Loading(request));
        let path = self.root.path.clone();
        let work = cx
            .background_executor()
            .spawn(async move { carbon_profiles::instance_targets(&path, id) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if this
                    .targets
                    .get_mut(&id)
                    .is_some_and(|entry| entry.complete(request, result))
                {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn refresh_targets(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.personal else {
            return;
        };
        self.targets.remove(&id);
        self.load_targets(id, cx);
    }

    fn choose_personal(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.personal != Some(id) || self.review {
            self.tab = Tab::Overview;
            self.diagnostics_open = false;
        }
        self.personal = Some(id);
        self.review = false;
        self.show_detail = true;
        if !self.targets.contains_key(&id) {
            self.load_targets(id, cx);
        }
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        if pack_has_requirements(&profile) {
            return;
        }
        if self
            .setup
            .get(&id)
            .is_some_and(|setup| setup.version == profile.version.id())
        {
            return;
        }
        self.setup.remove(&id);
        let path = self.root.path.clone();
        let work = cx
            .background_executor()
            .spawn(async move { data::inspect(&path, id, true) });
        cx.spawn(async move |this, cx| {
            if let Ok(inspection) = work.await {
                this.update(cx, |this, cx| {
                    if this.profiles.iter().any(|current| current == &profile) {
                        this.setup.entry(id).or_insert(Setup {
                            version: profile.version.id().into(),
                            inspection: Some(inspection),
                            plan: None,
                            receipt: None,
                            launch_input_review: None,
                            error: None,
                        });
                    }
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn choose_fixture(&mut self, id: InstanceId) {
        if !self.review || self.fixture != id {
            self.tab = Tab::Overview;
        }
        self.fixture = id;
        self.review = true;
        self.show_detail = true;
        self.search_back_to_detail = false;
    }

    fn select_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.tab_focus[tab.index()].focus(window);
        cx.notify();
    }

    fn check_fixture_play(&mut self, cx: &mut Context<Self>) {
        if self.selected_fixture().is_some() && !self.fixture_busy() {
            self.fixture_play_checked = Some(self.fixture);
            self.last_activity = Some(ActivityTarget::Fixture(self.fixture));
            self.message = "This synthetic fixture cannot launch Minecraft.".to_owned();
            cx.notify();
        }
    }

    fn activity_target(&self) -> Option<ActivityTarget> {
        self.task
            .as_ref()
            .and_then(Task::target)
            .or_else(|| self.fixture_task.as_ref().and_then(Task::target))
            .or(self.last_activity)
    }

    fn activity_name(&self, target: ActivityTarget) -> Option<&str> {
        match target {
            ActivityTarget::Personal(id) => self
                .profiles
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.name.as_str()),
            ActivityTarget::Fixture(id) => self
                .fixtures
                .as_ref()?
                .instances
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.name),
        }
    }

    fn show_activity_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_some() || self.picker_open {
            return;
        }
        self.show_surface(Surface::Library, window, cx);
        match self.activity_target() {
            Some(ActivityTarget::Personal(id))
                if self.profiles.iter().any(|profile| profile.id == id) =>
            {
                self.select_personal(id, cx);
                self.tab = Tab::Overview;
            }
            Some(ActivityTarget::Fixture(id)) => self.choose_fixture(id),
            _ => return,
        }
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.focus_detail(window);
        cx.notify();
    }

    fn work<F>(
        &mut self,
        profile: Option<Profile>,
        label: &'static str,
        can_cancel: bool,
        cx: &mut Context<Self>,
        f: F,
    ) where
        F: FnOnce(PathBuf, Activity) -> Result<Outcome, String> + Send + 'static,
    {
        self.start_work(profile, None, label, can_cancel, cx, f);
    }

    fn start_work<F>(
        &mut self,
        profile: Option<Profile>,
        fixture: Option<InstanceId>,
        label: &'static str,
        can_cancel: bool,
        cx: &mut Context<Self>,
        f: F,
    ) where
        F: FnOnce(PathBuf, Activity) -> Result<Outcome, String> + Send + 'static,
    {
        let fixture_work = fixture.is_some();
        let busy = if fixture_work {
            self.fixture_busy()
        } else {
            self.busy()
        };
        if busy || self.popup.is_some() {
            return;
        }
        let activity = Activity::default();
        let path = self.root.path.clone();
        let operation = activity.clone();
        let reduced = self.reduced_motion();
        let meter = cx.new(|_| progress::Meter::new(Instant::now(), reduced));
        let meter_id = meter.entity_id();
        let task = Task {
            label,
            profile: profile.as_ref().map(|profile| profile.id),
            fixture,
            activity,
            meter,
            can_cancel,
            protects_session: false,
            sample_run: SampleRun::default(),
        };
        if fixture_work {
            self.fixture_task = Some(task);
        } else {
            self.task = Some(task);
        }
        if let Some(profile) = &profile {
            if label != LAUNCH_REVIEW_LABEL {
                for setup in self.setup.values_mut() {
                    setup.launch_input_review = None;
                }
            }
            let setup = self.setup.entry(profile.id).or_insert(Setup {
                version: profile.version.id().into(),
                inspection: None,
                plan: None,
                receipt: None,
                launch_input_review: None,
                error: None,
            });
            if label != LAUNCH_REVIEW_LABEL {
                setup.error = None;
            }
            setup.launch_input_review = None;
        }
        let work = cx
            .background_executor()
            .spawn(async move { f(path, operation) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.complete(profile.as_ref(), fixture_work, result, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(150)).await;
                if !this
                    .update(cx, |this, cx| {
                        let reduced = this.reduced_motion();
                        let task = if fixture_work {
                            this.fixture_task.as_mut()
                        } else {
                            this.task.as_mut()
                        };
                        let Some(task) = task.filter(|task| task.meter.entity_id() == meter_id)
                        else {
                            return false;
                        };
                        let mut changed = false;
                        if task.protects_session {
                            changed |= task.sample_run.observe();
                        }
                        let progress = task.activity.latest();
                        changed |= task.meter.update(cx, |meter, cx| {
                            let changed = meter.retarget(progress, reduced, Instant::now());
                            if changed {
                                cx.notify();
                            }
                            changed
                        });
                        if changed {
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn complete(
        &mut self,
        profile: Option<&Profile>,
        fixture_work: bool,
        result: Result<Outcome, String>,
        cx: &mut Context<Self>,
    ) {
        let mut task = if fixture_work {
            self.fixture_task.take()
        } else {
            self.task.take()
        };
        if self.exit_after_sample && !fixture_work {
            self.pending_sample_stop = false;
            // Drain admission and terminal evidence published before this
            // completion, so a run that retired inside one tick is still seen.
            if let Some(task) = task.as_mut() {
                task.sample_run.observe();
            }
            let instance = task.as_ref().and_then(|task| task.profile);
            // Closing must not imply a clean stop the run never confirmed. A
            // lost or still-populated boundary keeps the launcher open with the
            // recovery state instead of exiting silently.
            if task
                .as_ref()
                .is_some_and(|task| task.protects_session && task.sample_run.exit_blocked(instance))
            {
                self.exit_after_sample = false;
                self.last_activity = task.as_ref().and_then(Task::target);
                let summary = task
                    .as_ref()
                    .and_then(|task| task.sample_run.summary())
                    .unwrap_or_else(|| {
                        "The protected sample ended without a retained outcome.".to_owned()
                    });
                self.message = format!(
                    "Carbon Desktop stayed open because the protected sample's process tree was not confirmed empty. {summary}"
                );
                cx.notify();
                return;
            }
            self.close_after_sample(cx);
            return;
        }
        if !fixture_work {
            self.pending_sample_stop = false;
        }
        self.last_activity = task.as_ref().and_then(Task::target);
        let cancelled = task
            .as_ref()
            .is_some_and(|task| task.activity.cancel.load(Ordering::Acquire));
        let requires_recheck = task.as_ref().is_some_and(|task| {
            matches!(
                task.label,
                "Checking setup"
                    | "Checking selected Java folder"
                    | "Preparing managed JDK"
                    | "Verifying client JAR"
            )
        });
        let setup = profile.and_then(|profile| self.setup.get_mut(&profile.id));
        let mut profiles_reloaded = false;
        match result {
            Ok(Outcome::LaunchInputs(review)) => {
                if let Some(setup) = setup
                    && profile.is_some_and(|profile| self.profiles.contains(profile))
                {
                    setup.launch_input_review = Some(review.message);
                }
                self.message = "Launch input review finished.".into();
            }
            Ok(Outcome::Review(inspection, plan)) => {
                if let Some(setup) = setup {
                    setup.inspection = Some(inspection);
                    setup.plan = Some(plan);
                }
                self.message = "Install review is ready.".to_owned();
            }
            Ok(Outcome::Files(receipt, refresh)) => {
                self.message = format!(
                    "{} game files verified for Minecraft {}.",
                    receipt.unique_files, receipt.version
                );
                if let Some(setup) = setup {
                    setup.plan = None;
                    setup.receipt = Some(receipt);
                    match refresh {
                        Ok(inspection) => setup.inspection = Some(inspection),
                        Err(error) => {
                            setup.inspection = None;
                            let failure = format!(
                                "{} The instance could not be confirmed. Review setup again. {error}",
                                self.message
                            );
                            self.message = failure.clone();
                            setup.error = Some(failure);
                        }
                    }
                }
            }
            Ok(Outcome::Inspected(inspection, message)) => {
                if let Some(setup) = setup {
                    if setup
                        .plan
                        .as_ref()
                        .is_some_and(|plan| plan.metadata_sha1 != inspection.metadata.metadata_sha1)
                    {
                        setup.plan = None;
                    }
                    setup.inspection = Some(inspection);
                }
                self.message = message.to_owned();
            }
            Ok(Outcome::OpenedFolder) => {
                self.message = "Opened this instance's game folder.".to_owned();
            }
            Ok(Outcome::Sandbox(report)) => {
                let mut message = if report.saved {
                    format!("Sandbox sample saved · Count {}.", report.sample_count)
                } else {
                    "Sandbox test closed without saving.".into()
                };
                if let Some(summary) = task.as_ref().and_then(|task| task.sample_run.summary()) {
                    message.push(' ');
                    message.push_str(&summary);
                }
                self.message = message;
            }
            Ok(Outcome::Fixtures(snapshot)) => {
                self.message = snapshot.message.clone();
                self.fixture_issue = None;
                self.fixtures = Some(snapshot);
                if self.selected_fixture().is_none() {
                    self.fixture = InstanceId::Sample;
                }
            }
            Ok(Outcome::Profiles(profiles)) => {
                match profiles {
                    Ok(profiles) => {
                        self.setup.retain(|id, setup| {
                            setup.launch_input_review = None;
                            profiles
                                .iter()
                                .any(|profile| profile.id == *id && self.profiles.contains(profile))
                        });
                        self.targets.clear();
                        self.profiles = profiles;
                        self.home_order.retain_profiles(&self.profiles);
                        self.profile_issue = None;
                        if self.selected_profile().is_none() {
                            self.personal = self.profiles.first().map(|profile| profile.id);
                        }
                    }
                    Err(error) => self.profile_issue = Some(error),
                }
                self.message = "Library refresh completed.".to_owned();
                profiles_reloaded = true;
            }
            Err(error) => {
                if let Some(setup) = setup {
                    if let Some(inspection) = &mut setup.inspection
                        && let Some(jdk) = task.and_then(|task| {
                            task.activity
                                .take_prepared_jdk(&inspection.metadata.metadata_sha1)
                        })
                    {
                        inspection.managed_java = jdk;
                    }
                    if !cancelled {
                        setup.error = Some(error.clone());
                        if requires_recheck || setup.plan.is_none() {
                            setup.plan = None;
                            setup.receipt = None;
                            setup.inspection = None;
                        }
                    }
                }
                if fixture_work {
                    self.fixture_issue = Some(error.clone());
                }
                self.message = error;
            }
        }
        if profiles_reloaded && let Some(id) = self.personal {
            self.targets.remove(&id);
            self.load_targets(id, cx);
        }
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.review {
            self.start_work(
                None,
                Some(self.fixture),
                "Reading review fixtures",
                false,
                cx,
                |path, _| carbon_store::snapshot(&path).map(Outcome::Fixtures),
            );
        } else {
            self.work(None, "Reading library", false, cx, |path, _| {
                Ok(Outcome::Profiles(carbon_profiles::list(&path)))
            });
        }
    }

    fn open_game_folder(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_profile().map(|profile| profile.id) else {
            return;
        };
        self.work(None, "Opening game folder", false, cx, move |root, _| {
            carbon_profiles::open_game_folder(&root, id).map(|()| Outcome::OpenedFolder)
        });
    }

    fn review_install(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        if pack_has_requirements(&profile) || self.profile_issue.is_some() {
            return;
        }
        let id = profile.id;
        self.tab = Tab::Overview;
        self.work(
            Some(profile),
            "Reviewing install",
            false,
            cx,
            move |path, _| {
                let (inspection, plan) = data::review(&path, id)?;
                Ok(Outcome::Review(inspection, plan))
            },
        );
    }

    fn check_setup(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        if pack_has_requirements(&profile) || self.profile_issue.is_some() {
            return;
        }
        let id = profile.id;
        self.work(
            Some(profile),
            "Checking setup",
            false,
            cx,
            move |path, _| {
                data::inspect(&path, id, false)
                    .map(|report| Outcome::Inspected(report, "Instance setup checked."))
            },
        );
    }

    fn review_launch_inputs(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        let id = profile.id;
        if profile.imported_pack.is_some()
            || self.profile_issue.is_some()
            || !self.setup.get(&id).is_some_and(Setup::game_files_prepared)
        {
            return;
        }
        self.work(
            Some(profile),
            LAUNCH_REVIEW_LABEL,
            false,
            cx,
            move |root, activity| {
                activity.progress("Verifying installed game files and managed Java…".into());
                Ok(Outcome::LaunchInputs(
                    carbon_setup::review_installed_launch_inputs(&root, id),
                ))
            },
        );
    }

    fn select_java(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick(Picker::Java, window, cx);
    }

    fn prepare_jdk(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        if pack_has_requirements(&profile) || self.profile_issue.is_some() {
            return;
        }
        let Some(report) = self
            .setup
            .get(&profile.id)
            .and_then(|setup| setup.inspection.as_ref())
        else {
            return;
        };
        let digest = report.metadata.metadata_sha1.clone();
        let id = profile.id;
        self.work(
            Some(profile),
            "Preparing managed JDK",
            true,
            cx,
            move |path, activity| {
                carbon_setup::prepare_managed_java(&path, id, &digest, |received, total| {
                    activity.java_progress(received, total)
                })
                .map(|report| Outcome::Inspected(report, "Managed JDK verified and selected."))
            },
        );
    }

    fn prepare_files(&mut self, offline: bool, cx: &mut Context<Self>) {
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        if pack_has_requirements(&profile) || self.profile_issue.is_some() {
            return;
        }
        let Some(state) = self.setup.get(&profile.id) else {
            return;
        };
        let digest = if offline {
            state
                .inspection
                .as_ref()
                .map(|i| i.metadata.metadata_sha1.clone())
        } else {
            state.plan.as_ref().map(|p| p.metadata_sha1.clone())
        };
        let Some(digest) = digest else { return };
        let id = profile.id;
        self.work(
            Some(profile),
            if offline {
                "Verifying files"
            } else {
                "Installing instance"
            },
            true,
            cx,
            move |path, activity| {
                let (receipt, refresh) = data::prepare(&path, id, &digest, offline, &activity)?;
                Ok(Outcome::Files(receipt, refresh))
            },
        );
    }
    fn launch_sandbox(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.popup.is_some() {
            return;
        }
        let Some(profile) = self.selected_profile().cloned() else {
            return;
        };
        let id = profile.id;
        let version = profile.version;
        let (admitted, admitted_rx) = mpsc::channel();
        self.work(
            Some(profile),
            "Running the protected sandbox sample",
            false,
            cx,
            move |path, _| {
                let report = data::inspect(&path, id, true)?;
                if report.metadata.version != version.id() {
                    return Err("The profile version changed. Recheck setup.".into());
                }
                let major = u16::try_from(report.metadata.java_major)
                    .map_err(|_| "Unsupported Java major.")?;
                carbon_session::run_with_admission(&path, id, major, move |run| {
                    let _ = admitted.send(run);
                })
                .map(Outcome::Sandbox)
                .map_err(|e| e.to_string())
            },
        );
        if let Some(task) = self.task.as_mut() {
            task.protects_session = true;
            task.sample_run = SampleRun::pending(admitted_rx);
        }
    }

    fn fixture_action(&mut self, action: Action, cx: &mut Context<Self>) {
        if self.fixture_issue.is_some() || self.fixture_busy() || self.popup.is_some() {
            return;
        }
        let target = match action {
            Action::CreateSample => InstanceId::Sample,
            Action::CreateRepeat => InstanceId::Repeat,
            Action::CreateOverlap => InstanceId::Overlap,
            Action::Review { instance }
            | Action::Stage { instance }
            | Action::Cancel { instance }
            | Action::Interrupt { instance }
            | Action::Resume { instance }
            | Action::Activate { instance }
            | Action::Rollback { instance }
            | Action::Repair { instance }
            | Action::SimulateMissing { instance }
            | Action::SimulateCorruption { instance }
            | Action::EditSetting { instance }
            | Action::EditSave { instance } => instance,
        };
        if matches!(
            action,
            Action::CreateSample | Action::CreateRepeat | Action::CreateOverlap
        ) {
            self.choose_fixture(target);
        }
        self.start_work(
            None,
            Some(target),
            "Changing fixture",
            false,
            cx,
            move |path, _| carbon_store::apply(&path, action).map(Outcome::Fixtures),
        );
    }

    fn pick(&mut self, kind: Picker, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || self.popup.is_some() || self.profile_issue.is_some() {
            return;
        }
        let profile = match kind {
            Picker::Client | Picker::Java => self.selected_profile().cloned(),
            Picker::Pack => None,
        };
        if !matches!(kind, Picker::Pack) && profile.as_ref().is_none_or(pack_has_requirements) {
            return;
        }
        let digest = profile
            .as_ref()
            .and_then(|profile| self.setup.get(&profile.id))
            .and_then(|setup| setup.inspection.as_ref())
            .map(|report| report.metadata.metadata_sha1.clone());
        if matches!(kind, Picker::Java) && digest.is_none() {
            return;
        }
        let (title, suffix) = match kind {
            Picker::Pack => ("Import a local Minecraft pack", Some("mrpack")),
            Picker::Client => ("Import the selected Minecraft client JAR", Some("jar")),
            Picker::Java => ("Choose the Java home folder", None),
        };
        self.picker_open = true;
        self.last_activity = None;
        #[cfg(target_os = "linux")]
        let selection = {
            let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: suffix.is_some(),
                directories: suffix.is_none(),
                multiple: false,
                prompt: Some(title.into()),
            });
            async move {
                match receiver.await {
                    Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                    Ok(Err(error)) => Err(format!("The desktop picker could not open. {error}")),
                    Err(_) => Err("The desktop picker was interrupted.".to_owned()),
                }
            }
        };
        #[cfg(not(target_os = "linux"))]
        let selection = {
            let dialog = parent_file_dialog(rfd::AsyncFileDialog::new().set_title(title), window);
            async move {
                let result = if let Some(suffix) = suffix {
                    dialog.add_filter(title, &[suffix]).pick_file().await
                } else {
                    dialog.pick_folder().await
                };
                Ok::<_, String>(result.map(|file| file.path().to_owned()))
            }
        };
        cx.spawn_in(window, async move |this, cx| {
            let selected = selection.await;
            let _ = this.update_in(cx, move |this, window, cx| {
                this.picker_open = false;
                let selected = match selected {
                    Ok(selected) => selected,
                    Err(error) => {
                        this.message = error;
                        cx.notify();
                        return;
                    }
                };
                if let Some(path) = selected {
                    match kind {
                        Picker::Pack => this.review_pack(path, window, cx),
                        Picker::Client => {
                            let profile = profile.expect("client picker has an owning profile");
                            let id = profile.id;
                            this.work(
                                Some(profile),
                                "Verifying client JAR",
                                false,
                                cx,
                                move |root, _| {
                                    let file = carbon_vanilla::open_client_file(&path)?;
                                    carbon_vanilla::import_client(&root, id, file)?;
                                    data::inspect(&root, id, true).map(|report| Outcome::Inspected(report, "Client JAR verified and added to the shared cache."))
                                },
                            );
                        }
                        Picker::Java => {
                            let (Some(profile), Some(digest)) = (profile, digest) else { return };
                            let id = profile.id;
                            this.work(Some(profile), "Checking selected Java folder", false, cx, move |root, _| {
                                let metadata = carbon_vanilla::prepare(&root, id, true)?;
                                if metadata.metadata_sha1 != digest {
                                    return Err("The instance version changed. Check setup before choosing Java.".into());
                                }
                                carbon_setup::select_java_home(&root, metadata, path)
                                    .map(|report| Outcome::Inspected(report, "Java folder verified and selected for this instance."))
                            });
                        }
                    }
                } else {
                    this.message = "Selection cancelled.".into();
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn review_pack(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let work = cx
            .background_executor()
            .spawn(async move { carbon_pack::review_file(&path).map(|review| (path, review)) });
        self.picker_open = true;
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, move |this, window, cx| {
                this.picker_open = false;
                match result {
                    Ok((path, review)) => {
                        popup::open(this, popup::Kind::ImportPack(path, review), window, cx)
                    }
                    Err(error) => {
                        this.message = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_icon_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.personal else { return };
        match carbon_profiles::existing_profile(&self.root.path, id) {
            Ok(Some(profile)) => {
                if let Some(row) = self.profiles.iter_mut().find(|row| row.id == id) {
                    row.icon_id.clone_from(&profile.icon_id);
                }
                popup::open(self, popup::Kind::Icon(profile), window, cx);
            }
            Ok(None) => self.message = "This instance is unavailable. Reload the library.".into(),
            Err(error) => self.message = error,
        }
        cx.notify();
    }

    fn editor(&mut self, mode: popup::Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || self.popup.is_some() || self.profile_issue.is_some() {
            return;
        }
        let profile = (mode != popup::Mode::Create)
            .then(|| self.selected_profile().cloned())
            .flatten();
        if mode != popup::Mode::Create && profile.is_none() {
            return;
        }
        if mode == popup::Mode::Clone && profile.as_ref().is_some_and(|p| p.imported_pack.is_some())
        {
            return;
        }
        popup::open(self, popup::Kind::Profile(mode, profile), window, cx);
    }

    fn primary_label(profile: &Profile) -> &'static str {
        if profile.server_address.is_some() {
            "Review join"
        } else {
            "Play"
        }
    }

    fn activate_primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_some() || self.picker_open {
            return;
        }
        self.show_surface(Surface::Library, window, cx);
        self.tab = Tab::Overview;
        let focus = if self
            .selected_profile()
            .is_some_and(|profile| profile.server_address.is_some())
        {
            self.server_focus.clone()
        } else {
            self.task_focus.clone()
        };
        window.on_next_frame(move |window, _| focus.focus(window));
        cx.notify();
    }

    fn open_targets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_some() || self.picker_open || self.profile_issue.is_some() {
            return;
        }
        let Some(profile) = self
            .selected_profile()
            .filter(|profile| profile.server_address.is_none())
        else {
            return;
        };
        let id = profile.id;
        match carbon_profiles::instance_targets(&self.root.path, id) {
            Ok(targets) => popup::open(self, popup::Kind::Targets(id, targets), window, cx),
            Err(error) => {
                self.message = error;
                self.last_activity = Some(ActivityTarget::Personal(id));
                if let Ok(profile) = carbon_profiles::server_profile(&self.root.path, id) {
                    self.refresh_server_binding(profile);
                }
                cx.notify();
            }
        }
    }

    fn server_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() || self.popup.is_some() || self.profile_issue.is_some() {
            return;
        }
        let Some(id) = self.selected_profile().map(|profile| profile.id) else {
            return;
        };
        match carbon_profiles::server_profile(&self.root.path, id) {
            Ok(profile) => popup::open(self, popup::Kind::Server(profile), window, cx),
            Err(error) => {
                self.message = error;
                self.last_activity = Some(ActivityTarget::Personal(id));
                cx.notify();
            }
        }
    }

    fn refresh_server_binding(&mut self, updated: Profile) {
        if let Some(current) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == updated.id)
        {
            current.server_address = updated.server_address;
        }
    }

    fn server_saved(
        &mut self,
        updated: carbon_profiles::ServerBindingUpdate,
        cx: &mut Context<Self>,
    ) {
        self.message = updated.message().into();
        let id = updated.profile.id;
        self.last_activity = Some(ActivityTarget::Personal(id));
        self.refresh_server_binding(updated.profile);
        self.targets.remove(&id);
        self.load_targets(id, cx);
        cx.notify();
    }

    fn copy_server(&mut self, cx: &mut Context<Self>) {
        let Some(profile) = self.selected_profile() else {
            return;
        };
        let id = profile.id;
        let Some(address) = profile.server_address.clone() else {
            return;
        };
        match carbon_profiles::server_profile(&self.root.path, id) {
            Ok(profile) if profile.server_address.as_ref() == Some(&address) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(address.to_string()));
                self.message = "Server address copy requested.".into();
            }
            Ok(profile) => {
                self.message = if profile.server_address.is_some() {
                    "The server address changed. Review the updated address before copying.".into()
                } else {
                    "The server address was removed in another window.".into()
                };
                self.refresh_server_binding(profile);
            }
            Err(error) => self.message = error,
        }
        self.last_activity = Some(ActivityTarget::Personal(id));
        cx.notify();
    }

    fn set_profiles(&mut self, profiles: Vec<Profile>, id: Uuid, cx: &mut Context<Self>) {
        self.setup.remove(&id);
        self.targets.clear();
        self.profiles = profiles;
        self.last_activity = Some(ActivityTarget::Personal(id));
        self.message = "Instance saved.".to_owned();
        self.choose_personal(id, cx);
        self.remember_recent(id);
    }

    fn pack_imported(
        &mut self,
        profile: Profile,
        profiles: Result<Vec<Profile>, String>,
        cx: &mut Context<Self>,
    ) {
        let id = profile.id;
        match profiles {
            Ok(profiles) => {
                self.set_profiles(profiles, id, cx);
                self.message =
                    "Pack imported. Review the instance's remaining requirements.".into();
            }
            Err(error) => {
                if !self.profiles.iter().any(|profile| profile.id == id) {
                    self.profiles.push(profile);
                }
                self.choose_personal(id, cx);
                self.profile_issue = Some(error);
                self.message = "Pack imported. Reload the library to resume editing.".into();
            }
        }
    }

    fn frame_ready(&mut self, window: &mut Window) {
        if self.profiles.is_empty() || self.selected_profile().is_none() {
            return;
        }
        if let Some((began, destination)) = self.readiness.take() {
            window.on_next_frame(move |_, _| {
                let marker = format!("CARBON_LIBRARY_READY {}\n", began.elapsed().as_millis());
                print!("{marker}");
                let _ = std::io::stdout().flush();
                if let Some(path) = destination {
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                    {
                        Ok(mut file) => {
                            if let Err(error) = file.write_all(marker.as_bytes()) {
                                eprintln!(
                                    "Could not write the readiness file {}: {error}",
                                    path.display()
                                );
                            }
                        }
                        Err(error) => eprintln!(
                            "Could not create the readiness file {}: {error}",
                            path.display()
                        ),
                    }
                }
            });
        }
    }
}

fn pack_has_requirements(profile: &Profile) -> bool {
    profile
        .imported_pack
        .as_ref()
        .is_some_and(|pack| pack.loader.is_some() || pack.pending_files > 0)
}

fn is_narrow(window: &Window) -> bool {
    window.viewport_size().width < px(736.)
}

/// Escape returns to the detail pane only when a keyboard search started there
/// and the library replaced it in a compact one-pane layout.
fn back_returns_to_detail(search_back_to_detail: bool, show_detail: bool, narrow: bool) -> bool {
    search_back_to_detail && !show_detail && narrow
}

#[cfg(windows)]
fn parent_file_dialog(dialog: rfd::AsyncFileDialog, window: &Window) -> rfd::AsyncFileDialog {
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };

    struct Owner<'a>(&'a Window);

    impl HasWindowHandle for Owner<'_> {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            HasWindowHandle::window_handle(self.0)
        }
    }

    impl HasDisplayHandle for Owner<'_> {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            // GPUI 0.2.2 does not implement this on Windows. The Windows display
            // handle is empty and can be constructed without borrowing native state.
            Ok(DisplayHandle::windows())
        }
    }

    dialog.set_parent(&Owner(window))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn parent_file_dialog(dialog: rfd::AsyncFileDialog, window: &Window) -> rfd::AsyncFileDialog {
    dialog.set_parent(window)
}

fn argument_value(
    args: &[std::ffi::OsString],
    flag: &str,
) -> anyhow::Result<Option<std::ffi::OsString>> {
    let Some(index) = args.iter().position(|argument| argument == flag) else {
        return Ok(None);
    };
    let value = args
        .get(index + 1)
        .filter(|value| value.to_str().is_none_or(|text| !text.starts_with("--")));
    value.cloned().map(Some).ok_or_else(|| {
        anyhow::anyhow!("{flag} requires a value. See docs/dev-fixtures.md for the named sets.")
    })
}

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    shell::configure_renderer();
    let began = Instant::now();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let comparison = args.iter().any(|a| a == "--comparison-review");
    let temporary = comparison || args.iter().any(|a| a == "--temporary-review");
    let seed = comparison || args.iter().any(|a| a == "--seed-review");
    let ready_file = args
        .windows(2)
        .find(|pair| pair[0] == "--ready-file")
        .map(|pair| PathBuf::from(&pair[1]));
    let fixture_root = argument_value(&args, "--fixture-root")?.map(PathBuf::from);
    let fixture_set = argument_value(&args, "--seed-fixtures")?;
    let reset_fixtures = args.iter().any(|a| a == "--reset-fixtures");
    let seed_only = args.iter().any(|a| a == "--seed-only");
    if reset_fixtures && fixture_set.is_none() {
        anyhow::bail!("--reset-fixtures requires --seed-fixtures <default|empty|large|faults>.");
    }
    if seed_only && fixture_set.is_none() {
        anyhow::bail!("--seed-only requires --seed-fixtures <default|empty|large|faults>.");
    }
    if (fixture_set.is_some() || fixture_root.is_some()) && (comparison || temporary || seed) {
        anyhow::bail!(
            "--fixture-root and --seed-fixtures are separate from --comparison-review, --temporary-review and --seed-review."
        );
    }
    let (data, start) = if let Some(set) = fixture_set {
        let set = dev_fixtures::Set::parse(
            set.to_str()
                .context("--seed-fixtures must be a UTF-8 set name")?,
        )?;
        let root = match fixture_root {
            Some(root) => root,
            None => dev_fixtures::default_root(set)?,
        };
        let (root, marker) = dev_fixtures::prepare(&root, set, reset_fixtures)?;
        eprintln!(
            "Seeded '{}' development fixtures ({} rows) at {}",
            marker.set.as_str(),
            marker.rows.len(),
            root.display()
        );
        if seed_only {
            return Ok(());
        }
        (
            DataRoot::at(root),
            if set.starts_in_library() {
                Surface::Library
            } else {
                Surface::Home
            },
        )
    } else if let Some(root) = fixture_root {
        let root = dev_fixtures::guard(&root)?.path;
        let start = if dev_fixtures::reopen_set(&root)?.starts_in_library() {
            Surface::Library
        } else {
            Surface::Home
        };
        (DataRoot::at(root), start)
    } else {
        // The seeded comparison launch is the direct, automation-friendly Library
        // entry. Ordinary launches open Home first and keep Library one tab away.
        let start = if comparison {
            Surface::Library
        } else {
            Surface::Home
        };
        (DataRoot::open(temporary, seed, comparison)?, start)
    };
    let app = Application::new().with_assets(icons::Assets);
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-f", FindInstance, Some("CarbonDesktop")),
            KeyBinding::new("ctrl-n", NewInstance, Some("CarbonDesktop")),
            KeyBinding::new("ctrl-o", ImportPack, Some("CarbonDesktop")),
            KeyBinding::new("ctrl-w", shell::CloseLauncher, Some("CarbonDesktop")),
            KeyBinding::new("escape", Back, Some("CarbonDesktop")),
            KeyBinding::new("alt-left", HistoryBack, Some("CarbonDesktop")),
            KeyBinding::new("f6", FocusActivity, Some("CarbonDesktop")),
            KeyBinding::new("escape", popup::Close, Some("CarbonPopup")),
            KeyBinding::new("enter", popup::Save, Some("CarbonPopup")),
        ]);
        shell::bind_menu_keys(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let (initial, display_id) = shell::initial_bounds(&data.path, cx);
        #[cfg(target_os = "linux")]
        let fitted_size = initial.get_bounds().size;
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Carbon Desktop · GPUI".into()),
                    ..Default::default()
                }),
                window_bounds: Some(initial),
                display_id,
                is_resizable: false,
                window_min_size: Some(initial.get_bounds().size),
                window_decorations: Some(WindowDecorations::Server),
                app_id: Some("dev.carbonmine.gpui".into()),
                show: !cfg!(windows),
                focus: !cfg!(windows),
                ..Default::default()
            },
            |window, cx| {
                shell::configure_main_window(window);
                popup::set_app_icon(window);
                let view = cx.new(|cx| {
                    #[cfg(target_os = "linux")]
                    shell::keep_main_windowed(window, fitted_size, cx);
                    Launcher::new(data, (began, ready_file), start, window, cx)
                });
                let weak = view.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    weak.update(cx, |view, cx| view.allow_close(window, cx))
                        .unwrap_or(true)
                });
                let root = cx.new(|cx| Root::new(view, window, cx));
                #[cfg(windows)]
                window.activate_window();
                root
            },
        )
        .expect("Open Carbon Desktop GPUI");
        cx.activate(true);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_sample_runs_keep_other_instances_untouched() {
        let own = Uuid::new_v4();
        let other = Uuid::new_v4();
        let state = |run, instance| carbon_session::RunState {
            run,
            instance,
            phase: carbon_session::RunPhase::Running,
        };
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let runs = [
            state(first, own),
            state(second, other),
            state(Uuid::new_v4(), own),
        ];
        assert_eq!(
            Launcher::owned_sample_runs(&runs, own),
            vec![first, runs[2].run]
        );
        assert_eq!(
            Launcher::owned_sample_runs(&runs, Uuid::new_v4()),
            Vec::<Uuid>::new()
        );
    }

    #[test]
    fn rail_width_stays_within_the_reviewed_bounds() {
        assert_eq!(clamp_rail_width(0.), RAIL_MIN_WIDTH);
        assert_eq!(clamp_rail_width(120.), RAIL_MIN_WIDTH);
        assert_eq!(clamp_rail_width(236.), 236.);
        assert_eq!(clamp_rail_width(400.), RAIL_MAX_WIDTH);
        assert!((RAIL_MIN_WIDTH..=RAIL_MAX_WIDTH).contains(&RAIL_DEFAULT_WIDTH));
    }

    #[test]
    fn rail_keyboard_resize_clamps_and_resets() {
        assert_eq!(
            rail_width_for_key(RAIL_DEFAULT_WIDTH, "left", false),
            Some(RAIL_DEFAULT_WIDTH - RAIL_STEP)
        );
        assert_eq!(
            rail_width_for_key(RAIL_DEFAULT_WIDTH, "right", true),
            Some(RAIL_DEFAULT_WIDTH + RAIL_STEP_LARGE)
        );
        assert_eq!(
            rail_width_for_key(RAIL_DEFAULT_WIDTH, "home", false),
            Some(RAIL_MIN_WIDTH)
        );
        assert_eq!(
            rail_width_for_key(RAIL_DEFAULT_WIDTH, "end", false),
            Some(RAIL_MAX_WIDTH)
        );
        assert_eq!(
            rail_width_for_key(200., "enter", false),
            Some(RAIL_DEFAULT_WIDTH)
        );
        assert_eq!(
            rail_width_for_key(200., "space", false),
            Some(RAIL_DEFAULT_WIDTH)
        );
        assert_eq!(rail_width_for_key(200., "tab", false), None);

        let mut width = RAIL_DEFAULT_WIDTH;
        for _ in 0..40 {
            width = rail_width_for_key(width, "left", true).unwrap();
        }
        assert_eq!(width, RAIL_MIN_WIDTH);
        for _ in 0..40 {
            width = rail_width_for_key(width, "right", true).unwrap();
        }
        assert_eq!(width, RAIL_MAX_WIDTH);
    }

    #[test]
    fn late_inventory_completion_preserves_a_newer_result() {
        let obsolete = Uuid::new_v4();
        let current = Uuid::new_v4();
        let mut entry = TargetsState::Loading(current);
        assert!(!entry.complete(obsolete, Err("old read".into())));
        assert!(entry.complete(current, Err("current read".into())));
        assert!(!entry.complete(obsolete, Err("old read".into())));
        assert!(matches!(entry, TargetsState::Error(message) if message == "current read"));
    }

    #[test]
    fn compact_search_returns_to_detail_only_from_its_origin() {
        assert!(back_returns_to_detail(true, false, true));
        assert!(!back_returns_to_detail(true, false, false));
        assert!(!back_returns_to_detail(true, true, true));
        assert!(!back_returns_to_detail(false, false, true));
    }

    #[test]
    fn every_live_phase_has_a_player_facing_status() {
        use carbon_session::RunPhase;
        let phases = [
            RunPhase::Starting,
            RunPhase::Running,
            RunPhase::Stopping,
            RunPhase::Draining,
            RunPhase::Finishing,
            RunPhase::Finished,
        ];
        for phase in phases {
            let label = phase_label(phase);
            assert!(!label.is_empty());
            assert!(label.ends_with('…') || label.ends_with('.'));
        }
    }

    #[test]
    fn run_status_follows_observed_phases_and_keeps_the_terminal_record() {
        use carbon_session::{RunEnd, RunOutcome, RunPhase, RunState, RunUpdate};
        let run = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let state = |phase| {
            RunUpdate::State(RunState {
                run,
                instance,
                phase,
            })
        };
        let mut sample = SampleRun::default();
        assert_eq!(sample.status(), None);

        assert!(sample.record(state(RunPhase::Starting)));
        assert_eq!(
            sample.status().as_deref(),
            Some("Starting the protected sample in its isolated window…")
        );
        assert!(!sample.record(state(RunPhase::Starting)));
        assert!(sample.record(state(RunPhase::Running)));
        assert_eq!(
            sample.status().as_deref(),
            Some("The protected sample is running in its isolated window.")
        );
        assert!(sample.record(state(RunPhase::Draining)));
        assert_eq!(
            sample.status().as_deref(),
            Some("Waiting for the protected sample's processes to exit…")
        );

        assert!(sample.record(RunUpdate::Finished(RunOutcome {
            run,
            instance,
            end: RunEnd::StopRequested,
            drained: Some(true),
        })));
        let terminal = sample.status();
        assert_eq!(
            terminal.as_deref(),
            Some("The protected sample stopped after your request.")
        );
        assert_eq!(sample.summary(), terminal);
        assert!(sample.closed);

        // A stale state update cannot replace the settled terminal record.
        assert!(sample.record(state(RunPhase::Running)));
        assert_eq!(sample.status(), terminal);
    }

    #[test]
    fn a_bound_run_without_a_retained_record_is_reported_as_unknown() {
        let sample = SampleRun {
            run: Some(Uuid::new_v4()),
            closed: true,
            ..SampleRun::default()
        };
        assert_eq!(
            sample.status().as_deref(),
            Some("The protected sample ended without a retained outcome.")
        );
    }

    #[test]
    fn terminal_outcome_never_claims_more_than_the_record_holds() {
        use carbon_session::{RunEnd, RunOutcome};
        let outcome = |end, drained| RunOutcome {
            run: Uuid::new_v4(),
            instance: Uuid::new_v4(),
            end,
            drained,
        };
        assert_eq!(
            end_label(&outcome(RunEnd::Completed, Some(true))),
            "The protected sample completed."
        );
        assert_eq!(
            end_label(&outcome(RunEnd::StopRequested, Some(true))),
            "The protected sample stopped after your request."
        );
        assert_eq!(
            end_label(&outcome(RunEnd::StopRequested, Some(false))),
            "The protected sample stopped after your request. Remaining processes were closed."
        );
        assert_eq!(
            end_label(&outcome(RunEnd::StopForced, Some(true))),
            "The protected sample did not close in time, so Carbon ended it."
        );
        assert_eq!(
            end_label(&outcome(RunEnd::Completed, None)),
            "The protected sample completed. Its process boundary could not be observed."
        );
        assert_eq!(
            end_label(&outcome(RunEnd::Unsettled, None)),
            "The protected sample ended without a confirmed outcome."
        );
    }

    #[test]
    fn a_requested_exit_waits_for_a_confirmed_empty_boundary() {
        use carbon_session::{RunEnd, RunOutcome};
        let run = Uuid::new_v4();
        let instance = Uuid::new_v4();
        let record = |end, drained| RunOutcome {
            run,
            instance,
            end,
            drained,
        };
        let settled = |outcome| SampleRun {
            run: Some(run),
            outcome: Some(outcome),
            closed: true,
            ..SampleRun::default()
        };

        // A start that never admitted a run has nothing to drain.
        assert!(!SampleRun::default().exit_blocked(None));
        assert!(!SampleRun::default().exit_blocked(Some(instance)));
        assert!(!settled(record(RunEnd::Completed, Some(true))).exit_blocked(None));
        assert!(!settled(record(RunEnd::StopRequested, Some(true))).exit_blocked(None));
        assert!(!settled(record(RunEnd::StopForced, Some(true))).exit_blocked(None));
        // A failed post-launch check still observed an empty boundary.
        assert!(!settled(record(RunEnd::Unsettled, Some(true))).exit_blocked(None));
        assert!(settled(record(RunEnd::StopRequested, Some(false))).exit_blocked(None));
        assert!(settled(record(RunEnd::Completed, None)).exit_blocked(None));
        assert!(settled(record(RunEnd::Unsettled, None)).exit_blocked(None));
        // A bound run whose record was never retained cannot confirm anything.
        assert!(
            SampleRun {
                run: Some(run),
                ..SampleRun::default()
            }
            .exit_blocked(None)
        );
    }
}
