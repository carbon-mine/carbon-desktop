// SPDX-License-Identifier: MPL-2.0
//! Process-wide directory of live protected sample runs.
//!
//! Each entry pairs a run UUID with the one OS job that owns its process tree.
//! The owning runner keeps the entry for as long as it may still act on the
//! run; a stop request terminates only that entry's job, so stopping run A
//! cannot affect a later run B or another instance's run.
use super::job;
use super::Handle;
use crate::{RunControlError, RunEnd, RunOutcome, RunPhase, RunState, RunSubscription, RunUpdate};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex, MutexGuard, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// How long a stop request lets the run's own windows close before the launcher
/// forces the remaining process tree down.
const CLOSE_GRACE: Duration = Duration::from_secs(5);
/// A window may not exist yet when the stop is accepted, so the request is
/// repeated while the tree is still alive.
const CLOSE_INTERVAL: Duration = Duration::from_millis(500);
const DRAIN_POLL: Duration = Duration::from_millis(250);
/// Recent terminal records kept for a late observer. Bounded so an unattended
/// process cannot grow one entry per run without limit.
const RETAINED_OUTCOMES: usize = 32;

struct Status {
    phase: RunPhase,
    /// The owner claimed completion after observing an empty tree.
    completed: bool,
    /// The owner has retired the run; no stop may claim it any more.
    retired: bool,
}

struct Entry {
    instance: Uuid,
    status: Mutex<Status>,
    job: Arc<Handle>,
    subscribers: Mutex<Vec<Sender<RunUpdate>>>,
}

fn directory() -> &'static Mutex<BTreeMap<Uuid, Arc<Entry>>> {
    static DIRECTORY: OnceLock<Mutex<BTreeMap<Uuid, Arc<Entry>>>> = OnceLock::new();
    DIRECTORY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn entries() -> MutexGuard<'static, BTreeMap<Uuid, Arc<Entry>>> {
    directory()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn outcomes() -> &'static Mutex<VecDeque<RunOutcome>> {
    static OUTCOMES: OnceLock<Mutex<VecDeque<RunOutcome>>> = OnceLock::new();
    OUTCOMES.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn phase(entry: &Entry) -> RunPhase {
    lock_status(entry).phase
}

fn lock_status(entry: &Entry) -> MutexGuard<'_, Status> {
    entry
        .status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_subscribers(entry: &Entry) -> MutexGuard<'_, Vec<Sender<RunUpdate>>> {
    entry
        .subscribers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn state(run: Uuid, entry: &Entry, status: &Status) -> RunState {
    RunState {
        run,
        instance: entry.instance,
        phase: status.phase,
    }
}

/// Publishes one update to every live subscriber while the caller holds the
/// status lock, so phase order and the terminal update cannot be reordered.
fn publish(entry: &Entry, update: &RunUpdate) {
    lock_subscribers(entry).retain(|sender| sender.send(update.clone()).is_ok());
}

fn set_phase(run: Uuid, entry: &Entry, status: &mut Status, phase: RunPhase) {
    status.phase = phase;
    publish(
        entry,
        &RunUpdate::State(RunState {
            run,
            instance: entry.instance,
            phase,
        }),
    );
}

fn retain(outcome: RunOutcome) {
    retain_in(
        &mut outcomes()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        outcome,
    );
}

fn retain_in(outcomes: &mut VecDeque<RunOutcome>, outcome: RunOutcome) {
    if outcomes.len() >= RETAINED_OUTCOMES {
        outcomes.pop_front();
    }
    outcomes.push_back(outcome);
}

/// Removes its run from the directory when the owning runner finishes,
/// including on an error or panic path. Closing the last job handle also
/// destroys any tree the run never drained.
pub(super) struct RunGuard {
    run: Uuid,
    entry: Arc<Entry>,
}

impl RunGuard {
    /// Admission has resumed the root process.
    pub(super) fn running(&self) {
        let mut status = lock_status(&self.entry);
        if status.phase == RunPhase::Starting {
            set_phase(self.run, &self.entry, &mut status, RunPhase::Running);
        }
    }

    /// The owner observed an empty tree and claims the completed outcome.
    /// Returns false when a stop request claimed the run first, so a later
    /// request cannot rewrite an outcome the owner already decided.
    pub(super) fn finish(&self) -> bool {
        let mut status = lock_status(&self.entry);
        if matches!(
            status.phase,
            RunPhase::Stopping | RunPhase::Draining | RunPhase::Finished
        ) {
            return false;
        }
        status.completed = true;
        set_phase(self.run, &self.entry, &mut status, RunPhase::Finishing);
        true
    }

