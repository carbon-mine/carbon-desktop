use crate::{
    client,
    play::{self, Clock, Installation, LaunchDecision, PlayBlocker, PlayMode, Selection},
    vault::{SavedAccounts, Secret, Vault, ACCOUNT_LIMIT},
    AccountError, AuthenticatedAccount, DeviceClient, DevicePoll,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountState {
    Unavailable,
    SignedOut,
    Starting,
    AwaitingBrowser,
    SignedIn,
    Error,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: Uuid,
    pub name: String,
    pub verified: bool,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub state: AccountState,
    pub message: String,
    pub name: Option<String>,
    pub user_code: Option<String>,
    pub accounts: Vec<AccountSummary>,
    pub active_account: Option<Uuid>,
    pub cancellable: bool,
    pub removal_retry: Option<Uuid>,
}

#[derive(Clone)]
pub struct Accounts {
    owner: Arc<Owner>,
    root: Option<PathBuf>,
    compiled: Option<Uuid>,
    source: Option<RegistrationSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RegistrationSource {
    Compiled,
    Custom,
    #[cfg(feature = "dev-simulation")]
    Simulated,
}

enum Backend {
    Real,
    #[cfg(feature = "dev-simulation")]
    Simulated(crate::simulation::Simulation),
    #[cfg(feature = "dev-simulation")]
    SimulatedUnavailable,
}

struct Owner(Arc<Core>);

impl Drop for Owner {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.stopping = true;
        self.0.changed.notify_one();
    }
}

struct Core {
    subject: Arc<()>,
    registration: Option<Uuid>,
    state: Mutex<State>,
    changed: Condvar,
    backend: Backend,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Job {
    Restore,
    Add,
    Switch(Uuid),
    Refresh(Uuid),
    Remove(Uuid),
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Ticket {
    generation: u64,
    job: Job,
}

struct BrowserPage {
    address: String,
    deadline: Instant,
    code: String,
}

struct State {
    status: AccountState,
    message: String,
    ticket: Option<Ticket>,
    pending: Option<Ticket>,
    generation: u64,
    stopping: bool,
    committing: bool,
    retry_removals: BTreeSet<Uuid>,
    browser: Option<BrowserPage>,
    saved: SavedAccounts,
    active: Option<Uuid>,
    live: BTreeMap<Uuid, AuthenticatedAccount>,
    entitlement_denials: BTreeSet<Uuid>,
}

const SIGNED_OUT: &str = "Sign in with the Microsoft account that owns Minecraft: Java Edition.";
const SIGNED_IN: &str = "Minecraft: Java Edition account verified.";
const VAULT_UNAVAILABLE: &str =
    "Secure credential storage is unavailable. Sign-in can continue for this session.";
const VAULT_WRITE_FAILED: &str =
    "Signed in for this session. Secure storage did not save this account.";
const VAULT_CHANGED: &str =
    "The saved Microsoft account changed or could not be confirmed. Sign in again.";
const ACCOUNT_REMOVED: &str = "The Microsoft account was removed from Carbon Desktop.";
const NEED_VERIFY: &str = "Verify this Microsoft account to check its Java Edition entitlement.";
const ADD_LIMIT: &str = "Remove a Microsoft account before adding another one.";

impl State {
    fn new() -> Self {
        Self {
            status: AccountState::SignedOut,
            message: SIGNED_OUT.into(),
            ticket: None,
            pending: None,
            generation: 0,
            stopping: false,
            committing: false,
            retry_removals: BTreeSet::new(),
            browser: None,
            saved: SavedAccounts::default(),
            active: None,
            live: BTreeMap::new(),
            entitlement_denials: BTreeSet::new(),
        }
    }

    fn schedule(&mut self, job: Job, message: &str) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("account revision exhausted");
        let ticket = Ticket {
            generation: self.generation,
            job,
        };
        self.ticket = Some(ticket);
        self.pending = Some(ticket);
        self.browser = None;
        self.committing = false;
        self.status = AccountState::Starting;
        self.message = message.into();
    }

    fn finish(&mut self, status: AccountState, message: impl Into<String>) {
        self.ticket = None;
        self.pending = None;
        self.browser = None;
        self.committing = false;
        self.status = status;
        self.message = message.into();
    }

    fn current(&self, ticket: Ticket) -> bool {
        !self.stopping && self.ticket == Some(ticket)
    }

    fn cancellable(&self) -> bool {
        !self.committing
            && self
                .ticket
                .is_some_and(|ticket| !matches!(ticket.job, Job::Remove(_)))
    }

    fn verified(&self, id: Uuid) -> bool {
        self.live
            .get(&id)
            .is_some_and(AuthenticatedAccount::is_current)
    }

    fn knows(&self, id: Uuid) -> bool {
        self.saved.includes(id) || self.live.contains_key(&id)
    }

    fn stable_status(&self) -> AccountState {
        if self.active.is_some_and(|id| self.verified(id)) {
            AccountState::SignedIn
        } else {
            AccountState::SignedOut
        }
    }

    fn replace_saved(&mut self, saved: SavedAccounts) {
        self.live
            .retain(|id, _| !self.saved.includes(*id) || saved.includes(*id));
        self.entitlement_denials
            .retain(|id| saved.includes(*id) || self.live.contains_key(id));
        self.saved = saved;
    }

    fn view(&self) -> AccountView {
        let mut accounts: Vec<_> = self
            .saved
            .accounts
            .iter()
            .map(|profile| AccountSummary {
                id: profile.id,
                name: profile.name.clone(),
                verified: self.verified(profile.id),
            })
            .collect();
        accounts.extend(
            self.live
                .iter()
                .filter(|(id, _)| !self.saved.includes(**id))
                .map(|(id, account)| AccountSummary {
                    id: *id,
                    name: account.profile().name.clone(),
                    verified: account.is_current(),
                }),
        );
        let name = accounts
            .iter()
            .find(|profile| Some(profile.id) == self.active)
            .map(|profile| profile.name.clone());
        let (state, message) = if self.status == AccountState::SignedIn
            && self.stable_status() != AccountState::SignedIn
        {
            (AccountState::SignedOut, NEED_VERIFY)
        } else {
            (self.status, self.message.as_str())
        };
        AccountView {
            state,
            message: message.into(),
            name,
            user_code: self
                .browser
                .as_ref()
                .filter(|page| Instant::now() < page.deadline)
                .map(|page| page.code.clone()),
            accounts,
            active_account: self.active,
            cancellable: self.cancellable(),
            removal_retry: self
                .active
                .filter(|id| self.retry_removals.contains(id))
                .or_else(|| self.retry_removals.first().copied()),
        }
    }
}

impl Core {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn launch_selection<'a>(&'a self, state: &State) -> Selection<'a> {
        Selection {
            owner: &self.subject,
            revision: state.generation,
            registration: self.registration,
            profile: state.active,
            online_verified: state.active.is_some_and(|id| state.verified(id)),
            entitlement_denied: state
                .active
                .is_some_and(|id| state.entitlement_denials.contains(&id)),
            pending: state.ticket.is_some() || state.stopping,
            removal_pending: state
                .active
                .is_some_and(|id| state.retry_removals.contains(&id)),
        }
    }

    fn device_client(&self, client_id: Uuid) -> Result<DeviceClient, AccountError> {
        match &self.backend {
            Backend::Real => DeviceClient::new(client_id),
            #[cfg(feature = "dev-simulation")]
            Backend::Simulated(simulation) => {
                DeviceClient::with_transport(client_id, simulation.transport())
            }
            #[cfg(feature = "dev-simulation")]
            Backend::SimulatedUnavailable => Err(AccountError::TemporarilyUnavailable),
        }
    }

    fn is_simulated(&self) -> bool {
        #[cfg(feature = "dev-simulation")]
        {
            matches!(
                self.backend,
                Backend::Simulated(_) | Backend::SimulatedUnavailable
            )
        }
        #[cfg(not(feature = "dev-simulation"))]
        {
            false
        }
    }

    fn cancel_simulation(&self) {
        #[cfg(feature = "dev-simulation")]
        if let Backend::Simulated(simulation) = &self.backend {
            simulation.cancel_pending();
        }
    }

    fn complete(&self, ticket: Ticket, status: AccountState, message: impl Into<String>) {
        let mut state = self.state();
        if state.current(ticket) {
            state.finish(status, message);
        }
    }

    fn wait(&self, ticket: Ticket, delay: Duration) -> bool {
        let (state, _) = self
            .changed
            .wait_timeout_while(self.state(), delay, |state| state.current(ticket))
            .unwrap_or_else(|poison| poison.into_inner());
        state.current(ticket)
    }

    /// Linearize an authenticated result before performing any vault mutation.
    /// Cancellation remains available during network and browser authorization.
    fn commit(&self, ticket: Ticket) -> bool {
        let mut state = self.state();
        if !state.current(ticket) {
            return false;
        }
        state.browser = None;
        state.committing = true;
        state.status = AccountState::Starting;
        state.message = "Finishing the Microsoft account change.".into();
        true
    }
}

