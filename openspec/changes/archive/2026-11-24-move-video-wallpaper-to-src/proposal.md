## Why

The repo hosts a single crate but buries it in `crates/video-wallpaper/` with a one-member `[workspace]` indirection. Per AGENTS.md «Файлы», a flat `src/` catalog of concept files is the project layout; the crate wrapper is dead weight. A pre-move review of the sources also found rule violations (panics via `unwrap`/`expect` in working code, a string-sentinel error, dead files, mega-files mixing responsibilities) that should be fixed in the same change.

## What Changes

- **BREAKING** (repo layout only; binary behavior unchanged): move the package to the repo root — `crates/video-wallpaper/src/` → `src/`, crate `Cargo.toml` merged into the root `Cargo.toml` (the `[workspace]` table is removed, `edition`/`license`/`rust-version` inlined), `crates/` deleted.
- Move `shader.wgsl` with the sources; `include_str!` path stays relative to `src/`.
- Review fixes folded into the move (details in design.md):
  - delete the empty `compositor.rs` (one doc-comment line, module declared but unused);
  - remove all `unwrap`/`expect`/panic paths from working code (`Mutex::lock` poisoning, `CString::new` on the render node, `pending.take()`, `frame.unwrap()` in `App::configure_entry`, `blur_targets.unwrap()` in `Renderer::ensure_blur`, thread-spawn `expect` in `niri::spawn`/`gamemode::spawn`);
  - replace the `"__eof__"` string-sentinel error in `Decoder::next_frame` with a typed enum outcome matched by `match`;
  - stop treating lock poisoning as fatal: `unwrap_or_else(PoisonError::into_inner)` on shared `Mutex` guards;
  - split files that mix responsibilities per «Файлы»: `decoder.rs` (758 lines: FFI wrappers + Control + Mailbox + Worker + thread handoff) and `app.rs` (879 lines: SurfaceEntry + OutputPair + App + calloop wiring) into flat per-concept files;
  - drop the unused `queue` parameter in `Renderer::import`;
  - remove explanatory comments/doc-comments per AGENTS.md (meaning moves into names) during the split.
- No changes to user-facing behavior: config format, namespaces, CLI flags, transcode cache, Wayland surfaces, game-mode and overview logic stay as specified.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `video-wallpaper/playback`: startup failure handling becomes a requirement — any startup failure (decoder open, transcode plan, thread spawn, instance lock) must exit the process with a diagnostic error, never a panic.
