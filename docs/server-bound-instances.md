# Server-bound instances

An instance can store one optional Minecraft Java server address. In **Settings**, use **Set server address**. Enter a DNS hostname, an IPv4 address, or an IPv6 address in brackets. An explicit port is optional. For example, use `play.example.org:25565`, `192.0.2.40`, or `[2001:db8::1]:25565`. The examples are reserved for documentation. Select **Save changes** to apply it, **Cancel** to leave it alone, or clear the address and save to remove it.

A bound instance shows the address and local pack in **Multiplayer** on its Overview and Settings. **Copy server address** sends the exact saved text to the desktop clipboard. Opening the editor, selecting an instance and copying an address do not start Minecraft, reach the server, look up DNS, sync a pack or download content. To try the server manually, paste that address in Multiplayer in a compatible Minecraft installation you open yourself.

Carbon does not currently offer a working **Join server** or **Open Minecraft** for these instances. The launcher explains when the local pack needs supported client files or a qualified loader. An address does not certify that the destination is online, that a local pack matches it, or that an account can join.

## Data and platform behavior

Older profiles with no address load normally. Binding, changing, or clearing preserves the instance ID, game directory, save data, settings, and the exact installed profile and pack pins. The dedicated shared operation compares the address observed when the editor opened against the current record under the profile lock, then updates just that field. Profile settings edits also preserve the saved address. A normal clone and a new pack import start unbound. Repeating the same already committed import preserves the original profile and its binding. The existing restriction on cloning an imported pack still applies.

GPUI uses its owned application dialog and platform clipboard. Svelte/Tauri asks its native Windows or GTK dialog to validate the address and uses named commands for saving and copying; passive data comes with the loaded profile. Where native Tauri editing or clipboard integration is unavailable, its command reports that state. The displayed address remains selectable in the renderer.

## Verification scope

- CDESK-80 covers binding and editing only. The shared Rust tests exercise normalization, old-profile compatibility, conflicting changes, two independent profiles, retained saves, imported pack pins, repeat import, clone and clearing.
- The Svelte headless Edge suite checks that the native bridge receives the selected instance ID and expected current address, cancellation restores focus, no other instance sees the binding, the copy request carries no launch or network action, and the local pack and blocked join guidance stay visible.
- CDESK-27 still needs an approved account reference and entitlement check, a protected runner, an exact qualified game and loader combination for Quick Play, and an independent connection verification. Only offer **Open Minecraft** once an ordinary protected launch is available.
- CDESK-26 still needs a real authorized online-mode server and matching local pack, native Windows and native Linux, and approved application registration. Evidence must distinguish process start, connection attempt, and confirmed in-game join. Include a stopped or unreachable server, a mismatched pack, and the manual fallback. A WSL or JVM process start alone does not qualify the join.
