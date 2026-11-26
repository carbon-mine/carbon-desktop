# Prepare a personal instance before game launch

The owner selected the split Library and found that saving a profile left no clear next action. S1 requires vanilla and Java preparation before an entitled standalone game can open. The profile UUID and its pinned Minecraft version are sufficient to begin this work while live sign-in is pending.

## Decision

Use native File and View menus, platform window decorations and a compact split workspace with Overview, Setup and Settings. Open personal instances on first use. Keep disposable update demonstrations in Review tools. The renderer requests named operations for the selected UUID. Rust resolves the official version metadata and chooses the required Java major. The player can prepare the pinned Temurin runtime, select an installed Java home, or import the exact client JAR through a native picker. Reopening a profile checks its cache offline. A network miss requires an explicit setup action.

The application owns its provider-specific content caches. Preparing Java preserves the publisher's notices, verifies the archive and extracted files and checks cancellation before activation. Import verifies the client file against its official metadata and publishes it atomically. The same version can reuse these bytes across independent profiles. The native core keeps directory paths, launch decisions and all writes behind the bridge. A local Java home is a candidate identified by passive metadata. Its declarations, on their own, grant no permission to execute.

## Consequences

Players can complete part of S1 with public metadata and a licensed runtime while awaiting approved account registration. The client JAR picker requires the player to supply a matching file. A complete playable installation also requires platform libraries, assets, a qualified protected runner and account authorization. Packaging the application does not include any Temurin or Minecraft payload. Provider changes require reviewing the pins, notices and test matrix.

Keep the existing profile schema for compatibility. Java preferences live alongside it, keyed by UUID under the same writer lock. The renderer refreshes measured state after successful writes and clears old results when an explicit check fails. The [vanilla adapter](../vanilla.md), [Java runtime](../runtime.md), [profile store](../profiles.md) and [desktop design](../design/README.md) own the verification details.
