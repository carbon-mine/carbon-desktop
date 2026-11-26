# Protected sample run reservation

The fixed Windows x64 Java sample holds its existing exclusive per-instance `sample.lock` from reservation through any save publication and transient cleanup. A separate instance can hold its own lock. The managed runtime remains read-locked during its trusted copy into the sample's private stage. The transient registry's short lock covers ownership records and cleanup, then releases before process creation or waits.

Each reservation generates a fresh UUID. The transient ownership record pairs the run UUID with the exact instance UUID. Before the host asks Windows to create the suspended process, it flushes and atomically publishes a no-replace `pending-run-v1` record in that instance's sample-only directory. The record carries no executable, account credential, or PID.

If creation fails before Windows returns a process, the host can clear its own record. Once a process has been created, the record can be cleared after live job accounting reports zero active members. If admission validation rejects the suspended root, the host also waits on that exact process handle. Error paths attempt bounded termination and drain. A verified forced drain allows the record to clear but returns the original sample error instead of publishing a count.

An owner disappearing or losing the ability to observe the boundary leaves the record in place. A later launch returns a recovery-required error for that instance even if the lock is free. Registry recovery retains that run's transient tree. It also preserves malformed, unrecognized, and older v1 ownership records whose state cannot establish that no process was admitted. A valid v2 record whose reservation never reached admission, or whose admission is already cleared, can be retired. The code never signals a process by a number read from storage.

`cargo test --locked -p carbon-session` covers two real test processes contending for one instance while the other instance remains available. The tests also cover an owner exiting without running destructors, retained transient data after unknown drain, preservation of unrecognized records, and rejection of a mismatched run record.

This is a protected-sample reservation only. It does not offer a force-unlock operation on unresolved records, a production game runner, active stop from either shell, or restart recovery by verified OS-boundary identity. Unknown outcomes remain fenced until that recovery exists. Player saves and installed instances remain outside this fixture storage.
