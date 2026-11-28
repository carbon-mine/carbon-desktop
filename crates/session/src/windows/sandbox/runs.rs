// SPDX-License-Identifier: MPL-2.0
//! Process-wide directory of live protected sample runs.
//!
//! Each entry pairs a run UUID with the one OS job that owns its process tree.
//! The owning runner keeps the entry for as long as it may still act on the
//! run; a stop request terminates only that entry's job, so stopping run A
//! cannot affect a later run B or another instance's run.
use super::job;
use super::Handle;
use crate::{
    ResourceLimit, RunControlError, RunEnd, RunOutcome, RunPhase, RunState, RunSubscription,
    RunUpdate,
};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// The owner could not confirm the run settled, even if a stop claimed it.
    unsettled: bool,
    /// The close bound expired and this process terminated the remaining tree
    /// itself, so the terminal record must not read as a graceful close.
    forced: bool,
    /// The root exited on its own but reported a non-zero status after the
    /// completion claim had been made, so the record must not claim a sample.
    failed: bool,
    /// The boundary reported that its qualified limit refused a further process.
    resource_limit: Option<ResourceLimit>,
    /// The owner has retired the run; no stop may claim it any more.
    retired: bool,
}

struct Entry {
    instance: Uuid,
    status: Mutex<Status>,
    job: Arc<Handle>,
    subscribers: Mutex<Vec<Sender<RunUpdate>>>,
    /// Test-only count of graceful close requests this entry's tree received.
    #[cfg(test)]
    closes: AtomicUsize,
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

#[cfg(not(test))]
fn outcomes() -> &'static Mutex<VecDeque<RunOutcome>> {
    static OUTCOMES: OnceLock<Mutex<VecDeque<RunOutcome>>> = OnceLock::new();
    OUTCOMES.get_or_init(|| Mutex::new(VecDeque::new()))
}

// Tests run in parallel in one process. Each test thread gets its own bounded
// store so a neighbouring test's retirement cannot evict the record a test is
// about to assert. Production uses the single process-wide store.
#[cfg(test)]
thread_local! {
    static OUTCOMES: std::cell::RefCell<VecDeque<RunOutcome>> =
        const { std::cell::RefCell::new(VecDeque::new()) };
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

#[cfg(not(test))]
fn retain(outcome: RunOutcome) {
    retain_in(
        &mut outcomes()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        outcome,
    );
}

#[cfg(test)]
fn retain(outcome: RunOutcome) {
    OUTCOMES.with(|outcomes| retain_in(&mut outcomes.borrow_mut(), outcome));
}

fn retain_in(outcomes: &mut VecDeque<RunOutcome>, outcome: RunOutcome) {
    // A reused run UUID must not serve the previous record while the new run is
    // live, so the old one is replaced rather than duplicated.
    outcomes.retain(|existing| existing.run != outcome.run);
    if outcomes.len() >= RETAINED_OUTCOMES {
        outcomes.pop_front();
    }
    outcomes.push_back(outcome);
}

fn forget_outcome(run: Uuid) {
    #[cfg(test)]
    OUTCOMES.with(|outcomes| outcomes.borrow_mut().retain(|outcome| outcome.run != run));
    #[cfg(not(test))]
    outcomes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|outcome| outcome.run != run);
}

/// Removes its run from the directory when the owning runner finishes,
/// including on an error or panic path. Closing the last job handle also
/// destroys any tree the run never drained.
pub(crate) struct RunGuard {
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

    /// Records that the owner could not settle the run, even if a stop claimed
    /// it. The terminal record reports `Unsettled` instead of claiming a sample,
    /// a stop or a drain the caller never confirmed.
    pub(crate) fn unsettle(&self) {
        let mut status = lock_status(&self.entry);
        status.completed = false;
        status.unsettled = true;
    }

    /// Withdraws the completion claim after the drained sample reported its own
    /// failure. The empty boundary is already confirmed, so the record reports
    /// `SampleFailed` rather than an unconfirmed outcome. A stop that claimed the
    /// run first keeps its own end, and a later [`Self::unsettle`] still wins.
    pub(super) fn fail(&self) {
        let mut status = lock_status(&self.entry);
        if status.completed && !status.retired {
            status.completed = false;
            status.failed = true;
        }
    }

