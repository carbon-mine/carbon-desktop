# GPUI accessibility

Carbon Desktop exposes the GPUI launcher through Win32 UI Automation on Windows and AT-SPI on Linux. The provider follows the rendered library, workspace, active tab and Activity section. Controls report their actual labels, enabled and selected states, focus and visible bounds. An accessibility action invokes the same app operation as the visual control. Search and profile form edits use the real input entities. Opening a popup disables accessibility actions in its owner, and closing it restores them.

Platform services own their file picker controls. Windows uses the normal system menu provider for its File and View menus. Linux uses the GPUI accessibility provider for the floating command menus. A popup exposes its own semantic tree, including field values, version choices, confirmation and validation. Provider callbacks send bounded requests to the native UI thread, which resolves them against the current controls and their supported actions.

`accesskit` 0.25.2, `accesskit_windows` 0.35.2 and `accesskit_unix` 0.24.1 resolve to AccessKit source revision `f3026222559c7f5f46d8f25cb7404de68eb149f0` according to their published crate metadata. They are dual licensed under MIT or Apache 2.0. The project retains both notices. The existing async-channel dependency provides the handoff to GPUI. [AccessKit source](https://github.com/AccessKit/accesskit/tree/f3026222559c7f5f46d8f25cb7404de68eb149f0) · [MIT notice](../third_party/accesskit/LICENSE-MIT) · [Apache notice](../third_party/accesskit/LICENSE-APACHE)

## Verification

The integration scripts use a separate disposable 100-profile library and the actual launcher binary. They query the platform provider, exercise focus and search, select an instance, open its own Create window, trigger validation, enter a new name, save, and check the persisted record. They leave desktop accounts, ordinary profiles and game saves alone.

Windows uses PowerShell 7 on the native desktop.

```powershell
cargo build --locked -p carbon-desktop-gpui
./scripts/check-gpui-uia.ps1 -Executable ./target/debug/carbon-desktop-gpui.exe -EvidenceDirectory ./.artifacts/a11y-windows
```

The Windows smoke pass on the integrated native shell exposed 122 descendants in the initial launcher UIA tree and 11 in the separate editor. The external test observed actual search focus and value changes, filtered and selected instance 099, invoked the editor, observed a validation alert and saved one disposable profile. Exact node counts can change as the visible controls change.

The Linux integration script requires a graphical desktop session and its AT-SPI bus, plus Python 3 with GObject introspection bindings for Atspi.

```sh
python3 scripts/check-gpui-atspi.py ./target/debug/carbon-desktop-gpui ./.artifacts/a11y-linux
```

On the shipped Linux ELF from `bf7ab16`, an unprivileged WSL2 run under Xvfb, Openbox and Mesa lavapipe exposed 125 AT-SPI nodes in the main window and 12 in the separate Create window. The client focused search, set and read back `099`, chose the matching instance, opened the form, observed rejected-name validation and focused the `26.3` option. It then edited the actual name field, saved and verified exactly one new disposable profile. The optional `--activate-x11` flag uses `xdotool` to foreground this test window by PID and title on an isolated X11 display.

Orca on the focus-fixed Linux build `0e4a458` announced the selected library row, the search field and the focused Minecraft version choice in the separate New vanilla instance window. That test used a private WSL X11 desktop. The later Linux build `bf7ab16` redraws after actions sent over the accessibility bus so the provider can report updated values even when the app was idle. A complete speech walkthrough, Narrator, native Wayland, high-contrast behavior, and exact per-character geometry for the custom editor remain open.
