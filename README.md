# 🦆 pato

A fast, lightweight Yazi-inspired remote GitHub repository and file explorer TUI built with Rust, `ratatui`, and `crossterm`.

Browse GitHub personal accounts, organizations, repositories, directory trees, branches, and remote files with full syntax highlighting as if they were local folders.

---

## Highlights

- **Ink Theme**: True black background (`#000000`), crisp white (`#f0f0f0`), neutral grey borders (`#828282`), and inverted solid block highlights.
- **Yazi-style 3-Column Miller Navigation**: Context $\rightarrow$ Active list $\rightarrow$ Live async preview.
- **Branch Switching (`b`)**: Choose and switch to any remote branch on any repo on the fly.
- **Persistent Saved Accounts**: Added public users and organizations are auto-saved to `~/.config/pato/saved_accounts.json`. Press `r` on any saved account in the Accounts pane to remove it.
- **Aligned Status Grid**: 4-column aligned repository indicators (`🔒` private, `📦` archived, `🔱` fork, `📁` repo) so repo names start on the exact same column.
- **Multi-criteria Sorting (`s`)**: Cycle between Latest Updated, Latest Created, A-Z, Public/Private, Archived/Active, Source/Fork, and Language.
- **Terminal Image Previews**: High-resolution 24-bit half-block (`▀`) image rendering (`png`, `jpg`, `webp`, `gif`, `bmp`).
- **Oversized File Safeguards**: Prevents UI freeze on files > 100KB with clean download options.
- **1-Key Actions**:
  - `c`: Git clone selected repo into the directory where you launched `pato`.
  - `C` (`Shift+c`): Copy `.git` clone URL to clipboard.
  - `d`: Download selected file into your launch directory.
  - `D` (`Shift+d`): Copy web URL of current repo, file, or profile to clipboard.
  - `o`: Open in default browser (`xdg-open`).
  - `/`: Fuzzy / substring filter.
  - `?`: Keybindings guidance overlay.

---

## Keybindings Cheatsheet

| Key | Action |
| --- | --- |
| `hjkl` / `Arrows` | Move selection & step into (`l`/`Right`/`Enter`) or back (`h`/`Left`/`Backspace`) |
| `J` / `K` | Scroll preview pane down / up |
| `b` | **Switch branch** for selected repo (choose from available remote branches) |
| `s` | **Cycle sort mode** (Updated / Created / A-Z / Status / Archived / Fork / Lang) |
| `c` | **Git clone** selected repository into current launch directory |
| `C` (`Shift+c`) | **Copy clone URL** (`.git`) to system clipboard |
| `d` | **Download** selected remote file into current launch directory |
| `D` (`Shift+d`) | **Copy web URL** of selected item (repo, file, or account) to clipboard |
| `r` | **Remove** saved custom user/org from list (Accounts pane) |
| `u` | Search / explore and auto-remember any public GitHub user or organization |
| `o` | Open selected item in web browser (`xdg-open`) |
| `/` | Live substring / fuzzy filter (Enter to lock, Esc to clear) |
| `?` | Toggle keybindings guidance modal |
| `q` / `Esc` | Quit / close modal / clear filter |

---

## Installation & Running

The binary is installed at `~/.local/bin/pato` and `~/.cargo/bin/pato`.

```bash
# Run from any terminal directory:
pato
```

### Build from source

```bash
cd repos/pato
cargo build --release
```
