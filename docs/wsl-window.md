# WSL2 offscreen Linux window check

On 26 November 2026 an ordinary Ubuntu 24.04 WSL2 user opened the Linux candidate on a private Xvfb display with WebKit sandboxing requested. The web-content process ran in a nested PID namespace with no-new-privileges and an active seccomp filter. The 1040 × 760 capture shows one installed Forest sample, Generation A, the Review update and Repair files actions, and 80 KiB of fixture content. The frontend had received the seeded native library response.

The executable from the [Linux development package](linux-build.md) had SHA-256 `17ca29cfaf82547ff9d00a5921df494dc63e712218071616300dece556021b53`. The sandbox-required Carbon-only PNG is `.artifacts/linux-build/window-check-Iq3BmPhJ/carbon-window.png` in the review checkout. Its SHA-256 is `ae722ccdbc4ad7398a4c388adf4ae509468b0c379bfa177f748a7bccf56b2259`. The reviewer visually checked the text and data after first paint. Only the seeded read was exercised.

## Window and sandbox observations

| Run | Result |
| --- | --- |
| SDK libraries and standalone WebKit helper environment path | The GTK launcher started. WebKit sought its network helper at its compiled system path, which was absent on this WSL installation. |
| Private mount view exposing the SDK helpers at that path, default WebKit settings | The populated Carbon window rendered. A repeat showed the launcher and WebKit web process at PID namespace depth 1 with `NoNewPrivs=1` and `Seccomp=0`. No sandbox-disabling environment variable was supplied. These observations provide no evidence of WebKit child sandbox enforcement. |
| Same mount view with `WEBKIT_FORCE_SANDBOX=1` | The nested Bubblewrap helper could not set its UID map because its filesystem was read-only. WebKit reported that its D-Bus proxy could not launch. This configuration did not render a candidate window. The log is `.artifacts/linux-build/window-check-WRf5wWew/process.log`. |
| Sandbox required with a private outer PID namespace and procfs | The helper passed UID-map setup, then stopped because the namespace lacked `/usr/bin/xdg-dbus-proxy`. The SDK contains this executable in its own `usr/bin` directory. |
| Sandbox required with the SDK proxy and helper directory mounted read-only at their compiled paths | The seeded window rendered. Inside the harness procfs the web-content process had PID namespace depth 2, `NoNewPrivs=1`, `Seccomp=2` and one filter. The launcher and WebKit network helper had depth 1, `NoNewPrivs=1` and `Seccomp=0`. The observation applies to the web-content process and this launch configuration. |

The display used X11 with TCP listening disabled. A new private home, runtime directory, session bus and temporary fixture library isolated the review data. No system package was installed and no visible desktop was opened. The renderer requested software graphics. The accessibility bus was unavailable and GStreamer's PulseAudio plugin reported a missing `libpulsecommon-16.1.so`. Assistive technology, audio, GPU acceleration, input, native GNOME/KDE behavior and gameplay were untested.

## Repeat the sandbox-required check

With the extracted SDK and executable from the Linux package runbook, run `bash scripts/wsl-window-check.sh` from an ordinary WSL2 account. The script requests WebKit's sandbox, checks the web-content process flags, gives the check a bounded lifetime and writes its executable hash and process log into a new ignored directory under `.artifacts/linux-build/`. Inspect the PNG for the installed Forest sample. A window title or the opening-library message alone cannot confirm the native library response. An authorized native Linux desktop is still required for native qualification.
