# Linux process tree fixture

CDESK-22 tracks the shared session supervisor. The Linux fixture enables local testing of namespace admission, reaping and stop on a fixed root, child and grandchild. It runs independently of both launchers and needs no account, installed game files, profile, or save directory.

Run from the repository root on an ordinary Linux user account with system-provided, non-setuid Bubblewrap and Python 3.10 or later. Bubblewrap requires unprivileged user and PID namespaces. The fixture uses the merged `/usr` layout used by the existing Ubuntu isolation probe.

```sh
/usr/bin/python3 -I scripts/linux-supervision.py
```

The external owner opens one private control pipe per instance of Bubblewrap and waits for the trusted Python init to attest PID 1 and its own procfs. An explicit admission byte starts the same fixed process tree each time. The sandbox uses separate user, PID, network, IPC and UTS namespaces, a read-only system and fixture view, and dropped capabilities. A namespace setup failure closes the handshake and reports an unavailable or incomplete fixture. There is no host process fallback.

Namespace init reaps the root and the adopted orphan descendants. A live grandchild deliberately ignores SIGTERM. On repeated stop, the first stop establishes a single 200 ms grace interval, after which init escalates to SIGKILL inside its PID namespace. A drain event requires `waitpid(-1, WNOHANG)` to report `ECHILD`. A timed-out observation produces an error.

Only the literal release byte lets the grandchild exit normally. The negative control closes the release pipe while init remains alive, observes that the grandchild received EOF, then requires forced drain. In the separate owner-loss check, the grandchild holds fixture stdout open through release-pipe EOF, so closing the release pipe cannot explain the observer's loss of stdout. Bubblewrap's parent-death handling triggers kernel teardown. The 5-second observation bound and 15-second descendant watchdog are independent. A watchdog emits a failure marker instead of a successful drain. Stdout closure supplies no game save acknowledgement. [Linux PID namespaces](https://www.man7.org/linux/man-pages/man7/pid_namespaces.7.html) · [Bubblewrap reference](https://github.com/containers/bubblewrap/blob/v0.9.0/bwrap.xml)

## WSL2 evidence

Ubuntu 24.04 x86-64, WSL2 kernel `6.6.87.2-microsoft-standard-WSL2`, UID 1000, Bubblewrap 0.9.0 and Python 3.12.3. Ten consecutive runs observed the same cases.

| Case | Observation |
| --- | --- |
| Cancel or lose the owner before admission | No root or descendant event |
| Root exit with a surviving grandchild | Init emitted `root-exited-descendant-live` after reaping the root and receiving the grandchild's readiness marker |
| Close the release pipe while init remains alive | Grandchild acknowledged EOF and stayed alive until forced stop. A temporary variant that voluntarily exited on EOF failed this check. |
| Repeated stop with an uncooperative grandchild | Forced drain in 209–211 ms across ten runs, including the 200 ms grace interval |
| Independent second tree | Stayed live until its own release and then drained naturally |
| Kill the external owner after admission | Independent observer saw fixture stdout close in 2–3 ms across ten runs, with no release byte sent |
| Run init directly on the host | Refused before admission because it was outside the designated PID namespace |
| Forbid user namespaces in a transient service | Bubblewrap reported kernel permission denial, the readiness handshake closed, and the fixture exited 1 |

On a Linux session with a systemd user manager, reproduce the permission denial with a transient service.

```sh
systemd-run --user --wait --pipe --collect -p RestrictNamespaces=~user \
  /usr/bin/python3 -I "$(realpath scripts/linux-supervision.py)"
```

Native Linux desktop acceptance is pending. The production host still needs run-ID and per-instance leases, artifact and runtime references, crash recovery by stable boundary identity, explicit launcher shutdown policy, resource-limit qualification, and real Minecraft acceptance. This fixture does not configure cgroups or change the shared session registry, installed files, or saves.
