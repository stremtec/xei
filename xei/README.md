# xei  晴

> A modern Vim-like terminal editor in Rust with LSP, tree-sitter, and IDE features.

![](xei.png)

```bash
npm install -g xei-editor       # npm
brew install stremtec/xei/xei   # Homebrew
cargo install xei-editor        # Cargo
```

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/stremtec/xei/master/install.sh | bash
```

**Windows (PowerShell):**
```powershell
iwr https://raw.githubusercontent.com/stremtec/xei/master/install.ps1 | iex
```

## Features

- **LSP integration** — auto-starts language servers for 16 languages (rust-analyzer, clangd, pyright, gopls, tsserver...). Inline diagnostics, go-to-definition (`gd`), and completion
- **Tree-sitter highlighting** — AST-based syntax for Rust, Python, JS/TS, C/C++
- **Multi-buffer tabs** — `gt`/`gT` to switch, `:e` opens in new tab, `:bd` closes
- **Vim modal editing** — Normal, Insert, Visual, Visual Line modes
- **Incremental search** — live results as you type (`/`)
- **File change detection** — auto-reload on external modification
- **System clipboard** — `Cmd+C` / `Cmd+V` via pbcopy/pbpaste
- **Mouse support** — click to move, drag to select, scroll, panel resize
- **Auto-pairing** — brackets/quotes close automatically, smart indent on Enter
- **Smart indent** — Enter copies indentation, adds extra for `{`, `[`, `(`, `:`, `=>`, `->`
- **Panel system** — file explorer (`Ctrl+F`), built-in PTY terminal (`Ctrl+T`), XLC command bar (`:`)
- **10 themes** — ocean, monokai, nord, solarized, gruvbox, everforest, sakura, newspaper, mono, mono_dark
- **CJK support** — Korean, Japanese, Chinese characters render at full width

## LSP

Auto-detected on file open. Status bar shows `LSP: clangd (3)`.

| Command | Action |
|---|---|
| `:LspStart <cmd>` | Manually start a language server |
| `gd` | Go to definition |
| `Ctrl+A` | Completions (keywords + LSP items) |

## Keybindings

### Normal Mode

| Key | Action |
|---|---|
| `h` `j` `k` `l` / `←↓↑→` | Move cursor |
| `w` `b` | Next / previous word |
| `0` `$` | Start / end of line |
| `gg` `G` | Top / bottom of file |
| `i` `a` `A` `o` `O` | Enter Insert mode |
| `x` | Delete character |
| `dd` `dw` | Delete line / word (yanked) |
| `p` | Paste |
| `u` | Undo |
| `v` `V` | Visual / Visual Line |
| `/` | Incremental search |
| `n` `N` | Next / previous match |
| `gt` `tt` `gT` | Next / previous tab |
| `gd` | Go to definition (LSP) |
| `:` | XLC command panel |

### Visual Mode

| Key | Action |
|---|---|
| `d` | Delete selection |
| `y` | Yank (copy) |
| `Cmd+C` | Copy to system clipboard |

### Insert Mode

| Key | Action |
|---|---|
| `Ctrl+A` | Trigger autocomplete (LSP + keywords) |
| `Cmd+V` | Paste from system clipboard |

### Panels

| Key | Action |
|---|---|
| `Ctrl+F` | Toggle file explorer |
| `Ctrl+T` / `F12` | Toggle built-in terminal |
| `Ctrl+E` | Toggle XLC panel |

## XLC Commands (`:`)

| Command | Action |
|---|---|
| `:w` `:save` | Save file |
| `:e <file>` | Open file (new tab if already open) |
| `:q` / `:q!` | Quit / Force quit |
| `:wq` `:x` | Save and quit |
| `:bd` | Close current tab |
| `:mv <dest>` | Move / rename file |
| `:rm` | Delete file |
| `:pwd` `:ls` | Directory info |
| `/pattern` | Incremental search |
| `:theme <name>` | Switch theme |
| `:LspStart <cmd>` | Start language server |
| `:help` | List all commands |

## Themes

`ocean` · `monokai` · `nord` · `solarized` · `gruvbox` · `everforest` · `sakura` · `newspaper` · `mono` · `mono_dark`

```bash
:theme sakura    # persists to ~/.xei.toml
```

## Configuration

`~/.xei.toml` (auto-saved on theme change):

```toml
theme = "gruvbox"
```

## License

MIT