impl Accounts {
    pub fn new(root: &Path) -> Self {
        Self::start(Some(root), configured_client())
    }

    pub fn session_only() -> Self {
        Self::start(None, configured_client())
    }

    pub fn snapshot(&self) -> AccountView {
        self.owner.0.state().view()
    }

    fn start(root: Option<&Path>, compiled: Option<Uuid>) -> Self {
        let root = root.map(Path::to_path_buf);
        let custom = root.as_deref().and_then(client::load);
        let selection = resolve_client(compiled, custom);
        let client_id = selection.map(|(client_id, _)| client_id);
        let source = selection.map(|(_, source)| source);
        Self::start_backend(root, compiled, client_id, source, Backend::Real)
    }

    /// Start the real lifecycle with the deterministic in-process provider.
    /// Compiled only with the non-default `dev-simulation` feature and called
    /// only after the GPUI developer startup gates accept a fixture root.
    #[cfg(feature = "dev-simulation")]
    pub fn simulated(root: &Path, simulation: crate::simulation::Simulation) -> Self {
        if !crate::simulation::root_is_marked(root) {
            let accounts = Self::start_backend(
                Some(root.to_path_buf()),
                None,
                None,
                None,
                Backend::SimulatedUnavailable,
            );
            accounts.owner.0.state().finish(
                AccountState::Unavailable,
                "The developer simulation root is not marked. Start it through the guarded GPUI developer flag.",
            );
            return accounts;
        }
        Self::start_backend(
            Some(root.to_path_buf()),
            None,
            Some(crate::simulation::CLIENT_ID),
            Some(RegistrationSource::Simulated),
            Backend::Simulated(simulation),
        )
    }

