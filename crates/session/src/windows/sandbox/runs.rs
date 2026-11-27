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
    *entry
        .phase
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn set_phase(entry: &Entry, next: RunPhase) {
    *entry
        .phase
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
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
        if phase(&self.entry) == RunPhase::Starting {
            set_phase(&self.entry, RunPhase::Running);
        }
    }

    /// The root process has exited and the tree is draining.
    pub(super) fn draining(&self) {
        set_phase(&self.entry, RunPhase::Draining);
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
            set_phase(&entry, current);
            return Err(RunControlError::Boundary(error));
        }
        set_phase(&entry, RunPhase::Draining);
    }
    Ok(state(run, &entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> Uuid {
        Uuid::new_v4()
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
        assert!(job::boundary_empty(first_run).unwrap());
        assert!(!job::boundary_empty(second_run).unwrap());
        assert_eq!(inspect(second_run).unwrap().phase, RunPhase::Running);

        // The unrelated run stays under its owner's control.
        assert_eq!(stop(second_run).unwrap().phase, RunPhase::Draining);
        assert!(job::boundary_empty(second_run).unwrap());
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
        assert!(job::boundary_empty(run).unwrap());
        assert!(guard.stopped());
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
        assert!(job::boundary_empty(run).unwrap());
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
