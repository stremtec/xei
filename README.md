# xei  晴

A modern, fast terminal text editor written in Rust. Vim-modal editing with IDE features — syntax highlighting, autocomplete, file explorer, built-in terminal, and a theming engine.

> **晴** (pronounced _sei_, Japanese for "clear sky") — a clean, bright editing experience.

## Features

- **Modal editing** — Normal, Insert, Visual, and Visual Line modes
- **Syntax highlighting** — 14 languages out of the box (Rust, Python, Go, JS/TS, C/C++, Shell, HTML, CSS, SQL, YAML, TOML, and more)
- **Autocomplete** — file-type-aware suggestions with `Ctrl+A`, navigate with `↑`/`↓`, apply with `Tab`
- **Auto-pairing** — brackets, quotes, and backticks close automatically; paired backspace deletes both
- **Smart indent** — Enter copies the current indentation, adding extra when opening blocks (`{`, `[`, `(`, `:`, `=>`, `->`)
- **XLC command panel** — `:` opens a terminal-style command bar for saving, opening files, searching, changing themes, and file system operations
- **File explorer** — `Ctrl+F` toggles a sidebar; `j`/`k` to navigate, `Enter` to open, `h` to go up
- **Built-in terminal** — `F12` opens a real zsh session via `/usr/bin/script` with PTY support; `Esc` to close
- **7 themes** — Ocean, Monokai, Nord, Solarized Dark, Gruvbox, Everforest, Sakura. Switch with `:theme <name>`
- **Mouse support** — click to move cursor, drag to select text, scroll to navigate, drag panel borders to resize
- **Search** — `/pattern` + `n`/`N` for next/previous match
- **Undo / Paste** — snapshot-based undo (`u`) and yank register (`dd`, `dw`, `p`)
- **Config auto-save** — theme persists via `~/.xei.toml`
- **19 tests, zero warnings**

## Installation

### npm

```bash
npm install -g xei-editor
```

### Cargo

```bash
cargo install xei
```

### From source

```bash
git clone https://github.com/stremtec/xei.git
cd xei
cargo build --release
cp target/release/xei ~/.local/bin/
```

Make sure `~/.local/bin` is in your `$PATH`.

## Usage

```bash
xei                  # open with a blank buffer
xei src/main.rs      # open a file
xei script.py        # language-aware completions + highlighting
```

## Keybindings

### Normal Mode

| Key | Action |
|---|---|
| `h` `j` `k` `l` | Move cursor |
| `w` `b` | Next / previous word |
| `0` `$` | Start / end of line |
| `gg` `G` | Top / bottom of file |
| `i` | Enter Insert mode |
| `a` | Append (insert after cursor) |
| `A` | Append at end of line |
| `o` | Open new line below |
| `O` | Open new line above |
| `x` | Delete character |
| `dd` | Delete line (yanked) |
| `dw` | Delete word (yanked) |
| `p` | Paste after cursor |
| `u` | Undo |
| `v` | Enter Visual mode |
| `V` | Enter Visual Line mode |
| `:` | Open XLC command panel |
| `/` | Search forward |
| `n` `N` | Next / previous search match |

### Visual Mode

| Key | Action |
|---|---|
| `d` | Delete selection |
| `y` | Yank selection |
| `Esc` | Return to Normal |

### Insert Mode

| Key | Action |
|---|---|
| `Esc` | Return to Normal |
| `←` `→` `↑` `↓` | Move cursor |
| `Ctrl+A` | Trigger autocomplete |
| `Tab` | Apply completion / indent |

### Panels

| Key | Action |
|---|---|
| `Ctrl+F` | Toggle file explorer |
| `F12` | Toggle terminal panel |
| `Esc` | Close terminal, return to Normal |
| `Ctrl+E` | Toggle XLC panel |

### Mouse

| Action | Behavior |
|---|---|
| Click | Move cursor |
| Drag | Select text (enters Visual mode) |
| Scroll | Navigate document |
| Drag panel border | Resize explorer / terminal |

## XLC Commands

Open with `:` or `Ctrl+E`.

| Command | Action |
|---|---|
| `:w` | Save file |
| `:w <path>` | Save as |
| `:e <file>` | Open file |
| `:q` | Quit (warns if unsaved) |
| `:q!` | Force quit |
| `:wq` / `:x` | Save and quit |
| `:mv <dest>` | Move / rename current file |
| `:rename <name>` | Rename in same directory |
| `:rm` | Delete current file |
| `:pwd` / `:ls` | Show directory info |
| `/pattern` | Search in buffer |
| `:theme` | List themes |
| `:theme <name>` | Switch theme |
| `:help` | Show all commands |

## Themes

| Theme | Vibe |
|---|---|
| `ocean` | Deep blue-gray, cyan keywords (default) |
| `monokai` | Dark brown-gray, hot-pink keywords |
| `nord` | Arctic cool blue-gray |
| `solarized` | Blue-green scientific |
| `gruvbox` | Warm retro dark |
| `everforest` | Forest green-gray |
| `sakura` | Cherry blossom pink |

Switch with `:theme sakura`. Persists across sessions via `~/.xei.toml`.

## Configuration

xei reads `~/.xei.toml` on startup:

```toml
theme = "gruvbox"
```

## License

MIT — see [LICENSE](LICENSE)