    fn start_backend(
        root: Option<PathBuf>,
        compiled: Option<Uuid>,
        client_id: Option<Uuid>,
        source: Option<RegistrationSource>,
        backend: Backend,
    ) -> Self {
        let mut state = State::new();
        match client_id {
            None => state.finish(
                AccountState::Unavailable,
                "Minecraft account sign-in is not configured. Use account setup or the Minecraft Launcher.",
            ),
            Some(_) if root.is_some() => {
                state.schedule(Job::Restore, "Checking for saved Minecraft accounts.")
            }
            Some(_) => {}
        };
        let core = Arc::new(Core {
            subject: Arc::new(()),
            registration: client_id,
            state: Mutex::new(state),
            changed: Condvar::new(),
            backend,
        });
        if let Some(client_id) = client_id {
            let worker = Arc::clone(&core);
            let worker_root = root.clone();
            if thread::Builder::new()
                .name("carbon-accounts".into())
                .spawn(move || run(&worker, worker_root.as_deref(), client_id))
                .is_err()
            {
                core.state().finish(
                    AccountState::Unavailable,
                    "The sign-in worker could not start.",
                );
            }
        }
        Self {
            owner: Arc::new(Owner(core)),
            root,
            compiled: compiled.filter(|client_id| !client_id.is_nil()),
            source,
        }
    }