    /// True when a stop request was observed before the run finished.
    pub(super) fn stopped(&self) -> bool {
        matches!(phase(&self.entry), RunPhase::Stopping | RunPhase::Draining)
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        // The directory is taken first, matching active() and subscribe(), and
        // held through removal. Retirement and removal are therefore atomic: no
        // observer can read a finished entry out of the live directory.
        let mut directory = entries();
        let mut status = lock_status(&self.entry);
        status.retired = true;
        // Every notification the boundary posted before it was retired is read
        // here, so a payload whose helper the limit refused is explained in the
        // record even though the refusal happened while it ran.
        if let Some(limit) = job::refusal(&self.entry.job) {
            status.resource_limit = Some(limit);
        }
        let drained = job::is_empty(&self.entry.job).ok();
        let end = if status.unsettled || drained != Some(true) {
            // A failed observation or a boundary that still held members means
            // the owner never confirmed drain, so no stop or sample is claimed.
            RunEnd::Unsettled
        } else if status.forced {
            // A stop's close bound expired and this process ended the tree.
            RunEnd::StopForced
        } else if matches!(status.phase, RunPhase::Stopping | RunPhase::Draining) {
            RunEnd::StopRequested
        } else if status.failed {
            // The drained root reported its own non-zero status, so the record
            // must not claim a completed sample.
            RunEnd::SampleFailed
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
            drained,
            resource_limit: status.resource_limit,
        };
        // Retain and notify while the entry is still in the directory and the
        // status lock proves it is live, so a subscriber that races removal
        // cannot miss the terminal update.
        retain(outcome.clone());
        for sender in lock_subscribers(&self.entry).drain(..) {
            let _ = sender.send(RunUpdate::Finished(outcome.clone()));
        }
        if directory
            .get(&self.run)
            .is_some_and(|entry| Arc::ptr_eq(entry, &self.entry))
        {
            directory.remove(&self.run);
        }
    }
}

