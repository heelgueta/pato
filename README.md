# 🦆 pato

A fast, lightweight GitHub repository and tree explorer TUI with Miller-columns, inspired by `yazi`.

Browse your personal accounts, organizations, repositories, directory trees, branches, and code files with syntax highlighting as if they were local directories.

---

## Installation

### From Source

Ensure you have Rust (`cargo`) installed:

```bash
git clone https://github.com/heelgueta/pato.git
cd pato
cargo build --release
cp target/release/pato ~/.local/bin/  # or ~/.cargo/bin/
```

### Authentication

`pato` zero-config authenticates via your existing GitHub CLI setup:
```bash
gh auth login
```
Or by setting `export GITHUB_TOKEN="ghp_..."`. If unauthenticated, `pato` still works in public-only mode (60 requests/hour).

---

## Keybindings

| Key | Action |
| --- | --- |
| `h` `j` `k` `l` / Arrows | Navigate list, step into (`l` / `Enter`) or back (`h` / `Backspace`) |
| `J` / `K` | Scroll preview pane down / up |
| `b` | **Switch branch** (select from available remote branches) |
| `s` | **Cycle sort mode** (Updated / Created / A-Z / Status / Archived / Fork / Lang) |
| `c` | **Git clone** repository into launch folder |
| `C` (`Shift+c`) | **Copy clone URL** (`.git`) to clipboard |
| `d` | **Download** selected remote file into launch folder |
| `D` (`Shift+d`) | **Copy web URL** of selected item to clipboard |
| `r` | **Remove** saved user/org from list |
| `R` (`Shift+r`) | **Refresh** current level and bypass cache |
| `u` | Search / explore and auto-remember any public user or organization |
| `o` | Open selected item in web browser (`xdg-open`) |
| `/` | Live filter in active list |
| `?` | Toggle keybindings cheat sheet |
| `q` / `Esc` | Quit / clear filter / close dialog |

---

## Features

- **Ink Monochrome Theme**: Crisp black-and-white terminal palette (`Border::Double`, clean dark panels).
- **Yazi-style Cursor Memory**: Returning to a parent directory or repo list preserves your exact previous cursor position.
- **Auto-Pagination**: Automatically follows GitHub Link headers up to 1,000 repositories so full lists are sorted and filtered accurately.
- **Terminal Image Previews**: High-resolution 24-bit half-block (`▀`) rendering for `png`, `jpg`, `webp`, `gif`, and `bmp`.
- **Live Rate-Limit Badge**: Displays live `API: <remaining>/<limit>` in header, warning when running low.
- **Auto-Remembered Accounts**: Discovered users/orgs persist in `~/.config/pato/saved_accounts.json`.
- **In-Pane Human Errors**: Friendly error messages inside panes with instant retry (`R`).