    /// True for the opt-in developer provider. Production builds cannot reach
    /// this state because the feature and startup gates are both required.
    pub fn is_simulated(&self) -> bool {
        #[cfg(feature = "dev-simulation")]
        {
            matches!(
                self.owner.0.backend,
                Backend::Simulated(_) | Backend::SimulatedUnavailable
            )
        }
        #[cfg(not(feature = "dev-simulation"))]
        {
            false
        }
    }

    /// Developer controls for the simulated provider, when this instance uses it.
    #[cfg(feature = "dev-simulation")]
    pub fn simulation(&self) -> Option<&crate::simulation::Simulation> {
        match &self.owner.0.backend {
            Backend::Simulated(simulation) => Some(simulation),
            Backend::Real | Backend::SimulatedUnavailable => None,
        }
    }

    /// A compiled Carbon configuration always takes precedence over local opt-in.
    pub fn compiled_client_configured(&self) -> bool {
        self.compiled.is_some()
    }

    /// Per-install configuration is offered only when this build has no compiled ID.
    pub fn custom_client_allowed(&self) -> bool {
        self.compiled.is_none() && self.root.is_some() && !self.is_simulated()
    }

    /// The worker currently running in this process was started from the local setting.
    pub fn custom_client_active(&self) -> bool {
        self.source == Some(RegistrationSource::Custom)
    }

    /// A public ID is stored for the next launch. This does not imply it is in use yet.
    pub fn custom_client_configured(&self) -> bool {
        self.custom_client_allowed() && self.root.as_deref().and_then(client::load).is_some()
    }

    /// The stored opt-in differs from the registration used by the running worker.
    pub fn custom_client_pending_restart(&self) -> bool {
        self.compiled.is_none()
            && !self.is_simulated()
            && self.root.as_deref().and_then(client::load) != self.owner.0.registration
    }

    /// Validate and persist only a public application ID. Errors never echo input.
    pub fn set_custom_client(&self, value: &str) -> Result<(), String> {
        if self.is_simulated() {
            return Err(
                "The developer simulation does not read or write a client setting.".to_owned(),
            );
        }
        let root = self
            .root
            .as_deref()
            .ok_or_else(|| "This session does not keep per-install account settings.".to_owned())?;
        if self.compiled.is_some() {
            return Err("This build already has a compiled client configuration.".to_owned());
        }
        let client_id = Uuid::parse_str(value.trim())
            .map_err(|_| "Enter a public client ID as a UUID.".to_owned())?;
        if client_id.is_nil() {
            return Err("Enter a non-zero public client ID.".to_owned());
        }
        client::store(root, client_id)
            .map_err(|_| "The public client ID could not be saved.".to_owned())
    }

    /// Forget the local opt-in without touching profiles, saves or vault entries.
    pub fn clear_custom_client(&self) -> Result<(), String> {
        if self.is_simulated() {
            return Err(
                "The developer simulation does not read or write a client setting.".to_owned(),
            );
        }
        let root = self
            .root
            .as_deref()
            .ok_or_else(|| "This session does not keep per-install account settings.".to_owned())?;
        if self.compiled.is_some() {
            return Err("This build already has a compiled client configuration.".to_owned());
        }
        client::clear(root)
            .map_err(|_| "The per-install client setting could not be cleared.".to_owned())
    }

