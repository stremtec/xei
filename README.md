# xei  晴

> A modern Vim-like terminal editor in Rust.

![](xei.png)

```bash
npm install -g xei-editor       # npm
brew install stremtec/xei/xei   # Homebrew
cargo install xei               # Cargo
```

## Usage

```bash
xei                  # blank buffer
xei src/main.rs      # open a file
xei script.py        # language-aware highlighting + completions
```

## Keybindings

| Key | Mode | Action |
|---|---|---|
| `h` `j` `k` `l` | Normal | Move cursor |
| `w` `b` | Normal | Next / previous word |
| `i` `a` `A` `o` `O` | Normal | Enter insert mode |
| `v` `V` | Normal | Visual / Visual Line |
| `x` `dd` `dw` | Normal | Delete char / line / word |
| `p` `u` | Normal | Paste / Undo |
| `:` | Normal | Open XLC command panel |
| `/` `n` `N` | Normal | Search |
| `Esc` | Insert / Visual | Return to Normal |
| `Ctrl+A` | Insert | Autocomplete |
| `Ctrl+F` | — | Toggle file explorer |
| `F12` , `Ctrl+T`| — | Toggle built-in terminal |

## XLC Commands (`:`)

| Command | Action |
|---|---|
| `:w` , `:save` | Save file |
| `:w <path>` | Save as new path |
| `:e <file>` , `:open <file>` | Open file |
| `:q` , `:quit` | Quit (warns if unsaved) |
| `:q!` , `:quit!` | Force quit |
| `:wq` , `:x` | Save and quit |
| `:mv <dest>` , `:move <dest>` | Move/rename file |
| `:rename <name>` | Rename in same directory |
| `:rm` | Delete current file |
| `:pwd` | Show working directory |
| `:ls` | List files |
| `/pattern` , `:find <pat>` | Search in buffer |
| `:theme` | List themes |
| `:theme <name>` | Switch theme |
| `:help` , `:h` , `:?` | Show all commands |

## Themes

`ocean` (default), `monokai`, `nord`, `solarized`, `gruvbox`, `everforest`, `sakura`.


```bash
xei :theme sakura    # switches immediately, persists to ~/.xei.toml
```

## Panels

The panel size can be adjusted using the mouse.

## License

MIT
