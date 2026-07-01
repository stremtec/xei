# xei  晴

> A modern Vim-like terminal editor in Rust.

```bash
npm install -g xei-editor
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
| `F12` | — | Toggle built-in terminal |

## XLC Commands (`:`)

| Command | Action |
|---|---|
| `:w` / `:wq` / `:q` | Save / Save+quit / Quit |
| `:e <path>` | Open file |
| `:theme <name>` | Switch theme |
| `:help` | List all commands |

## Themes

`ocean` (default), `monokai`, `nord`, `solarized`, `gruvbox`, `everforest`, `sakura`.

```bash
xei :theme sakura    # switches immediately, persists to ~/.xei.toml
```

## License

MIT