    /// Reports a fixed, redacted reason for this build's unavailable play mode.
    pub fn play_blocker(&self, mode: PlayMode) -> PlayBlocker {
        if self.is_simulated() {
            return PlayBlocker::SimulationUnavailable;
        }
        let core = &self.owner.0;
        let state = core.state();
        play::missing_evidence(core.launch_selection(&state), mode)
    }

    /// Requests a process-local typed decision from trusted account evidence.
    /// Provider-approved offline and scoped online evidence are unavailable in this build.
    pub fn prepare_launch(
        &self,
        mode: PlayMode,
        installation: Option<Installation>,
    ) -> Result<LaunchDecision, PlayBlocker> {
        if self.is_simulated() {
            return Err(PlayBlocker::SimulationUnavailable);
        }
        let core = &self.owner.0;
        let state = core.state();
        play::issue(
            core.launch_selection(&state),
            mode,
            installation,
            None,
            Clock::now(),
        )
    }

    /// Runs the final bounded native spawn admission under the account state lock.
    /// The runner must already hold the exact protected installation lease. Keep
    /// blocking waits, downloads, reentrant account calls, and IPC outside this callback.
    pub fn with_launch_authority<T>(
        &self,
        decision: LaunchDecision,
        installation: Option<Installation>,
        start: impl FnOnce() -> T,
    ) -> Result<T, PlayBlocker> {
        if self.is_simulated() {
            return Err(PlayBlocker::SimulationUnavailable);
        }
        let core = &self.owner.0;
        let state = core.state();
        play::admit(
            decision,
            core.launch_selection(&state),
            installation,
            None,
            Clock::now(),
            start,
        )
    }

    pub fn begin(&self) {
        let core = &self.owner.0;
        let mut state = core.state();
        if state.ticket.is_some() || state.status == AccountState::Unavailable {
            return;
        }
        if state.view().accounts.len() < ACCOUNT_LIMIT {
            state.schedule(Job::Add, "Starting Microsoft sign-in.");
        } else {
            state.finish(AccountState::Error, ADD_LIMIT);
            return;
        }
        core.changed.notify_one();
    }

    pub fn cancel(&self) {
        let core = &self.owner.0;
        let mut state = core.state();
        if state.cancellable() {
            core.cancel_simulation();
            let status = state.stable_status();
            state.finish(status, AccountError::Cancelled.to_string());
            core.changed.notify_one();
        }
    }

    pub fn switch(&self, id: Uuid) {
        self.profile_job(Some(id), Job::Switch, "Switching Microsoft accounts.");
    }

    pub fn refresh_selected(&self) {
        self.profile_job(None, Job::Refresh, "Verifying the Microsoft account.");
    }

    pub fn sign_out(&self) {
        self.profile_job(None, Job::Remove, "Removing the Microsoft account.");
    }

    pub fn sign_out_account(&self, id: Uuid) {
        self.profile_job(Some(id), Job::Remove, "Removing the Microsoft account.");
    }

    fn profile_job(&self, id: Option<Uuid>, job: impl FnOnce(Uuid) -> Job, message: &str) {
        let core = &self.owner.0;
        let mut state = core.state();
        if state.ticket.is_some() {
            return;
        }
        if let Some(id) = id.or(state.active).filter(|id| state.knows(*id)) {
            state.schedule(job(id), message);
            core.changed.notify_one();
        }
    }

    pub fn open_browser(&self) -> Result<(), String> {
        if self.is_simulated() {
            return Err(
                "The developer simulation never opens a real Microsoft browser page. Use the simulated approval controls."
                    .to_owned(),
            );
        }
        let address = self
            .owner
            .0
            .state()
            .browser
            .as_ref()
            .filter(|page| Instant::now() < page.deadline)
            .map(|page| page.address.clone())
            .ok_or_else(|| {
                "There is no current Microsoft sign-in page. Start sign-in again.".to_owned()
            })?;
        // DeviceClient validates the URI before the worker publishes it.
        open::that_detached(address).map_err(|_| {
            "The system browser could not open. Try again or use Microsoft's device sign-in page.".to_owned()
        })
    }
}

