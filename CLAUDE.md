# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Product

A macOS app that uses Claude to organize a folder: `~/Downloads` by default, or Desktop, Documents or a folder picked in the native dialog. Scan the folder locally and show a heatmap of space and file counts by type and age. Claude then proposes folders and Trash suggestions, the person reviews every item in a Finder-style explorer, and the app applies the approved changes with a full undo.

Stack: **Tauri 2 + React/TypeScript (Vite, pnpm)**. `src/` is the web UI and `src-tauri/` is the Rust backend. Rust is installed with rustup; if `cargo` isn't on `PATH`, run `source "$HOME/.cargo/env"` first. pnpm 11 only runs dependency build scripts listed under `allowBuilds` in `pnpm-workspace.yaml`.

## Commands

```bash
pnpm install                 # frontend deps
pnpm tauri dev               # run the app (Vite + Rust, hot reload)
pnpm tauri build             # release .app/.dmg
pnpm build                   # type-check + build the UI only
pnpm test                    # Vitest (src/lib/*.test.ts)
pnpm vitest run src/lib/review.test.ts -t "toggleTrash"   # one frontend test

cd src-tauri
cargo test --lib                             # Rust unit tests
cargo test --lib walk::tests::skips_hidden   # one test (name substring)
cargo test --lib live_ -- --ignored          # live tests: real Claude Code CLI + real Trash
cargo clippy --all-targets -- -D warnings
cargo fmt
```

CI (`.github/workflows/ci.yml`, macOS runner) runs the frontend tests, `pnpm build` (which creates the `dist/` that `tauri::generate_context!` needs to compile), fmt, clippy and `cargo test --lib`. The `live_` tests are not run in CI.

`pnpm tauri dev` serves the UI at `localhost:1420`, but opened in a normal browser that page has no Rust commands. Use the app window.

## Process boundary (Tauri)

- **Rust owns everything privileged:** scanning, hashing, moving files, Trash, the undo log, Quick Look, Keychain, and calls to Claude. The webview never sees paths it could act on or the API key.
- **Commands take IDs, not paths.** Every command resolves item IDs against the `Scan` held in `AppState`. `apply_plan` takes `{moves: [{item_id, folder}], trash: [item_id]}` and rejects the whole request if any ID or folder isn't in the current scan or plan.
- Don't grant the webview the `fs` or `shell` plugin permissions in `src-tauri/capabilities/`. The release CSP is in `tauri.conf.json` (`devCsp` is null so Vite HMR works); previews come back as `data:` PNGs.
- **Item IDs belong to a single scan.** A rescan clears the plan, a plan that finishes after a rescan is discarded, and Apply and Undo clear both, so the UI must rescan afterwards.

## Pipeline

1. **Scan (`scanner/`, offline).** `scan_folder` takes a `Location` (`downloads`, `desktop`, `documents` or `picked`), never a path. `pick_folder` opens the native dialog from Rust (`tauri-plugin-dialog`, used Rust-side only; the webview isn't granted its commands) and stores the result in `AppState.picked`. Every root, standard or picked, goes through `scanner/roots.rs::check_root`, which refuses the home folder, anything outside home or `/Volumes`, `~/Library`, hidden folders, macOS packages and code projects; extend its tests when changing the rules. An item is a **top-level entry**; a subfolder is one item, moved as a unit, with its size and file count summed recursively. Hidden entries and symlinks are skipped, and unreadable entries go to `Scan::skipped`. IDs are indices into the name-sorted list. After listing, `duplicates.rs` hashes (SHA-256) only files that share a size and sets `duplicate_group`. An unreadable file counts as unique, the safe direction. `sensitive.rs` flags credential- or key-like names. The heatmap is a fixed category × age grid (mtime buckets), with zero cells included.
2. **Classify (`claude/classify.rs`).** First, one request (`build_folders_request`) sends every item's name and kind, and Claude designs 6–12 folders. Every batch request (150 items, four at a time) then restricts `folder` to those names through a JSON-schema `enum`, so batches can't invent near-duplicate folders. Requests carry metadata only, never file contents. Both requests include the folder's name (not its path), and the prompt asks for far fewer Trash suggestions outside Downloads. `duplicate_group` tells Claude which files are verified copies. Any failed step fails the whole run.
3. **Validate (`claude/plan.rs::validate_plan`).** This is the safety boundary, so extend its tests whenever the rules change. It drops unknown IDs, unsafe or duplicate folder names, folders that collide with a file, multiple assignments for one item, and moves of a folder into itself. It never suggests trashing a target folder or a `sensitive` item. When every copy in a duplicate group is flagged, it keeps the one with the cleanest name (`keep_one_copy`).
4. **Review (UI).** `src/lib/review.ts` holds the decision for each item (`move`, `keep` or `trash`) as pure, immutable functions with tests. Only high-confidence suggestions and verified duplicate copies start out marked for the Trash.
5. **Apply / Undo (`ops/`).** Moves create folders as needed and never overwrite: a clash becomes `name (2).ext` (`unique_destination`). A file or symlink sitting where the folder should be is refused. Trash goes through `NSFileManager.trashItem` (`ops/trash.rs`), which returns the item's location in the Trash so it can be undone; tests use `FakeTrash` behind the `Trasher` trait. Every successful operation is written to an `OperationLog` saved in `<app data>/undo/<timestamp>.json`. Undo reverses the newest log in reverse order, never overwrites a file that has since taken the original name, removes created folders only if they're empty, and renames the log to `.undone.json`.

## Reaching Claude (two backends)

Both backends feed the same `classify()` and the same validation, and `send` turns a Messages-API-shaped body into a `RawPlan`.

- **Claude Code CLI (`claude/cli.rs`), the default when installed.** It runs `claude -p` with the person's Claude login, so no key is needed. The tool's `input_schema` goes in through `--json-schema`, and the result is read from `structured_output`. Keep the lock-down flags: `--tools ""`, `--strict-mcp-config`, `--setting-sources ""`, `--no-session-persistence`, run from the temp directory, with metadata over stdin. Finder-launched apps don't inherit the shell PATH, so `locate()` checks the usual install directories first.
- **Messages API (`claude/client.rs`), the fallback.** The key is stored in Keychain (`keychain.rs`) and is only ever read in Rust. The plan comes back through forced `tool_choice`, and `parse_plan` reads the first `tool_use` block.

The default model is `claude-haiku-5-5`, and the UI also offers `claude-opus-5-5` (`ALLOWED_MODELS` is the allowlist). Progress reaches the UI as Tauri events:
- `scan-progress` sends `{phase: listing|duplicates, done, total, name}`.
- `classify-progress` sends `ClassifyProgress`: steps done and total, with step 0 being folder design, plus up to five names from the batches in flight.

## Invariants

- **Claude proposes, the person applies.** Claude never touches the filesystem.
- **Nothing is destroyed.** "Delete" means the macOS Trash, never `remove_file`, and every Apply is logged so it can be undone.
- File names are untrusted input; prompts say so explicitly.
- **Cancel and quit stop Claude.** Every `claude -p` child is registered in `AppState.processes` (`claude/processes.rs`); `cancel_classify` and `RunEvent::Exit` SIGTERM them, and a cancelled run returns the exact error `"Cancelled."`, which the UI doesn't show as an error. With the API-key backend, requests already in flight still finish; no new ones start.
