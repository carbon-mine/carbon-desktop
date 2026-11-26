# GPUI desktop shell

Windows exposes an operating-system File and View menu bar. Linux exposes File and View in the toolbar and opens the commands in separate, themed floating windows. Both surfaces use the same launcher command handlers.

| Command | Keyboard |
| --- | --- |
| File or View menu | Alt+F or Alt+V |
| New vanilla instance | Ctrl+N |
| Import a local pack | Ctrl+O |
| Find instance | Ctrl+F |
| View activity | F6 |
| Close launcher | Ctrl+W |
| Move through a Linux command menu | Up, Down, Tab or Shift+Tab |
| Apply or dismiss a Linux menu item | Enter or Space to apply, Escape to dismiss |

View offers System, Light and Dark, with a selection mark on the active preference. Each launched library keeps its own appearance and normal window bounds. A maximized window also records its restore bounds. Startup uses the default centered size if a stored caption is unreachable or bounds are malformed. The normal close routes check for an active file chooser, owned form or fixed protected Java test session. During the Java session, Carbon directs the user to **Save and close** in that test window.

The Windows menu uses muda 0.20.0 under its Apache-2.0 or MIT license. Its ComCtl32 window subclass forwards unhandled messages. The launcher selects GPUI�s GPU-accelerated HWND swap chain so Windows can composite the native menu row and GPU-rendered workspace together. The menu and caption follow the resolved app palette. When Windows reports High Contrast, the native menu and caption use system colors. On Linux, GPUI 0.2.2 creates a transient for the active X11 parent for floating windows. Carbon also restores the previous GPUI focus handle when a child closes. On Wayland, the pinned GPUI implementation asks the compositor for a parented toplevel. A Wayland desktop was unavailable for verification.

## Verification

The Windows and Linux launcher both built against the lockfile. Placement tests cover disconnected displays, negative monitor coordinates and invalid bounds. Appearance tests cover persistence and malformed preference files.

On ordinary-user WSL2, Xvfb, Openbox and Mesa lavapipe, Alt+V opened a 315×248 View window. Its X11 transient hint identified the tested main window. The keyboard selected Dark and the private library recorded the preference. Alt+F and Enter then opened a separate 480×417 New vanilla instance window with the same transient parent. Escape returned focus to the launcher, where Ctrl+W closed it. Reopening the same library restored the measured 792×608 window size. The run used a private desktop session and app data directory. Native Linux and Wayland remain unverified.

At shell commit `0213267` on Windows, the native menu reported exactly one checked appearance. Choosing Dark set DWM attribute 20 on the main and New vanilla instance HWNDs. The editor had the main HWND as its owner and disabled it until Escape. After Escape, the launcher again held the foreground. Ctrl+W saved the preference and placement. Reopening restored the exact outer rectangle, 842×619 at (144, 83), and Dark remained checked. The File menu was opened through the Windows menu-loop command, then N and C exercised its native New and Close mnemonics. A physical Alt+V input moved focus to another application on this workstation, so that chord was not qualified here.

At integrated GPUI commit `692c7ad`, the ordinary-user WSL2 X11 pass exercised both Light and Dark and Ctrl+O. The real GTK portal chooser measured 1096×843 and its transient hint identified the launcher. The chooser accepted the fixture's local path through its AT-SPI location field and returned it to Carbon. The 480×425 Import modpack review also identified the launcher as its transient owner. Two Tab presses followed by Space activated Import pack and the new Forest settings instance appeared in the persisted profile index. The document-portal FUSE mount was unavailable in this WSL environment. The exercised application had local access to the chosen fixture. The final accessibility integration has its own qualification.

On Windows at `0867142`, actual desktop captures show the composited File and View labels and the caption text in System (resolved Dark on the test machine), Light, and Dark after Light. The physical File click opened the native Windows popup menu in each mode. While the protected sample was open, WM_CLOSE opened a separate Carbon-styled alert with the launcher as its HWND owner. Enter dismissed the alert, **Add one** followed by **Save and close** persisted sample count 2, and Ctrl+W then closed the launcher. The screenshots are in `.artifacts/gpui-menu-final-qa/`, including `native-menu-system.png`, `native-menu-light.png`, `native-menu-dark.png`, `native-menu-file-open-dark.png` and `protected-close-guard.png`. Windows High Contrast was not exercised.