fn configured_client() -> Option<Uuid> {
    option_env!("CARBON_MICROSOFT_CLIENT_ID")
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|id| !id.is_nil())
}

fn resolve_client(
    compiled: Option<Uuid>,
    custom: Option<Uuid>,
) -> Option<(Uuid, RegistrationSource)> {
    compiled
        .filter(|client_id| !client_id.is_nil())
        .map(|client_id| (client_id, RegistrationSource::Compiled))
        .or_else(|| {
            custom
                .filter(|client_id| !client_id.is_nil())
                .map(|client_id| (client_id, RegistrationSource::Custom))
        })
}

pub(crate) const MINECRAFT_DOWNLOAD_URL: &str = "https://www.minecraft.net/en-us/download";

/// Open only the official Minecraft download page on explicit user action.
pub fn open_minecraft_download() -> Result<(), String> {
    open::that_detached(MINECRAFT_DOWNLOAD_URL)
        .map_err(|_| "The system browser could not open the Minecraft download page.".to_owned())
}

fn run(core: &Core, root: Option<&Path>, client_id: Uuid) {
    let vault = root.map(|root| match &core.backend {
        Backend::Real => Vault::new(root, client_id),
        #[cfg(feature = "dev-simulation")]
        Backend::Simulated(_) | Backend::SimulatedUnavailable => Vault::simulated(root, client_id),
    });
    let vault = vault.as_ref();
    loop {
        let ticket = {
            let mut state = core
                .changed
                .wait_while(core.state(), |state| {
                    !state.stopping && state.pending.is_none()
                })
                .unwrap_or_else(|poison| poison.into_inner());
            if state.stopping {
                return;
            }
            state.pending.take().expect("worker has a queued operation")
        };
        let refresh = |token: &str| {
            core.device_client(client_id)
                .and_then(|client| client.refresh(token, || !core.state().current(ticket)))
        };
        match ticket.job {
            Job::Restore => restore(core, ticket, vault, refresh),
            Job::Add => sign_in(core, ticket, client_id, vault),
            Job::Switch(id) => {
                if core.state().verified(id) {
                    select_current(core, ticket, id, vault);
                } else {
                    verify(core, ticket, id, vault, refresh);
                }
            }
            Job::Refresh(id) => verify(core, ticket, id, vault, refresh),
            Job::Remove(id) => remove(core, ticket, id, vault),
        }
    }
}

fn restore(
    core: &Core,
    ticket: Ticket,
    vault: Option<&Vault>,
    refresh: impl FnOnce(&str) -> Result<AuthenticatedAccount, AccountError>,
) {
    let saved = match vault.map_or_else(|| Ok(SavedAccounts::default()), Vault::load) {
        Ok(saved) => saved,
        Err(()) => {
            core.complete(ticket, AccountState::SignedOut, VAULT_UNAVAILABLE);
            return;
        }
    };
    let id = saved.active;
    {
        let mut state = core.state();
        if !state.current(ticket) {
            return;
        }
        state.active = id;
        state.replace_saved(saved);
    }
    if let Some(id) = id {
        verify(core, ticket, id, vault, refresh);
    } else {
        core.complete(ticket, AccountState::SignedOut, SIGNED_OUT);
    }
}

