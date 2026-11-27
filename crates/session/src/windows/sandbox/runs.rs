// SPDX-License-Identifier: MPL-2.0
//! Process-wide directory of live protected sample runs.
//!
//! Each entry pairs a run UUID with the one OS job that owns its process tree.
//! The owning runner keeps the entry for as long as it may still act on the
//! run; a stop request terminates only that entry's job, so stopping run A
//! cannot affect a later run B or another instance's run.
use super::job;
use super::Handle;
use crate::{RunControlError, RunPhase, RunState};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
use uuid::Uuid;

struct Entry {
    instance: Uuid,
    phase: Mutex<RunPhase>,
    job: Arc<Handle>,
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

fn phase(entry: &Entry) -> RunPhase {
    *lock_phase(entry)
}

fn set_phase(entry: &Entry, next: RunPhase) {
    *lock_phase(entry) = next;
}

fn lock_phase(entry: &Entry) -> MutexGuard<'_, RunPhase> {
    entry
        .phase
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn state(run: Uuid, entry: &Entry) -> RunState {
    RunState {
        run,
        instance: entry.instance,
        phase: phase(entry),
    }
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
        let mut phase = lock_phase(&self.entry);
        if *phase == RunPhase::Starting {
            *phase = RunPhase::Running;
        }
    }

    /// The owner observed an empty tree and claims the completed outcome.
    /// Returns false when a stop request claimed the run first, so a later
    /// request cannot rewrite an outcome the owner already decided.
    pub(super) fn finish(&self) -> bool {
        let mut phase = lock_phase(&self.entry);
        if matches!(*phase, RunPhase::Stopping | RunPhase::Draining) {
            return false;
        }
        *phase = RunPhase::Finishing;
        true
    }

    /// True when a stop request was observed before the run finished.
    pub(super) fn stopped(&self) -> bool {
        matches!(phase(&self.entry), RunPhase::Stopping | RunPhase::Draining)
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
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
        phase: Mutex::new(RunPhase::Starting),
        job,
    });
    let replaced = entries().insert(run, Arc::clone(&entry));
    debug_assert!(replaced.is_none(), "a run UUID is only registered once");
    RunGuard { run, entry }
}

pub(super) fn inspect(run: Uuid) -> Option<RunState> {
    let entry = entries().get(&run).cloned()?;
    Some(state(run, &entry))
}

pub(super) fn active() -> Vec<RunState> {
    entries()
        .iter()
        .map(|(run, entry)| state(*run, entry))
        .collect()
}

/// Requests termination of one run's own process tree. A repeated request while
/// the run is still draining reports the same phase and does not signal again.
pub(super) fn stop(run: Uuid) -> Result<RunState, RunControlError> {
    let entry = entries()
        .get(&run)
        .cloned()
        .ok_or(RunControlError::UnknownRun)?;
    let current = phase(&entry);
    if matches!(current, RunPhase::Starting | RunPhase::Running) {
        set_phase(&entry, RunPhase::Stopping);
        if let Err(error) = job::terminate(&entry.job) {
            // Restore only our own request; the owner may have moved the run on
            // to a later phase while the termination call was in flight.
            let mut phase = lock_phase(&entry);
            if *phase == RunPhase::Stopping {
                *phase = current;
            }
            drop(phase);
            return Err(RunControlError::Boundary(error));
        }
        let mut phase = lock_phase(&entry);
        if *phase == RunPhase::Stopping {
            *phase = RunPhase::Draining;
        }
    }
    Ok(state(run, &entry))
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
        assert_eq!(stop(first_run).unwrap().phase, RunPhase::Draining);
        wait_drained(first_run);
        assert!(!job::boundary_empty(second_run).unwrap());
        assert_eq!(inspect(second_run).unwrap().phase, RunPhase::Running);

        // The unrelated run stays under its owner's control.
        assert_eq!(stop(second_run).unwrap().phase, RunPhase::Draining);
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
        assert_eq!(stop(run).unwrap().phase, RunPhase::Draining);
        assert_eq!(stop(run).unwrap().phase, RunPhase::Draining);
        wait_drained(run);
        assert!(guard.stopped());
        // A stop claims the outcome, so the owner cannot later report completion.
        assert!(!guard.finish());
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
        assert_eq!(stop(run).unwrap().phase, RunPhase::Draining);
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
}
