# Library folders

Folders organize the native library into named groups. They apply separately to **Instances** and **Worlds**. A folder is app-owned metadata: filing, renaming, moving or removing one never renames a world directory, moves a save, copies world data or changes launch preferences.

## Storage contract

The Rust profiles service owns `personal/folders.json` beside `profiles.json`:

```json
{
  "version": 1,
  "revision": "5b1c0e0e-0f1c-4a4e-9e20-70d0e2b1f8a6",
  "folders": [
    {
      "id": "8a2f6e70-7a5c-4f92-a3b5-5d6b1f2c9c11",
      "label": "Survival",
      "scope": "worlds",
      "members": [
        { "kind": "world", "instance": "b79dbad7-d038-46ce-b5a4-000000000000", "directory": "Home" }
      ]
    }
  ]
}
```

- A folder has a stable random UUID, a label, one scope and ordered members. Array order is display order.
- `instances` folders hold `{"kind":"instance","id":...}`. `worlds` folders hold `{"kind":"world","instance":...,"directory":...}`. A member kind is invalid in the other scope.
- A world member uses the shared world identity from [instance launch targets](instance-launch-targets.md): the backing instance UUID plus the exact save directory. It is never a display name, a full path or copied save data.
- Reads and writes use the existing `profiles.lock` and the same scratch-file, flush and rename publication used by `profiles.json`. A missing file means "no folders"; the file is created on the first change.
- The file is bounded to 4 MiB, 128 folders per scope and 4096 members per folder.

## Operations

| Operation | Effect |
| --- | --- |
| Create | Add an empty folder in one scope. Labels are unique within a scope, compared case-insensitively. The same label may exist in both scopes. |
| Rename | Change the label only. Folders and their members keep their IDs. |
| Reorder | Move a folder within its own scope. The other scope's order is untouched. |
| File / move | Put items in one folder. An item leaves its previous folder, so it is in at most one folder per mode. Filing requires the item to resolve now. |
| Unfile | Return items to the ungrouped list. Nothing is deleted. |
| Remove | Delete the folder record. Members become ungrouped and remain available. |
| Prune missing | Remove only references that a complete check confirmed missing. This is explicit and revision guarded. |

Every change takes the revision the window observed. A stale write fails with "Folders changed in another window" and the caller refreshes before retrying. Reads leave the revision unchanged.

## Missing references

Reads resolve every member without mutating the file:

- **Resolved**: the instance exists; or the backing instance exists and a complete save scan observed the exact directory.
- **Missing**: the instance no longer exists, or a complete save scan does not contain the directory.
- **Unknown**: the save scan could not inspect every entry, so absence is not proven.

Missing and unknown references stay in the folder. The UI can show a recovery row with **Remove from folder**; unknown references disable that action until a complete refresh. A folder removal always works and never deletes the referenced item.

## Empty and duplicate states

- With no folders, the library keeps its existing flat list. **New folder** is the only addition.
- A folder with no members remains and says it is empty.
- The ungrouped section is always available, including when every item is filed.
- Duplicate labels in one scope are refused without writing the file. Labels are trimmed and may not contain control or bidirectional formatting characters.
- Search is global: matching items appear even when their folder is collapsed.

## Keyboard and accessibility

The native list reuses the existing selection, search, scroll and Escape behaviour. Folder headings are disclosure controls: Enter or Space toggles, Left collapses and moves to the parent heading, Right expands, Home/End jump within the folder list, and F2 or the item menu renames. Focus never moves into a hidden member, and there is no second navigation stack or focus trap. Folder headings show the label and mode in their accessible names; the ungrouped heading is announced as such.

## Safety and verification

Folder operations accept only IDs, labels and typed references. They do not accept paths, contents or commands. The world reference is validated as one directory name without separators, `.` or `..`. Physical world directories are only inspected, never renamed, created, moved or removed.

Run the focused suite:

```text
cargo test --locked -p carbon-profiles
```

It covers the empty state, duplicate instance names, scoped typing, move/unfile, folder removal, byte-preserved profiles and saves, missing instances, renamed and restored world directories, partial scans, stale revisions, corrupt/oversized/unsupported files, wrong scopes, duplicate labels and a 100-instance library with 1000 world references.

The folder presentation is staged behind the library-mode work. The backend is additive and does not change an existing flat library.