fn sign_in(core: &Core, ticket: Ticket, client_id: Uuid, vault: Option<&Vault>) {
    let cancelled = || !core.state().current(ticket);
    let result = (|| {
        if cancelled() {
            return Err(AccountError::Cancelled);
        }
        let client = core.device_client(client_id)?;
        let mut login = client.begin(cancelled)?;
        {
            let mut state = core.state();
            if !state.current(ticket) {
                return Err(AccountError::Cancelled);
            }
            let challenge = login.challenge();
            state.browser = Some(BrowserPage {
                address: challenge.verification_uri.clone(),
                deadline: login.deadline(),
                code: challenge.user_code.clone(),
            });
            state.status = AccountState::AwaitingBrowser;
            state.message = if core.is_simulated() {
                "Simulated authorization is pending. Use the approval controls in this dialog; no browser or real code is used."
                    .into()
            } else {
                "Open the browser and enter this code to authorize Carbon.".into()
            };
        }
        loop {
            match client.poll(&mut login, cancelled)? {
                DevicePoll::Pending { retry_after } if core.wait(ticket, retry_after) => {}
                DevicePoll::Pending { .. } => return Err(AccountError::Cancelled),
                DevicePoll::Authenticated(account) => return Ok(account),
            }
        }
    })();
    match result {
        Ok(account) => add(core, ticket, account, vault),
        Err(error) => {
            if error == AccountError::Cancelled {
                core.cancel_simulation();
            }
            core.complete(ticket, AccountState::Error, error.to_string())
        }
    }
}

fn add(core: &Core, ticket: Ticket, account: AuthenticatedAccount, vault: Option<&Vault>) {
    if !core.commit(ticket) {
        return;
    }
    let persisted = vault.and_then(|vault| {
        let token = account.refresh_token()?;
        vault.add(account.profile().into(), token).ok()
    });
    let message = if vault.is_some() && persisted.is_none() {
        VAULT_WRITE_FAILED
    } else {
        SIGNED_IN
    };
    publish(core, ticket, account, persisted, message);
}

fn publish(
    core: &Core,
    ticket: Ticket,
    account: AuthenticatedAccount,
    persisted: Option<SavedAccounts>,
    message: &str,
) {
    let mut state = core.state();
    if !state.current(ticket) {
        return;
    }
    if let Some(persisted) = persisted {
        state.replace_saved(persisted);
    }
    let id = account.profile().id;
    state.active = Some(id);
    state.entitlement_denials.remove(&id);
    state.live.insert(id, account);
    let status = state.stable_status();
    let message = if core.is_simulated() {
        if message == VAULT_WRITE_FAILED {
            "Synthetic test account verified for this session. The test credential file was not saved."
        } else {
            "Synthetic test account verified."
        }
    } else {
        message
    };
    state.finish(status, message);
}

fn select_current(core: &Core, ticket: Ticket, id: Uuid, vault: Option<&Vault>) {
    if !core.commit(ticket) {
        return;
    }
    let persisted = if core.state().saved.includes(id) {
        match vault.and_then(|vault| vault.select(id).ok().flatten()) {
            Some(saved) => Some(saved),
            None => {
                core.complete(ticket, AccountState::Error, VAULT_CHANGED);
                return;
            }
        }
    } else {
        None
    };
    let mut state = core.state();
    if state.current(ticket) {
        if let Some(saved) = persisted {
            state.replace_saved(saved);
        }
        state.active = Some(id);
        let status = state.stable_status();
        state.finish(
            status,
            if status == AccountState::SignedIn {
                SIGNED_IN
            } else {
                NEED_VERIFY
            },
        );
    }
}

