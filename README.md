# xei  晴

> A modern Vim-like terminal editor in Rust with LSP, tree-sitter, and IDE features.

![](https://raw.githubusercontent.com/stremtec/xei/master/xei/xei.png)

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

- **LSP integration** — auto-starts servers when installed (rust-analyzer, pyright, tsserver, clangd, gopls, jdtls, lua-ls, zls, …). Stable init/didOpen/versioned didChange, diagnostics, `gd`/`gr`/`K`, completion, **semantic tokens**
- **Syntax highlighting (quality stack)** — LSP semantic tokens ▸ tree-sitter `highlights.scm` queries (Rust, Python, JS/TS, C, Go, Bash, JSON) ▸ line-tokenizer fallback for many other languages; CJK-safe char columns
- **Git gutter** — `+` / `~` / `▁` signs from `git diff HEAD`
- **Source Control (`Ctrl+G`)** — light SCM: commit message, stage/unstage, Changes, commit graph  
- **Git workbench (`Ctrl+Shift+G`)** — near full-screen mini GitHub: Status (stage/discard) · Branches (create/delete) · History (list/graph, cherry-pick/revert) · Commit · Diff · PRs · Issues · Auth; fetch/pull/rebase-pull/push/stash
- **Pretty document preview (`Ctrl+Shift+V`)** — GitHub-flavored Markdown / JSON (h1–h6, setext, tables+align, task/nested lists, alerts, footnotes, autolinks, images, ```/~~~ fences, entities, `<kbd>`); source transforms into the pretty view behind a ░▒▓ wavefront
- **Breadcrumbs** — VS Code-style path under the tab bar
- **Code folding** — indent folds: `za` toggle · `zc`/`zo` · `zM`/`zR`
- **Git blame** — `Ctrl+B` / `gb` flame-colored side panel (file slides right)
- **Which-key** — `Space` leader map (files/git/lsp/debug/window/…) + delayed chord popups for `g`/`z`/`d`/`Ctrl+W`
- **DAP debugger v2** — F5 start/continue · F6 pause · F9 breakpoints (`:bp if expr` / `:bp log msg`) · F10/F11 step · variables tree · console REPL (eval when stopped) · `:DapAttach` · `.vscode/launch.json` configs · debugpy / dlv / lldb-dap / js-debug (TCP)
- **Call hierarchy** — `gC` / `:calls` incoming · `gH` outgoing (LSP); Tab flip · Enter jump
- **Interactive rebase** — `:rebase 8` / `SPC g r` · pick/squash/fixup/drop · reorder · run
- **Code lens** — LSP lenses at EOL (`:codelens` / `SPC t l`)
- **PR review** — Enter on a PR (or `:pr 12`) · files · diff · review comments
- **Hooks** — `~/.xei/hooks.toml` shell commands on save/open/quit
- **Snippets** — Insert-mode `Tab` expands triggers (`fn`, `for`, `if`, `def`, …)
- **Session restore** — reopens last files + cursors from `~/.xei/session` when started with no args
- **Multi-buffer tabs** — `gt`/`gT` to switch, `:e` opens in new tab, `:bd` closes
- **Vim modal editing** — Normal, Insert, Visual, Visual Line + operators (`d`/`c`/`y`) × motions × text objects (`diw`, `ci"`, `daw`…)
- **Dot-repeat & redo** — `.` repeats last change; `Ctrl+R` redo
- **Incremental search** — live `/` and reverse `?`, `n`/`N`, `*`/`#`
- **File change detection** — auto-reload on external modification
- **System clipboard** — `Cmd+C`/`V`/`X`, `y`/`d`/`p` sync to OS (pbcopy / xclip / wl-copy + OSC 52)
- **Terminal scrollback** — wheel / PageUp scrolls history (`↑N` badge); wheel forwards to mouse-aware TUIs (claude, vim); CJK-correct rendering
- **Command palette** — `Ctrl+P` files, `Ctrl+Shift+P` commands, `:problems`
- **Diagnostics** — `]d`/`[d` jump, problems list; `K` LSP hover
- **Mouse support** — click tabs, drag select, double-click word, scroll, panel resize
- **Auto-pairing** — brackets/quotes close automatically, smart indent on Enter
- **Smart indent** — Enter copies indentation, adds extra for `{`, `[`, `(`, `:`, `=>`, `->`
- **Panel system** — file explorer (`Ctrl+F`), built-in PTY terminal (`Ctrl+T`), XLC command bar (`:`)
- **10 themes** — ocean, monokai, nord, solarized, gruvbox, everforest, sakura, newspaper, mono, mono_dark
- **CJK support** — Korean, Japanese, Chinese characters render at full width

## Terminal compatibility

xei requests the **kitty keyboard protocol** when the terminal supports it
(Ghostty, Kitty, WezTerm, foot, recent Alacritty) so `Ctrl+Shift+…` chords are
fully disambiguated. On legacy terminals (Terminal.app, plain xterm) those
chords collapse into plain `Ctrl+…` — use the built-in fallbacks:

| Chord | Fallback |
|---|---|
| `Ctrl+Shift+V` preview | `SPC f p` · `:preview` |
| `Ctrl+Shift+G` git workbench | `SPC g g` |
| `Ctrl+Shift+P` command palette | `SPC p` |
| `Ctrl+Shift+F` find in files | `SPC s f` |
| `Ctrl+Shift+T` terminal window | `SPC w t` / `SPC t T` |
| `Ctrl+Shift+D` debug panel | `SPC d d` |
| `Ctrl+Shift+I` format | `SPC l f` |
| `Ctrl+Shift+O` symbols | `gO` |

Windows: runs in Windows Terminal / ConPTY (PowerShell is spawned for the
built-in terminal; clipboard falls back to `clip` / `Get-Clipboard` + OSC 52).

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
| `Space` | Leader (which-key): `f` files · `g` git · `l` lsp · `w` window · … |
| `]c` `[c` | Next / previous git change |
| `i` `a` `A` `o` `O` | Enter Insert mode |
| `x` | Delete character |
| `d`/`c`/`y` + motion | Operators (`dw`, `d$`, `dG`, `cc`…) |
| `diw` `ci"` `dib` … | Text objects (inner/around word, quotes, brackets) |
| `"a` `"A` `"+` | Registers (named / append / clipboard) then `y`/`d`/`p` |
| `ma` `'a` `` `a `` | Set mark / jump (line / exact) |
| `Ctrl+O` `Ctrl+I` | Jumplist back / forward |
| `f`/`t` then `;` `,` | Find char; repeat / reverse |
| `.` | Repeat last change |
| `p` `P` | Paste after / before |
| `u` / `Ctrl+R` | Undo / Redo |
| `v` `V` | Visual / Visual Line |
| `/` `?` | Search forward / reverse |
| `n` `N` | Next / previous match |
| `*` `#` | Word under cursor (fwd / back) |
| `gt` `gT` | Next / previous tab |
| `gd` | Go to definition (LSP) |
| `gp` | Peek definition (Enter opens) |
| `gO` / `Ctrl+Shift+O` | Document symbols (outline) |
| `Ctrl+W v` / `s` | Vertical / horizontal split (repeat → up to 4 panes) |
| `Ctrl+W w` / `q` | Cycle pane / close focused pane |
| `Ctrl+W h/j/k/l` | Directional pane focus |
| `zh` `zl` `zH` `zL` | Pan view horizontally (`wrap_lines = false`) |
| `Ctrl+Shift+F` | Find in files (workspace) |
| `Ctrl+.` | Code actions / quick fix (LSP) |
| `Ctrl+Shift+I` | Format document (LSP) |
| `:` | Command panel (`:42` go to line, `:w` …) |
| `Ctrl+S` | Save |
| `Ctrl+G` | Light Source Control (stage/commit) |
| `Ctrl+Shift+G` | Full Git workbench (branch/sync/diff) |
| `Ctrl+,` | Settings (About / Setting / Help) |
| `Ctrl+Shift+V` | Pretty preview (Markdown / JSON) |
| `Ctrl+P` / `Cmd+P` | Quick open files |
| `Ctrl+Shift+P` | Command palette |
| `]d` / `[d` | Next / prev diagnostic |
| `K` | LSP hover |
| `Cmd+C` / `Cmd+V` / `Cmd+X` | Copy / paste / cut (system clipboard) |
| `Ctrl+V` | Visual block |
| `qa` … `q` / `@a` / `@@` | Record / play / replay macro |
| `gr` | LSP references |
| `gb` | Toggle git blame |
| `za` `zc` `zo` `zM` `zR` | Fold toggle / close / open / all |
| `:s/pat/repl/g` / `:%s//g` | Substitute |
| `:Rename name` | LSP rename |

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
| `Ctrl+T` / `F12` | Side terminal panel |
| `Ctrl+Shift+T` | Terminal **window** (auto split). Close: `Ctrl+Shift+W` then `y`. Esc goes to shell. |
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
| `:preview` | Pretty document preview |
| `:pr <N>` | PR review surface (gh) |
| `:bp` / `:bp if <expr>` | Breakpoint / conditional |
| `:DapLaunch <prog> [args]` | Debug a program |
| `:DapAttach <target>` | Attach debugger |
| `:hooks` | Reload ~/.xei/hooks.toml |
| `:LspStart <cmd>` | Start language server |
| `:help` | List all commands |

## Themes

`ocean` · `monokai` · `nord` · `solarized` · `gruvbox` · `everforest` · `sakura` · `newspaper` · `mono` · `mono_dark`

```bash
:theme sakura    # persists to ~/.xei.toml
```

## Configuration

`~/.xei.toml` — created on first theme change; every key optional:

```toml
theme = "ocean"
tab_width = 4
clipboard_sync = true
relative_number = false
wrap_lines = true       # false = horizontal scroll (zh/zl/zH/zL pan, ↔ badge)
gpu_acc = true          # Ghostty/Kitty enhancements (Ctrl+, → Setting)
key_hints = true        # which-key chord popups
lsp_enabled = true      # per-language overrides via [lsp] or Settings
```

Plugin hooks live in `~/.xei/hooks.toml` (`on_save` / `on_open` / `on_quit`).

## License

MIT