    /// Withdraws a completion claim when the run failed after the empty tree was
    /// observed, so the terminal record reports an unsettled outcome instead of
    /// a sample the caller never received.
    pub(super) fn unsettle(&self) {
        lock_status(&self.entry).completed = false;
    }

    /// True when a stop request was observed before the run finished.
    pub(super) fn stopped(&self) -> bool {
        matches!(phase(&self.entry), RunPhase::Stopping | RunPhase::Draining)
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        {
            let mut status = lock_status(&self.entry);
            status.retired = true;
            let end = if matches!(status.phase, RunPhase::Stopping | RunPhase::Draining) {
                RunEnd::StopRequested
            } else if status.completed {
                RunEnd::Completed
            } else {
                RunEnd::Unsettled
            };
            status.phase = RunPhase::Finished;
            let outcome = RunOutcome {
                run: self.run,
                instance: self.entry.instance,
                end,
            };
            // Retain and notify while the status lock still proves the entry is
            // live, so a subscriber that races registration cannot miss the
            // terminal update.
            retain(outcome.clone());
            for sender in lock_subscribers(&self.entry).drain(..) {
                let _ = sender.send(RunUpdate::Finished(outcome.clone()));
            }
        }
        let mut directory = entries();
        if directory
            .get(&self.run)
            .is_some_and(|entry| Arc::ptr_eq(entry, &self.entry))
        {
            directory.remove(&self.run);
        }
    }
}

pub(super) fn register(run: Uuid, instance: Uuid, job: Arc<Handle>) -> RunGuard {
    let entry = Arc::new(Entry {
        instance,
        status: Mutex::new(Status {
            phase: RunPhase::Starting,
            completed: false,
            retired: false,
        }),
        job,
        subscribers: Mutex::new(Vec::new()),
    });
    let replaced = entries().insert(run, Arc::clone(&entry));
    debug_assert!(replaced.is_none(), "a run UUID is only registered once");
    RunGuard { run, entry }
}

pub(super) fn inspect(run: Uuid) -> Option<RunState> {
    let entry = entries().get(&run).cloned()?;
    let status = lock_status(&entry);
    Some(state(run, &entry, &status))
}

pub(super) fn active() -> Vec<RunState> {
    entries()
        .iter()
        .map(|(run, entry)| state(*run, entry, &lock_status(entry)))
        .collect()
}

pub(super) fn outcome(run: Uuid) -> Option<RunOutcome> {
    outcomes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|outcome| outcome.run == run)
        .cloned()
}

/// Registers one observer without changing the run. A run that already retired
/// yields its retained terminal record instead of a live phase stream.
pub(super) fn subscribe(run: Uuid) -> Result<RunSubscription, RunControlError> {
    if let Some(entry) = entries().get(&run).cloned() {
        let (sender, receiver) = mpsc::channel();
        {
            let status = lock_status(&entry);
            if !status.retired {
                let current = state(run, &entry, &status);
                // Registration and the first update happen under the status
                // lock, so no later transition can overtake the current phase.
                lock_subscribers(&entry).push(sender.clone());
                let _ = sender.send(RunUpdate::State(current));
                return Ok(RunSubscription { receiver });
            }
        }
    }
    if let Some(outcome) = outcome(run) {
        let (sender, receiver) = mpsc::channel();
        let _ = sender.send(RunUpdate::Finished(outcome));
        return Ok(RunSubscription { receiver });
    }
    Err(RunControlError::UnknownRun)
}

/// Requests termination of one run's own process tree. A repeated request while
/// the run is still draining reports the same phase and starts no new bound.
///
/// The OS close request goes to every visible top-level window the tree owns, so
/// a game can save and exit normally. Only a tree that has not drained when the
/// bound expires is terminated.
pub(super) fn stop(run: Uuid) -> Result<RunState, RunControlError> {
    stop_after(run, CLOSE_GRACE)
}

fn stop_after(run: Uuid, grace: Duration) -> Result<RunState, RunControlError> {
    let entry = entries()
        .get(&run)
        .cloned()
        .ok_or(RunControlError::UnknownRun)?;
    // Claiming the run is one atomic phase transition, so exactly one request
    // sends the close and starts the bound. The lock is released before any
    // window enumeration or wait.
    let claimed = {
        let mut status = lock_status(&entry);
        if status.retired {
            false
        } else if matches!(status.phase, RunPhase::Starting | RunPhase::Running) {
            set_phase(run, &entry, &mut status, RunPhase::Stopping);
            true
        } else {
            false
        }
    };
    if claimed {
        request_close(&entry);
        let mut status = lock_status(&entry);
        if status.phase == RunPhase::Stopping {
            set_phase(run, &entry, &mut status, RunPhase::Draining);
        }
        drop(status);
        let escalation = Arc::clone(&entry);
        thread::spawn(move || escalate(&escalation, grace));
    }
    let status = lock_status(&entry);
    Ok(state(run, &entry, &status))
}

