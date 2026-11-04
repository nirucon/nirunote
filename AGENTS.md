# Agent assignment: NIRUNOTE 0.6.0, a one-shot correctness, efficiency and maintainability update

## 0. Quick start

1. Read this file, `README.md`, `RELEASE-CONTRACT.md`, `CHANGELOG.md` and `nirunote/*.py`.
   The audit baseline is `e48d644` (0.5.0, pinned as a NIRUX extra). Revalidate each item
   against current HEAD and skip items that are already fixed, noting that you did.
2. Implement **T1–T6, then U1–U4, then M1 last**. Commit per item. Keep CI green:
   `compileall`, `nirunote/app.py --core-self-test`, `gui-smoke-test.py` and
   `scripts/release-check.py` (all offscreen).
3. Pass the gates in §4, release in §5 and report in §6.

This is an **execution** assignment. Do not stop after a plan or a branch. Make sensible
decisions, document them, and continue without routine questions.

## 1. Invariants

- Documents are never lost or silently changed. That covers BOM, line endings,
  encoding, the save-as flow and recovery.
- Keep the external-change detection (file watcher) and the safe Markdown preview
  (blocked unsafe links).
- Theme "Follow NIRUX" keeps working, and so does the standalone fallback outside NIRUX.
- Python and PySide6 (≥ 6.7) requirements stay as today. Add no new required dependency.
- The owner runs NIRUNOTE all day. Do not touch the owner's running instance, settings,
  recovery or scratchpad. Use a temporary `HOME`/`XDG_*` for every test.

## 2. Bugs (verified 2026-11-04 on niruxwork, in a separate Xvfb with a temporary HOME)

### T1: Opening a file from the command line marks it as modified (high)
`nirunote test.md` showed **"Modified"** in the status bar and the ● dirty marker right
after opening an unchanged file. `recovery.json` was written with the full document.
Consequences:
- a needless "unsaved changes" prompt when closing,
- recovery writes (see T2),
- and possibly a misleading "recovered" state on the next start.

`load()` sets `_loading=True` around `setPlainText` and clears the marker, so something
fires `changed()` afterwards: the highlighter/spell pass, cursor restore,
`update_document_stats`, or the explicit-file path in `main()`.
**Fix:** find the real trigger, add a regression test (open → no dirty state, no
recovery file), and check the same for File → Open, Recent, Scratchpad and the
external-reload paths.

### T2: Recovery rewrites the whole document every 2.5 s while dirty
`self.timer.start(2500)` → `recover()` writes the full text atomically every 2.5 s as
long as the document is dirty, even when nothing has changed since the last snapshot.
That means constant SSD writes for an idle, unsaved document.
**Fix:** keep a text generation counter and write only when it changed since the last
recovery write. Debounce to about 2 s after the last edit, with a maximum delay of 10 s
while typing. Write nothing when nothing changed.

### T3: The idle editor still costs CPU
The owner's live instance used about **1.8 % CPU and 26 wakeups/s** while idle in the
background. In Xvfb with focus it used about 1.3 % and 20/s, and that is with T2 doing
nothing in the background test. Find the source: cursor blink, timers, the theme
watcher, the spell timer, or stats/preview re-entrancy. **Target:** an unfocused idle
window ≤ 0.2 % CPU and ≤ 3 wakeups/s. A focused idle window ≤ 0.5 %, where only the caret
blink should remain.

### T4: The status text is cryptic
`SPELL?` in the status bar does not tell the user anything. **Fix:** show
"Spelling: off", "Spelling: sv", or "Spelling: unavailable (install hunspell-sv)", and
make it clickable to open the spelling settings.

### T5: Stale branches
`feat/phase-1…5`, `perf/phase-3-async`, `refactor/phase-6-modules` and `release/0.5.0`
are all merged into `main`. **Fix:** delete these remote branches. Tag them first only if
a tag of the same name does not already exist.

### T6: The CI and release scripts
Make sure CI runs on pull requests as well as pushes, and that it uses the same Python
version as Debian 13 (3.13) in addition to the current one.

## 3. UX

### U1: An honest dirty state
The ● marker and "Modified" appear only after a real edit, and Undo back to the saved
state clears them. Use `QTextDocument.isModified`/`setModified` instead of
string-comparing a label.

### U2: Recovery prompt
After a crash, show what was recovered (file name, time, size difference) with
**Restore / Discard / Compare**. Compare opens a simple side-by-side diff in a dialog.

### U3: Outline navigation
Add a heading outline popup (Ctrl+Shift+O) that lists `#` headings. Enter jumps there,
and it supports type-to-filter.

### U4: Readable code blocks
Add fenced code block highlighting in the editor, in monospace with a subtle background.
No language parsing.

### M1: Readable source (last, behavior-neutral)
`nirunote/app.py` has 18 lines over 300 characters, the longest about 1 860 characters.
Whole widget setups are written as single `;`-joined lines, which makes review and
change risky.
- Reformat into normal statements in mechanical commits, with no logic change.
  `ruff format`/`black` with the line length set in `pyproject.toml` is fine as long as
  it is only a dev dependency.
- Split `Main` into editor, file I/O, preview and settings modules where it is natural.
- All tests pass after each commit.

Out of scope: sync or cloud, plugins, WYSIWYG editing and new file formats.

## 4. Gates

Automated:
- The existing CI steps.
- New regression tests: the T1 open paths, T2 recovery write counting (patch
  `atomic_write`), U1 modified/undo, U3 heading extraction.
- An offscreen idle check that counts timer firings over 10 s with a document open.

Native, on **niruxwork** (the owner allows local testing):
- Run a **second** instance with a temporary HOME. Never quit or reconfigure the owner's
  open NIRUNOTE.
- Check T1 through every open path.
- Measure CPU and wakeups for T3, unfocused and focused.
- Check the recovery flow with kill -9.
- Check Follow NIRUX after a theme change.

## 5. Release

Bump `VERSION` to **0.6.0** and update `CHANGELOG.md` and `RELEASE-NOTES.md`. Follow
`RELEASE-CONTRACT.md`: tag `v0.6.0` and create the release with the existing scripts.
Update the NIRUX extras pin in `nirucon/nirux`: `docs/EXTRAS.md` plus the pinned
version/checksum line in `nirux` (see how `0.5.0` is pinned). Install with
`nirux extras update`, or the documented equivalent, and verify the version shown by
`nirunote --version`. Tell the owner to restart their running NIRUNOTE. Do not kill it.

## 6. Final report

For each item T1–T6, U1–U4 and M1: the change, the tests and the evidence (the T1 root
cause, CPU and wakeups before and after, recovery writes per minute before and after).
Include release URLs, pinned versions and remaining limits. Mark anything unfinished as
**PARTIAL** and give the exact remaining steps.
