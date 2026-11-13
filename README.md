# 🦆 pato

A fast, lightweight remote GitHub repository, file, and tree explorer TUI with Miller columns, inspired by `yazi`.

Browse personal accounts, organizations, repositories, directory trees, branches, code files, and images remotely with zero clone latency.

---

## Installation

### Quick Install (from Source)

```bash
git clone https://github.com/heelgueta/pato.git
cd pato
cargo build --release
install -m 755 target/release/pato ~/.local/bin/pato
# or: cargo install --path .
```

Make sure `~/.local/bin` (or `~/.cargo/bin`) is in your `$PATH`.

### Authentication (Zero-Config)

`pato` automatically reads your token from the official GitHub CLI:
```bash
gh auth login
```
Alternatively, set the standard environment variable:
```bash
export GITHUB_TOKEN="ghp_your_personal_access_token"
```
*(If unauthenticated, `pato` operates in public-only mode with GitHub's default rate limit of 60 req/h).*

---

## Keybindings

| Key | Action |
| --- | --- |
| `h` `j` `k` `l` / Arrows | Navigate list, step into (`l` / `Enter`) or back (`h` / `Backspace` / `Esc`) |
| `J` / `K` | Scroll preview pane down / up (5 lines) |
| `#` | **Toggle line numbers** on code files (`ON` / `OFF`) |
| `m` | **Bookmark / save repository** (adds to `⭐ Saved Repositories` root list) |
| `r` | **Remove** saved repository or custom user/org from list |
| `b` | **Switch branch** (choose from available remote branches) |
| `s` | **Cycle sort mode** (Updated / Created / A-Z / Status / Archived / Fork / Lang) |
| `c` | **Git clone** repository into current launch folder |
| `C` (`Shift+c`) | **Copy clone URL** (`.git`) to clipboard |
| `d` | **Download** selected remote file into current launch folder |
| `D` (`Shift+d`) | **Copy web URL** of selected item to clipboard |
| `u` | **Explore & Remember**: search/add any user, org, or enter `owner/repo` to bookmark directly |
| `R` (`Shift+r`) | **Refresh** current view and re-fetch from GitHub |
| `o` | Open repository, file, or account in web browser |
| `/` | Live substring / fuzzy filter in active column |
| `?` | Toggle keybinding cheat sheet |
| `q` / `Esc` | Quit or close open dialog |

---

## Saved Repositories & Bookmarks

`pato` lets you keep direct references to repositories without needing to navigate via an account or organization:

- **Bookmark on the fly**: Highlight any repository and press `m` to bookmark or unbookmark it.
- **Direct entry**: Press `u`, type `owner/repo` (e.g. `ratatui/ratatui`), and press `Enter` to bookmark it immediately.
- **Direct navigation**: Saved repositories appear right at the root accounts column under `⭐ SAVED REPOSITORIES`. Selecting one jumps straight into its directory tree.
- **Remove saved items**: Press `r` on any saved repository or custom user/org to remove it from your saved list.
- **Persistent storage**: Saved repositories are stored locally in `~/.config/pato/saved_bookmarks.json`, and remembered users/orgs in `~/.config/pato/saved_accounts.json`.

---

## Features

- **Ink Monochrome Theme**: Crisp black-and-white terminal palette (`#000000` BG, `#f0f0f0` FG, `#828282` DIM, `#1c1c1c` PANEL, `Border::Double`).
- **Native Markdown Renderer**: Renders READMEs and `.md` files with formatted headers, list bullets, blockquotes, and fenced code blocks.
- **Code Preview & Line Numbers**: Syntax highlighting via `syntect`, top metadata summary bar (`lines`, `size`, `syntax`), and toggleable line numbers (`#`).
- **Terminal Image Previews**: 24-bit half-block (`▀`) graphics preview for PNG, JPEG, WEBP, GIF, and BMP files.
- **Branch Switcher**: Seamlessly query remote branches with `b` and switch trees on the fly.
- **Yazi-style Cursor Memory**: Returning to parent folders or repo lists preserves your exact previous cursor position.
- **Auto-Pagination**: Automatically follows RFC 5988 Link headers up to 1,000 repositories so full lists are sorted and filtered accurately.
- **Live Rate-Limit Badge**: Displays live `API: <remaining>/<limit>` in header, warning when running low.