pub(super) fn register(run: Uuid, instance: Uuid, job: Arc<Handle>) -> RunGuard {
    forget_outcome(run);
    let entry = Arc::new(Entry {
        instance,
        status: Mutex::new(Status {
            phase: RunPhase::Starting,
            completed: false,
            unsettled: false,
            forced: false,
            failed: false,
            resource_limit: None,
            retired: false,
        }),
        job,
        subscribers: Mutex::new(Vec::new()),
        #[cfg(test)]
        closes: AtomicUsize::new(0),
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

/// Test-only observation of how many graceful close requests one live entry
/// received. A run that is no longer in the directory reports `None`.
#[cfg(test)]
fn close_requests(run: Uuid) -> Option<usize> {
    entries()
        .get(&run)
        .map(|entry| entry.closes.load(Ordering::SeqCst))
}

#[cfg(not(test))]
pub(super) fn outcome(run: Uuid) -> Option<RunOutcome> {
    outcomes()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|outcome| outcome.run == run)
        .cloned()
}

#[cfg(test)]
pub(super) fn outcome(run: Uuid) -> Option<RunOutcome> {
    OUTCOMES.with(|outcomes| {
        outcomes
            .borrow()
            .iter()
            .find(|outcome| outcome.run == run)
            .cloned()
    })
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
    #[cfg(test)]
    entry.closes.fetch_add(1, Ordering::SeqCst);
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
    // A tree that drained exactly at the bound closed itself, so the record must
    // stay the graceful stop. Only a failed observation falls through to the
    // forced path, where bounding the tree matters more than the label.
    if matches!(job::is_empty(&entry.job), Ok(true)) {
        return;
    }
    // Claim the escalation before the OS call. A concurrent retirement that sees
    // an empty boundary must not publish a graceful stop for a tree this process
    // is about to end, so the claim is made while the stop still owns the run.
    {
        let mut status = lock_status(entry);
        if !matches!(status.phase, RunPhase::Stopping | RunPhase::Draining) {
            return;
        }
        status.forced = true;
    }
    if let Err(error) = job::terminate(&entry.job) {
        eprintln!(
            "Could not force the stopped protected run to drain ({:?}).",
            error.kind()
        );
        // Withdraw the claim: this process did not end the tree, and an
        // unconfirmed boundary already reports `Unsettled`.
        let mut status = lock_status(entry);
        if matches!(status.phase, RunPhase::Stopping | RunPhase::Draining) {
            status.forced = false;
        }
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
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::Completed);
        assert_eq!(record.drained, Some(true));
        assert!(inspect(run).is_none());
        assert!(subscription
            .recv_timeout(Duration::from_millis(50))
            .is_err());
    }

    #[test]
    fn a_subscriber_sees_a_forced_stop() {
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
        // The bound expires immediately, so this process terminates the tree
        // rather than the run's own windows closing it.
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        next_phase(&subscription, RunPhase::Stopping);
        next_phase(&subscription, RunPhase::Draining);
        wait_drained(run);
        drop(guard);
        next_end(&subscription, RunEnd::StopForced);
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::StopForced);
        assert_eq!(record.drained, Some(true));
    }

    #[test]
    fn a_stop_that_drains_within_the_bound_reports_the_graceful_stop() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        assert_eq!(
            stop_after(run, Duration::from_secs(3)).unwrap().phase,
            RunPhase::Draining
        );
        // The fixture tree exits on its own well inside the bound, so the
        // terminal record must not claim this process ended it.
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        drop(guard);
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::StopRequested);
        assert_eq!(record.drained, Some(true));
    }

    #[test]
    fn a_forced_stop_the_owner_could_not_settle_stays_unsettled() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        stop_soon(run).unwrap();
        wait_drained(run);
        // A confirmed empty boundary is still not a settled stop when the owner
        // could not complete its own checks.
        guard.unsettle();
        drop(guard);
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::Unsettled);
    }

    #[test]
    fn retiring_without_a_confirmed_drain_reports_unsettled() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        // A bound that has not expired leaves the fixture tree alive, so the
        // owner retires with members still inside the boundary and no confirmed
        // drain. The record must not claim the stop settled.
        assert_eq!(
            stop_after(run, Duration::from_secs(2)).unwrap().phase,
            RunPhase::Draining
        );
        drop(guard);
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::Unsettled);
        assert_eq!(record.drained, Some(false));
        wait_drained(run);
    }

    #[test]
    fn a_stop_whose_settlement_cannot_be_confirmed_reports_unsettled() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        // The owner's post-launch or drain check failed, so the record must not
        // claim a stop the caller never confirmed.
        guard.unsettle();
        drop(guard);
        assert_eq!(outcome(run).unwrap().end, RunEnd::Unsettled);
    }

    #[test]
    fn a_drained_sample_that_reported_its_own_failure_is_not_a_completion() {
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
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        assert!(guard.finish());
        next_phase(&subscription, RunPhase::Finishing);
        // The drained root reported a non-zero status, so the completion claim is
        // withdrawn without turning the confirmed drain into an unknown outcome.
        guard.fail();
        drop(guard);
        next_end(&subscription, RunEnd::SampleFailed);
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::SampleFailed);
        assert_eq!(record.drained, Some(true));
    }

    #[test]
    fn a_failure_that_cannot_be_confirmed_stays_unsettled() {
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
        guard.fail();
        // A later step could not confirm the tree settled, so the unknown outcome
        // must win over the reported sample failure.
        guard.unsettle();
        drop(guard);
        assert_eq!(outcome(run).unwrap().end, RunEnd::Unsettled);
    }

    #[test]
    fn a_reported_failure_cannot_replace_a_stop_that_claimed_the_run() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        assert_eq!(stop_soon(run).unwrap().phase, RunPhase::Draining);
        wait_drained(run);
        // The stop claimed the run first, so no later sample result may rewrite
        // its terminal record.
        guard.fail();
        drop(guard);
        assert_eq!(outcome(run).unwrap().end, RunEnd::StopForced);
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
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::Unsettled);
        assert_eq!(
            record.drained,
            Some(false),
            "the fixture child was still alive when the owner retired"
        );
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
    fn a_boundary_refusal_is_retained_in_the_redacted_record() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::refusing_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        fs::write(root.path().join("finish"), []).unwrap();
        wait_drained(run);
        assert!(guard.finish());
        drop(guard);
        let record = outcome(run).unwrap();
        assert_eq!(record.end, RunEnd::Completed);
        assert_eq!(record.drained, Some(true));
        assert_eq!(record.resource_limit, Some(ResourceLimit::ActiveProcess));
    }

    #[test]
    fn a_run_without_a_refusal_carries_no_limit_reason() {
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
        drop(guard);
        assert_eq!(outcome(run).unwrap().resource_limit, None);
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
                    drained: Some(true),
                    resource_limit: None,
                },
            );
        }
        assert_eq!(outcomes.len(), RETAINED_OUTCOMES);
        assert!(outcomes.iter().all(|outcome| outcome.run != runs[0]));
        assert_eq!(outcomes.back().unwrap().run, *runs.last().unwrap());

        // A repeated run ID replaces the earlier record rather than adding one.
        retain_in(
            &mut outcomes,
            RunOutcome {
                run: *runs.last().unwrap(),
                instance: Uuid::new_v4(),
                end: RunEnd::StopRequested,
                drained: Some(false),
                resource_limit: None,
            },
        );
        assert_eq!(outcomes.len(), RETAINED_OUTCOMES);
        assert_eq!(outcomes.back().unwrap().end, RunEnd::StopRequested);
        assert_eq!(outcomes.back().unwrap().drained, Some(false));
    }

    #[test]
    fn concurrent_stops_claim_one_run_and_request_close_once() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        let gate = Arc::new(std::sync::Barrier::new(6));
        let phases: Vec<RunPhase> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..6)
                .map(|_| {
                    let gate = Arc::clone(&gate);
                    scope.spawn(move || {
                        gate.wait();
                        stop_soon(run).unwrap().phase
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });
        // One request claims the transition; every later request observes the
        // same drain and starts no second bound.
        assert!(phases
            .iter()
            .all(|phase| matches!(phase, RunPhase::Stopping | RunPhase::Draining)));
        assert_eq!(
            close_requests(run),
            Some(1),
            "only the claiming request may ask the tree to close"
        );
        // The accepted stop already decided the outcome, so the owner cannot
        // publish a completion of the sample it never finished.
        assert!(!guard.finish());
        wait_drained(run);
        drop(guard);
        let record = outcome(run).unwrap();
        // The test bound is already expired, so the escalation ends the tree and
        // the record must not read as a graceful close.
        assert_eq!(record.end, RunEnd::StopForced);
        assert_eq!(record.drained, Some(true));
    }

    #[test]
    fn a_stop_and_a_completion_claim_agree_on_one_terminal_record() {
        // Either claim may win. The record must follow the winner and never
        // report an unsettled or contradictory end after an empty boundary.
        for _ in 0..8 {
            let root = tempfile::tempdir().unwrap();
            let run = Uuid::new_v4();
            let guard = Arc::new(register(
                run,
                instance(),
                job::test_boundary(run, root.path()).unwrap(),
            ));
            guard.running();
            let gate = Arc::new(std::sync::Barrier::new(2));
            let (finished, stopped) = std::thread::scope(|scope| {
                let owner = Arc::clone(&guard);
                let finish_gate = Arc::clone(&gate);
                let finish = scope.spawn(move || {
                    finish_gate.wait();
                    owner.finish()
                });
                let stop_gate = Arc::clone(&gate);
                let stop = scope.spawn(move || {
                    stop_gate.wait();
                    stop_soon(run).map(|state| state.phase)
                });
                (finish.join().unwrap(), stop.join().unwrap())
            });
            // A completion claim that won leaves the stop observing `finishing`;
            // a stop that won withdraws the completion claim.
            assert_eq!(
                finished,
                !matches!(stopped, Ok(RunPhase::Stopping | RunPhase::Draining))
            );
            // A completion claim leaves the fixture child running until it is
            // told to finish. When the stop claim won, the escalation owns the
            // tree, so writing the marker would let it exit naturally and make
            // the forced label depend on thread timing.
            if finished {
                fs::write(root.path().join("finish"), []).unwrap();
            }
            wait_drained(run);
            drop(guard);
            let record = outcome(run).unwrap();
            assert_eq!(record.drained, Some(true));
            assert_eq!(
                record.end,
                if finished {
                    RunEnd::Completed
                } else {
                    // The stop's zero bound expired, so the escalation ended it.
                    RunEnd::StopForced
                }
            );
        }
    }

    #[test]
    fn the_visible_surface_serializes_only_redacted_run_state() {
        let run = Uuid::new_v4();
        let instance = Uuid::new_v4();
        assert_eq!(
            serde_json::to_value(RunState {
                run,
                instance,
                phase: RunPhase::Draining,
            })
            .unwrap(),
            serde_json::json!({
                "run": run.to_string(),
                "instance": instance.to_string(),
                "phase": "draining",
            })
        );
        assert_eq!(
            serde_json::to_value(RunOutcome {
                run,
                instance,
                end: RunEnd::StopRequested,
                drained: Some(true),
                resource_limit: None,
            })
            .unwrap(),
            serde_json::json!({
                "run": run.to_string(),
                "instance": instance.to_string(),
                "end": "stopRequested",
                "drained": true,
                "resourceLimit": null,
            })
        );
        // The refusal is redacted to its kind, never the payload that asked.
        assert_eq!(
            serde_json::to_value(RunOutcome {
                run,
                instance,
                end: RunEnd::Completed,
                drained: Some(true),
                resource_limit: Some(ResourceLimit::ActiveProcess),
            })
            .unwrap(),
            serde_json::json!({
                "run": run.to_string(),
                "instance": instance.to_string(),
                "end": "completed",
                "drained": true,
                "resourceLimit": "activeProcess",
            })
        );
    }

    #[test]
    fn an_evicted_record_is_no_longer_served() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let guard = register(
            run,
            instance(),
            job::test_boundary(run, root.path()).unwrap(),
        );
        guard.running();
        drop(guard);
        assert!(outcome(run).is_some());
        for _ in 0..RETAINED_OUTCOMES {
            let newer = Uuid::new_v4();
            let newer_guard = register(
                newer,
                instance(),
                job::test_boundary(newer, root.path()).unwrap(),
            );
            newer_guard.running();
            drop(newer_guard);
        }
        assert!(outcome(run).is_none(), "the oldest record was evicted");
        assert!(matches!(subscribe(run), Err(RunControlError::UnknownRun)));
    }
}