fn verify(
    core: &Core,
    ticket: Ticket,
    id: Uuid,
    vault: Option<&Vault>,
    refresh: impl FnOnce(&str) -> Result<AuthenticatedAccount, AccountError>,
) {
    let (is_saved, memory) = {
        let state = core.state();
        if !state.current(ticket) {
            return;
        }
        (
            state.saved.includes(id),
            state
                .live
                .get(&id)
                .and_then(AuthenticatedAccount::refresh_token)
                .map(|token| Secret::new(token.to_owned())),
        )
    };
    let vault = vault.filter(|_| is_saved);
    let token = match vault.map_or(Ok(memory), |vault| vault.read(id)) {
        Ok(Some(token)) => token,
        Ok(None) => {
            failed_verification(core, ticket, id, vault, None, false, NEED_VERIFY);
            return;
        }
        Err(()) => {
            failed_verification(core, ticket, id, vault, None, false, VAULT_UNAVAILABLE);
            return;
        }
    };
    if !core.state().current(ticket) {
        return;
    }
    let result = refresh(&token).and_then(|account| {
        if account.profile().id == id {
            Ok(account)
        } else {
            Err(AccountError::InvalidResponse)
        }
    });
    match result {
        Ok(account) => {
            if !core.commit(ticket) {
                return;
            }
            let saved = if let Some(vault) = vault {
                let next = account.refresh_token().unwrap_or(&token);
                let required_active = (ticket.job == Job::Restore).then_some(id);
                match vault.update(
                    account.profile().into(),
                    &token,
                    next,
                    true,
                    required_active,
                ) {
                    Ok(Some(saved)) => Some(saved),
                    _ => {
                        failed_verification(core, ticket, id, None, None, false, VAULT_CHANGED);
                        return;
                    }
                }
            } else {
                None
            };
            publish(core, ticket, account, saved, SIGNED_IN);
        }
        Err(error) => {
            let rejected = (error == AccountError::RefreshRejected).then_some(token.as_str());
            let denied = matches!(
                error,
                AccountError::NotEntitled | AccountError::RefreshRejected
            );
            failed_verification(
                core,
                ticket,
                id,
                vault,
                rejected,
                denied,
                &error.to_string(),
            );
        }
    }
}

fn failed_verification(
    core: &Core,
    ticket: Ticket,
    id: Uuid,
    vault: Option<&Vault>,
    rejected: Option<&str>,
    denied: bool,
    message: &str,
) {
    if !core.commit(ticket) {
        return;
    }
    let saved = if let Some(vault) = vault {
        if let Some(rejected) = rejected {
            vault.remove(id, Some(rejected)).ok().flatten()
        } else if matches!(ticket.job, Job::Switch(_)) {
            vault.select(id).ok().flatten()
        } else {
            None
        }
    } else {
        None
    };
    let mut state = core.state();
    if state.current(ticket) {
        if denied {
            state.entitlement_denials.insert(id);
        }
        // Keep a session-only row for retry after transient failures, with its verification invalidated.
        if rejected.is_some() || state.saved.includes(id) {
            state.live.remove(&id);
        } else if let Some(account) = state.live.get_mut(&id) {
            account.invalidate_verification();
        }
        if let Some(saved) = saved {
            state.replace_saved(saved);
        }
        state.active = if state.knows(id) {
            Some(id)
        } else {
            state
                .saved
                .active
                .or_else(|| state.live.keys().next().copied())
        };
        state.finish(AccountState::Error, message);
    }
}

fn remove(core: &Core, ticket: Ticket, id: Uuid, vault: Option<&Vault>) {
    let saved = if core.state().saved.includes(id) {
        vault.map(|vault| vault.remove(id, None)).unwrap_or(Err(()))
    } else {
        Ok(None)
    };
    let mut state = core.state();
    if !state.current(ticket) {
        return;
    }
    state.live.remove(&id);
    match saved {
        Ok(saved) => {
            state.entitlement_denials.remove(&id);
            state.retry_removals.remove(&id);
            if let Some(saved) = saved {
                state.replace_saved(saved);
            }
            if state.active.is_some_and(|active| !state.knows(active)) {
                state.active = state
                    .saved
                    .active
                    .or_else(|| state.live.keys().next().copied());
            }
            let status = state.stable_status();
            let message = if core.is_simulated() {
                "The synthetic test account was removed from Carbon Desktop."
            } else {
                ACCOUNT_REMOVED
            };
            state.finish(status, message);
        }
        Err(()) => {
            state.retry_removals.insert(id);
            state.finish(
                AccountState::Error,
                "Secure storage could not remove this account. Try again.",
            );
        }
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
