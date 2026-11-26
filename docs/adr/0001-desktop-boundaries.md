# 0001 · Desktop owns the library and execution boundary

Accepted for the first development build, 26 November 2026. The player must be able to author a personal profile while the registry and identity provider are unavailable. Installers and mod code can run native code. Giving them launcher permissions would grant access to other instances and reusable credentials.

## Decision

Keep one app-selected local library root. Personal authoring stores only player-selected metadata under `personal/`. The fixture installer keeps manifests, immutable bytes, private data and backups in their own regions. The webview can request named actions and receive display records. Only the trusted native shell selects the data root and opens the system file picker. User-selected pack metadata carries no authority to download or launch anything. An arbitrary display name never becomes a filesystem path.

A small in-house Rust adapter exercises the storage and policy seams. PortableMC and XMCL remain candidates for version and loader resolution once their setup execution can be routed through the same qualified boundary. The experiments run a fixed benign program under Windows AppContainer and a fixed filesystem probe under Linux bubblewrap. The user-facing app refuses game execution at this checkpoint. The account fixture has no credential storage or network connection. The managed fixture models two local generations, and carries no publisher or registry trust.

An immutable cache address is the expected digest. Independent player roots and a protected backup survive generation changes. The split Library layout exposes the selected profile and persistent Activity. The owner chose this design after comparing three spatial alternatives. Native window chrome and a Desktop-owned copy of the Panel foundations work independently of any online Carbon service.

## Consequences and verification

The app can create, edit and clone profiles, preview pack metadata, and demonstrate interrupted updates without touching existing Minecraft installations. A complete launch still needs verified game and runtime inputs, isolated setup and play, and an approved account broker. An authenticated join additionally needs approved network routing. The preview does not materialize game files into a writable runtime view. The sync workstream owns the versioned release and trust contract.

Revisit the launch adapter after an entitled standalone run and the shared filesystem view after native game access tests. See [storage](../storage.md), [account policy](../account.md), [profile authoring](../profiles.md), [critical review](../critical-review.md), [design](../design/README.md) and [the product plan](../../../carbonpanel/docs/research/carbon-desktop-requirements-and-development-plan.md).