fn request_close(entry: &Entry) {
    if let Err(error) = job::request_close(&entry.job) {
        // A missing request is not a failed stop: the bound still forces the
        // tree down. Only the error kind is reported; no path or identity.
        eprintln!(
            "Could not ask the protected run to close ({:?}).",
            error.kind()
        );
    }
}

/// Waits out one close bound for a run this process already asked to stop, then
/// terminates what remains. The owner still performs the only drain
/// observation and record cleanup.
fn escalate(entry: &Entry, grace: Duration) {
    let deadline = Instant::now() + grace;
    let mut next_request = Instant::now() + CLOSE_INTERVAL;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if job::wait_empty(&entry.job, remaining.min(DRAIN_POLL)).is_ok() {
            return;
        }
        if !matches!(phase(entry), RunPhase::Stopping | RunPhase::Draining) {
            return;
        }
        if Instant::now() >= next_request {
            request_close(entry);
            next_request = Instant::now() + CLOSE_INTERVAL;
        }
    }
    if !matches!(phase(entry), RunPhase::Stopping | RunPhase::Draining) {
        return;
    }
    if let Err(error) = job::terminate(&entry.job) {
        eprintln!(
            "Could not force the stopped protected run to drain ({:?}).",
            error.kind()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs, thread,
        time::{Duration, Instant},
    };

    fn instance() -> Uuid {
        Uuid::new_v4()
    }

    fn wait_drained(run: Uuid) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !job::boundary_empty(run).unwrap() {
            assert!(Instant::now() < deadline, "The run did not drain");
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Most tests only need the escalation outcome. The production close bound
    /// is covered separately.
    fn stop_soon(run: Uuid) -> Result<RunState, RunControlError> {
        stop_after(run, Duration::ZERO)
    }

    #[test]
    fn stopping_one_run_leaves_another_live() {
        let root = tempfile::tempdir().unwrap();
        let first_run = Uuid::new_v4();
        let second_run = Uuid::new_v4();
        let first = register(
            first_run,
            instance(),
            job::test_boundary(first_run, root.path()).unwrap(),
        );
        first.running();
        let second = register(
            second_run,
            instance(),
            job::test_boundary(second_run, root.path()).unwrap(),
        );
        second.running();

        assert_eq!(inspect(first_run).unwrap().phase, RunPhase::Running);
        assert_eq!(stop_soon(first_run).unwrap().phase, RunPhase::Draining);
        wait_drained(first_run);
        assert!(!job::boundary_empty(second_run).unwrap());
        assert_eq!(inspect(second_run).unwrap().phase, RunPhase::Running);

        // The unrelated run stays under its owner's control.
        assert_eq!(stop_soon(second_run).unwrap().phase, RunPhase::Draining);
        wait_drained(second_run);
    }

    #[test]
    fn a_repeated_stop_reports_the_drain_without_signalling_again() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        wait_drained(run);
        assert!(guard.stopped());
        // A stop claims the outcome, so the owner cannot later report completion.
        assert!(!guard.finish());
    }

    #[test]
    fn a_stop_leaves_time_for_the_run_to_close_itself() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        assert_eq!(stop(run).unwrap().phase, RunPhase::Draining);
        // The fixture has no window, so only the production bound can end it.
        thread::sleep(Duration::from_millis(200));
        assert!(
            !job::boundary_empty(run).unwrap(),
            "the tree must not be forced down before the close bound"
        );
        wait_drained(run);
        assert!(guard.stopped());
    }

    #[test]
    fn a_stop_accepted_before_admission_survives_the_running_transition() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        assert_eq!(inspect(run).unwrap().phase, RunPhase::Starting);
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        // Admission must not rewrite the accepted stop as a running run.
        guard.running();
        assert_eq!(inspect(run).unwrap().phase, RunPhase::Draining);
        assert!(guard.stopped());
        wait_drained(run);
    }

    #[test]
    fn a_stop_after_the_owner_finished_leaves_the_outcome_alone() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        // The fixture child exits on its own; the owner then observes an empty tree.
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        assert!(guard.finish());
        let late = stop(run).unwrap();
        assert_eq!(late.phase, RunPhase::Finishing);
        assert!(!guard.stopped());
    }

    #[test]
    fn an_unregistered_run_cannot_be_stopped() {
        let run = Uuid::new_v4();
        assert!(inspect(run).is_none());
        assert!(matches!(stop(run), Err(RunControlError::UnknownRun)));
        assert!(active().iter().all(|state| state.run != run));
    }

    #[test]
    fn the_owner_leaving_retires_the_live_entry() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        assert!(active().iter().any(|state| state.run == run));
        drop(guard);
        assert!(inspect(run).is_none());
        assert!(!active().iter().any(|state| state.run == run));
        assert!(matches!(stop(run), Err(RunControlError::UnknownRun)));
        wait_drained(run);
    }

    #[test]
    fn the_directory_reports_only_redacted_state() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let instance = instance();
        let guard = register(run, instance, job::test_boundary(run, root.path()).unwrap());
        assert_eq!(inspect(run).unwrap().phase, RunPhase::Starting);
        guard.running();
        let state = inspect(run).unwrap();
        assert_eq!(state.run, run);
        assert_eq!(state.instance, instance);
    }

    fn update(subscription: &RunSubscription) -> RunUpdate {
        subscription
            .recv_timeout(Duration::from_secs(5))
            .expect("the subscribed run should publish an update")
    }

    fn next_phase(subscription: &RunSubscription, expected: RunPhase) {
        match update(subscription) {
            RunUpdate::State(state) => assert_eq!(state.phase, expected),
            other => panic!("expected the {expected:?} phase, received {other:?}"),
        }
    }

    fn next_end(subscription: &RunSubscription, expected: RunEnd) {
        match update(subscription) {
            RunUpdate::Finished(outcome) => assert_eq!(outcome.end, expected),
            other => panic!("expected the {expected:?} outcome, received {other:?}"),
        }
    }

    #[test]
    fn a_subscriber_follows_a_run_through_completion() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        let subscription = subscribe(run).unwrap();
        next_phase(&subscription, RunPhase::Starting);
        guard.running();
        next_phase(&subscription, RunPhase::Running);
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        assert!(guard.finish());
        next_phase(&subscription, RunPhase::Finishing);
        drop(guard);
        next_end(&subscription, RunEnd::Completed);
        assert_eq!(outcome(run).unwrap().end, RunEnd::Completed);
        assert!(inspect(run).is_none());
        assert!(subscription
            .recv_timeout(Duration::from_millis(50))
            .is_err());
    }

    #[test]
    fn a_subscriber_sees_a_requested_stop() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        let subscription = subscribe(run).unwrap();
        next_phase(&subscription, RunPhase::Running);
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        next_phase(&subscription, RunPhase::Stopping);
        next_phase(&subscription, RunPhase::Draining);
        wait_drained(run);
        drop(guard);
        next_end(&subscription, RunEnd::StopRequested);
        assert_eq!(outcome(run).unwrap().end, RunEnd::StopRequested);
    }

    #[test]
    fn a_late_subscriber_gets_the_retained_outcome() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        drop(guard);
        assert!(inspect(run).is_none());
        let subscription = subscribe(run).unwrap();
        next_end(&subscription, RunEnd::Unsettled);
        assert_eq!(outcome(run).unwrap().end, RunEnd::Unsettled);
    }

    #[test]
    fn a_retracted_completion_claim_reports_an_unsettled_outcome() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        assert!(guard.finish());
        // The sample's exit could not be verified, so the caller received an
        // error and the terminal record must not claim a completion.
        guard.unsettle();
        drop(guard);
        assert_eq!(outcome(run).unwrap().end, RunEnd::Unsettled);
    }

    #[test]
    fn a_dropped_subscriber_does_not_disturb_the_run() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        let subscription = subscribe(run).unwrap();
        next_phase(&subscription, RunPhase::Starting);
        drop(subscription);
        guard.running();
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        assert!(guard.finish());
        drop(guard);
        assert_eq!(outcome(run).unwrap().end, RunEnd::Completed);
    }

    #[test]
    fn an_unknown_run_cannot_be_subscribed() {
        let run = Uuid::new_v4();
        assert!(matches!(subscribe(run), Err(RunControlError::UnknownRun)));
        assert!(outcome(run).is_none());
    }

    #[test]
    fn retention_keeps_only_a_bounded_recent_window() {
        let mut outcomes = VecDeque::new();
        let runs: Vec<_> = (0..RETAINED_OUTCOMES + 4).map(|_| Uuid::new_v4()).collect();
        for run in &runs {
            retain_in(
                &mut outcomes,
                RunOutcome {
                    run: *run,
                    instance: Uuid::new_v4(),
                    end: RunEnd::Completed,
                },
            );
        }
        assert_eq!(outcomes.len(), RETAINED_OUTCOMES);
        assert!(outcomes.iter().all(|outcome| outcome.run != runs[0]));
        assert_eq!(outcomes.back().unwrap().run, *runs.last().unwrap());
    }
}
