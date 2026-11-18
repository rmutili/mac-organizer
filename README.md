# Mac Organizer

A macOS app that uses Claude to tidy a folder: Downloads by default, or Desktop, Documents or any folder you pick. It shows where the space goes, proposes folders, and suggests what can go to the Trash. You review every change in a Finder-style explorer before anything moves, and the whole thing can be undone.

## What it does

- **Scan.** Pick a folder, and it lists every top-level item in it, with size, file count and age, and shows a heatmap of space or file count by file type and age. Exact duplicates are found by comparing file contents (SHA-256), not by guessing from names. Files that look like credentials or keys are flagged.
- **Organize.** Claude first designs a folder list from all the file names, then sorts every item into those folders and suggests Trash candidates, each with a reason and a confidence.
- **Review.** A three-pane explorer: review lists and folders on the left, sortable files with Move / Keep / Trash on the right of each row, and a Quick Look preview with Claude's reasoning.
- **Apply and undo.** Approved moves and Trash actions are applied together. Every operation is logged, so **Undo** restores everything, even after you restart the app.

## Safety model

- **Claude only proposes.** It never touches the filesystem, and nothing changes until you press Apply.
- **Only file metadata is sent to Claude** (names, types, sizes, ages), never file contents. File names are treated as untrusted input.
- **Every suggestion is checked against the scan** before you see it.
  - At least one copy of every set of identical files is kept by default.
  - Credential-like files are never suggested for the Trash.
- **"Delete" always means the macOS Trash.** Moves never overwrite existing files.
- **Only safe folders can be organized.** The home folder itself, `~/Library`, system folders, hidden folders, app and photo-library packages, and code projects (a `.git`, `package.json` or `Cargo.toml` at the top) are refused, because moving their top-level items would break them.
- **The web UI can't touch your files.** It refers to items by ID, and the Rust side resolves them against its own scan.

## Requirements

- macOS
- [Rust](https://rustup.rs) and [pnpm](https://pnpm.io)
- Either [Claude Code](https://claude.com/claude-code), signed in (used by default, with no API key needed), or an Anthropic API key, which is stored in the macOS Keychain.

## Development

```bash
pnpm install
pnpm tauri dev        # run the app
pnpm test             # frontend tests (Vitest)
cd src-tauri && cargo test --lib   # backend tests
pnpm tauri build      # build the .app / .dmg
```

macOS asks for permission the first time you scan Downloads, Desktop or Documents.

Built with [Tauri 2](https://tauri.app), React and TypeScript.

## License

[MIT](LICENSE)
