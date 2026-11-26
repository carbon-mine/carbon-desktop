# Owner review

| Build or screen | Task | Owner's wording | Decision and follow-up |
|---|---|---|---|
| M0 interactive layout comparison | Select the first launcher layout | “A · Split library — selection and detail side by side, with Activity visible (recommended)” | Adopted. The narrow view returns to the selected library entry. |
| M0 local recovery fixture | Resolve missing sync contract | “An interrupted update to B should leave A playable, and applying B should preserve personal data.” | Keep A selected and readable after interruption. Retry verifies B before activation. Separate signed-registry and gameplay evidence remains pending. |

## Candidate walkthrough

1. Resize the window and press Ctrl+F. Select Forest and find **Check Play requirements**. It explains the missing live prerequisites. Look for Activity before and after changing selection.
2. Create a personal vanilla profile, pin a game version, edit memory, then clone it. Change the copy's settings and return to the original. Preview the generated `forest-preview.mrpack` through the file picker. World creation and live account selection are pending.
3. Install an exact repeat and Coast. Compare the reported source and disk figures, edit one fixture setting or sample save, then select the other sample. The built-in sample reads bundled fixture bytes. The independently measured loopback benchmark is in [storage](storage.md).
4. Review Forest's A → B changes. Simulate interruption, navigate to another profile, then return through Activity. Resume and activate. Verify the independent save and settings.
5. Under **Sample settings, save and fault controls**, simulate a missing file, repair it, and inspect the result. Use the keyboard to return to the selected item in the narrow library.

Automated walkthrough results are in the [build handoff](handoff.md).

On 26 November the Windows x64 debug candidate at local commit `693fc74` was opened with the disposable Forest sample. Native UI Automation observed the expected Library controls. The owner received the live walkthrough and a request for observations.

## Hands-on feedback on the Windows candidate

The owner viewed the profile screen and reviewed the walkthrough. They said, “I don't know what to do with the profiles. I can't go beyond the profiles.” The walkthrough looked okay to them. They requested substantially more UI work and observed, “It just seems like there's a lot more work to be done here.” They did not report completing the individual walkthrough steps or playing a world.

The personal detail screen offered **Edit profile** and **Clone profile**. Its **Setup pending** label left the owner looking for a way to continue. The build has no game installation or launch path.

Proposed next review: start from a fresh library, create a vanilla instance, and follow its main action through setup. Before live setup is available, show exactly where progress stops and whether the player can do anything about it. When the runtime, sandbox and entitled session are ready, the same journey should end with opening and reopening a disposable world.

The owner clarified that the original milestone should govern the work and that the launcher “should look like an app.” The follow-up keeps S1, the selected split library and platform window chrome. Native menus, a command bar and the selected instance's Overview, Setup and Settings replace the profile-only detail. Review tools holds the synthetic A/B demonstration.

For the next owner review, open **Vanilla survival**, choose **Check setup**, and follow the highlighted action. After Java is prepared, check whether the next available step and the reason Play is unavailable are clear. Resize the window and return to Library. The [handoff](handoff.md) identifies the development package and its native evidence. Owner feedback on this build is pending.
