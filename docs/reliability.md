# Saving and recovery on Windows

Dirty Markdown tabs checkpoint their UTF-8 source in the local nxmd profile, under `%LOCALAPPDATA%\nxmd\recovery-v1`. The normal writer coalesces edits, allows one background write at a time, and flushes a pending generation before a save or a conflict-close. An edit can still be lost if the process is killed before its checkpoint completes. An unavailable recovery location blocks saving the affected dirty tab and reports the error.

On relaunch, choose **Restore draft** or press **Ctrl+Shift+R**. The separate review window shows the original path and whether its baseline still matches disk. Closing review keeps the record. Editing the recovered text checkpoints a new generation. **Save as copy** requires a new Markdown filename. A completed copy retires the corresponding generation. **Discard draft** requires confirmation. The app retains invalid or unrecognized records for inspection. OS file locks keep live instances from claiming each other's drafts.

An external change pauses autosave for that tab. Inspect the disk text, save the editor text as a separate file, or keep the draft and reload or close the tab. A failed save blocks closing a dirty tab or the workspace. Recheck with Ctrl+S after resolving an access error. Conflict resolution does not merge text.

Windows saves compare the actual disk bytes to the loaded baseline and skip replacement when all three versions match. A changed save flushes a temporary file and calls ReplaceFileW with a displaced-file backup. It checks both the displaced contents and the installed contents before acknowledging success. A detected race or ambiguous OS result leaves a transaction directory beside the note. Its target.json identifies the intended destination, editor is the staged version if still present, and displaced is the original if Windows moved it. The error identifies the directory. Preserve these files, the note, and the profile draft before resolving the conflict. The app blocks further saves through that target while the transaction is present. Independent programs can change a file again after a completed verification.

During IME composition the editor displays provisional input and retains the original selection. Autosave and crash recovery use committed text. A commit becomes one undo step. Cancellation restores the previous selection and contents.

On Windows, moving a note checks its file identity and bytes against the file selected before the destination dialog. The move holds a handle that excludes writes and deletion and renames through that handle on the same volume. An existing destination, externally replaced source or incompatible open handle causes the move to fail. The tab adopts the new path only after success.

## Regression fixtures

| Reproduction | Cause | Fix and coverage |
| --- | --- | --- |
| Save an untouched note, then write through a pre-existing hard link | Save replaced the file even when its bytes were identical | Check disk, editor and baseline before staging. Test timestamp, hard-link identity and subsequent external-write rejection. |
| Start a list at 7, nest another list, then continue the outer item as an indented paragraph after a blank line | The continuation cleared the complete counter stack | Pop only completed levels. Compare the loose-list fixture with CommonMark using 3 spaces for the outer body and 6 for the inner body. The next outer item displays 8. Source ordinals stay intact. |
| Put 65,536 inline code spans on a single line and reveal the last span | A 16-bit pair ID overflowed. Repeated searches through the suffix also made the caret path quadratic on adjacent spans | Use source-sized IDs and pair markers in a forward pass. Check the final revealed source. |
| Replace the note between preflight and OS replacement | Preflight alone could acknowledge overwriting an independent writer | Inject the external write immediately before a real Windows replacement. Assert conflict and check the bytes at both surviving paths. Exercise documented partial failure outcomes independently. |
| Kill the program with a dirty, externally modified note, restore it, then kill it again | Unsaved state previously existed only in the process | Windows GUI smoke kills three processes and compares source, repeated restore, edited draft and separate copy. |

The native smoke suite also performs reproducible clipboard edits with seed 73217 against an independent expected source. Run the standard Windows release workflow or run `scripts/windows-smoke.ps1 -Executable target/release/nxmd.exe` in an interactive Windows PowerShell 7 session. All writer and destructive tests use temporary vaults.

Directory moves are deferred. Existing recoverable trash behavior is preserved. Power-loss durability depends on the filesystem and device. The CI suite exercises process death, simulated partial replacements, a real sharing violation, malformed records, missing files, source preservation and external writers. It does not exercise physical disk failures, every Windows IME, or every network filesystem.
