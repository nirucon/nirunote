# Notes for agents working on NIRUNOTE

NIRUNOTE 1.x is a Rust/x11rb program with its own canvas, in the style of
NIRUFM and NIRUTERM. Read `README.md`, `RELEASE-CONTRACT.md` and
`CHANGELOG.md` first.

## Layout of `src/`

- `main.rs`: command line (compatible with 0.x), self-tests.
- `app.rs`: the window, commands, menus, dialogs, files, recovery, spelling,
  theme and the event loop (poll on the X fd, an eventfd for workers,
  inotify for the open file and the NIRUX theme; no timers while idle).
- `editor.rs` and `doc.rs`: the source editor (layout, highlighting,
  editing, undo).
- `markdown.rs` and `codehl.rs`: a port of 0.7's renderer; keep it
  byte-compatible with the 0.7 HTML (`tests/fixtures/showcase.html`).
- `preview.rs`: lays the HTML subset out on the canvas; `images.rs` decodes
  images.
- `settings.rs`: the Qt `QSettings` INI file; `files.rs`: load, atomic save,
  snapshots, recovery; `spell.rs`: Hunspell; `theme.rs`: palettes and Follow
  NIRUX; `fonts.rs`, `canvas.rs`, `x.rs`, `ui.rs`, `dialogs.rs`, `logo.rs`.

## Rules

- Users' documents are never lost or silently changed (BOM, line endings,
  encoding, recovery). Existing settings and recovery files must keep
  working.
- Never touch the owner's running instance, settings or scratchpad; test
  with a temporary HOME and XDG directories in a private Xvfb
  (`tests/xvfb.sh PATH DISPLAY`).
- Gates before a release: fmt, clippy `-D warnings`, tests, `tests/xvfb.sh`.
