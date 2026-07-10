# xei Vision & Roadmap

> **Terminal that feels like VS Code. Muscle memory that feels like Vim.**  
> Super-modern TUI: mouse-first where it helps, modal editing where power lives.

## Product pillars

| Pillar | Meaning |
|--------|---------|
| **Vim core language** | Operators × motions × text objects, `.` repeat, registers over time |
| **VS Code comfort** | Tabs, panels, mouse, palette-ish commands, diagnostics, LSP |
| **Modern TUI** | Flat chrome, live search, chord hints, smooth mouse hit-targets |
| **Shared engine** | Logic in `xei-core`; TUI (`xei`) and desktop (`suisei`) are shells |

## Non-goals (for now)

- Full Vimscript/Lua plugin runtime  
- 1:1 Neovim LSP feature parity on day one  
- Pure terminal purism (we *want* mouse + chrome)

---

## Phase plan

### Phase 1 — Editing language + everyday power  ← **landed (core)**

Foundation every Vim user expects; unlocks “it sticks in the fingers.”

- [x] Plan / architecture doc (`docs/ROADMAP.md`)  
- [x] **Operator-pending**: `d`/`c`/`y` + motion (`w`, `b`, `e`, `0`, `$`, `j`/`k`, `G`, `gg`…)  
- [x] **Text objects**: `iw` `aw` `i"`/`a"` `i'` `i(`/`ab` `i[` `i{`/`aB` …  
- [x] **Dot-repeat** `.` for last operator / textobject / `x` / `r`  
- [x] **Redo** `Ctrl+R` (undo stack past/future)  
- [x] **Reverse search** `?`, word reverse `#`  
- [x] **Go to line** `:42`  
- [x] **Mouse**: click tabs to switch; explorer focus; scroll  
- [x] Tests for ops / textobjects / search (`cargo test -p xei-core` = 41)  

### Phase 2 — Registers, marks, jumps  ← **landed**

- [x] Named yank/put (`"a`–`"z`, append `"A`), system `"+` / `"*`  
- [x] Marks `m{a-z}`, jump `'a` (line) / `` `a `` (exact)  
- [x] Jumplist `Ctrl+O` / `Ctrl+I` (Tab ≈ forward)  
- [x] `;` `,` for f/t/F/T repeat  
- [x] Modules: `registers.rs`, `nav.rs` + tests  

### Architecture notes (Phase 2)

```
xei-core/
  registers.rs    named / unnamed / clipboard registers
  nav.rs          Marks, JumpList, LastFind
  app.rs          store_yank, jump_*, set_mark, repeat_find
```

### Phase 3 — VS Code surface  ← **landed (core)**

- [x] Command palette (`Ctrl+Shift+P` / `Cmd+Shift+P`)  
- [x] Fuzzy file open (`Ctrl+P` / `Cmd+P`)  
- [x] Problems palette (`:problems`, command palette)  
- [x] `]d` / `[d` diagnostic jump  
- [x] LSP hover (`K`)  
- [x] Double-click select word  
- [x] LSP references (`gr`) / rename (`:Rename name`)  
- [x] System clipboard overhaul (Cmd+C/V, unnamedplus, OSC 52)  

### Architecture notes (Phase 3)

```
xei-core/
  clipboard.rs    multi-backend + OSC 52
  palette.rs      files / commands / problems fuzzy UI state
  lsp.rs          hover request + parse
```

### Phase 4 — Structure & scale  ← **landed (core)**

- [x] Horizontal/vertical splits (`Ctrl+W v/s`, cycle `w`, close `q`)  
- [x] Workspace find/replace (`Ctrl+Shift+F`, rg-backed)  
- [x] Document symbols / outline (`gO` / `Ctrl+Shift+O`) + workspace symbols  
- [x] Peek definition (`gp`)  
- [x] LSP inlay hints (when server supports)  
- [x] `:s` / `:%s` substitute  
- [x] Macros `q{a-z}` / `@{a-z}` / `@@`  
- [x] Visual block (`Ctrl+V`) yank/delete  
- [x] Persistent config: theme, tab_width, clipboard_sync, relative_number  

```
xei-core/
  substitute.rs   :s parser + apply
  macros.rs       record/play bank
  config.rs       expanded ~/.xei.toml
```

### Phase 4.5 — Highlight quality (VS Code-class)  ← **landed**

Quality-first stack for most languages:

- [x] Expanded `TokenKind` + theme palette (function, macro, parameter, property, …)  
- [x] Tree-sitter **highlight queries** (`highlights.scm`) for Rust, Python, JS/TS, C, Go, Bash, JSON  
- [x] Line-tokenizer fallback for many other languages  
- [x] LSP **semantic tokens** (`textDocument/semanticTokens/full`) request + UTF-16 decode  
- [x] UI merge: **semantic > query > fallback** (tightest span)

```
xei-core/
  highlight.rs   TokenKind, style_for, from_capture, from_semantic_type
  syntax.rs      Query-based tree-sitter highlights
  lsp.rs         semantic tokens legend + decode overlay
xei/ui.rs        per-character merge stack
```

### Phase 5 — Delight

- [x] Git gutter signs (`git diff HEAD -U0`)  
- [x] **Source Control panel** (`Ctrl+G`) — light SCM: message, Commit, Staged/Changes, Graph  
- [x] **Git workbench** (`Ctrl+Shift+G`) — JetBrains-style docked 3-pane (Changes | Log+graph | Files), same z-layer as editor  
- [x] **LSP settings** — `lsp_enabled` + per-language `lsp.rust` / `lsp.python` … in Settings & `~/.xei.toml`  
  - Tabs: Status · Branches · History · Commit · Diff · **PRs** (open/closed/merged + filter) · **Issues** · Auth  
  - History: list or lane graph; Enter → commit page; file Enter → commit diff  
  - **gh auth** built-in: status / login --web / logout / setup-git (`:gh-login`, Auth tab)  
  - PRs: list · checkout · create from HEAD (`P`) when authenticated  
  - Esc stack: Diff → Commit → History → close → light SCM → editor  
- [x] **Pretty document preview** (`Ctrl+Shift+V`) — GFM-oriented Markdown / JSON (headings h1–h6 + setext, tables+align, task lists, nested lists, alerts, footnotes, autolinks, images, fences ```/~~~, entities, `<kbd>`, front matter)   
- [x] Preview **transform sweep** — in-pane mode switch (no overlay); the live source view (gutter + syntax colors intact) is consumed top-down by the pretty view behind a ░▒▓ wavefront; frame 0 is pixel-identical to NORMAL mode and content stays gutter-aligned (no leftward jump)  
- [x] Animation clocks start at the **first rendered frame** (sync open work — git, file walks, doc render — no longer eats the window)  
- [x] Panel entrance animations — palette expands in, SCM slides from the right  
- [x] Welcome screen — shade-art logo + key hints on empty start  
- [x] Explorer per-filetype colors, active-tab accent marker, ✓/✗/↻ status icons  
- [x] **Colored commit graph** (lane-based topology, VS Code–style branch dots)  
- [x] **Settings** (`Ctrl+,`) — About · Setting (theme/editor/git/`gpu_acc`) · Help (shortcut list); `~/.xei.toml`  
- [x] **GPU terminal Phase A** — `TerminalCaps` detect, synchronized output, colored diag underlines, status `GPU` badge (gated by `gpu_acc`)  
- [x] **GPU terminal Phase B (start)** — Kitty graphics helpers + peek soft-shadow card; OSC 8 foundations  
- [x] **LSP Phase Z** — UTF-16 positions, didClose/live buffer, soft/hard errors + install hints, multi-file WorkspaceEdit, code actions (`Ctrl+.`), formatting (`Ctrl+Shift+I`)  
- [x] **GPU Phase B2** — peek soft-shadow + Kitty shadow bar; OSC 8 file hyperlink re-emit on peek
- [x] Session restore (`~/.xei/session`)  
- [x] Git open path performance — lazy History/graph/`gh auth` (Status-only on open)  
- [x] **xeifetch screensaver** — cryptex clock + weather + easter egg (`/` · `fakers` → god)  
- [x] **Desktop pet** — Kitty GIF overlay, Settings Pet tab, speed slider  
- [x] **Media preview** — images (Kitty + resize), CSV/NPY tables, audio play/stop  
- [x] **GitHub Auth overhaul** — non-blocking login, device code + clipboard + auto browser  

### Phase 6 — Structure & daily power  ← **landed**

Breadcrumbs, folds, blame, snippets — the “use it all day” layer.

- [x] **Breadcrumbs** — path segments under the tab bar (VS Code-style)  
- [x] **Code folding** — indent-based ranges; `za` toggle · `zc` close · `zo` open · `zM`/`zR` all  
- [x] **Git blame panel** — `Ctrl+B` / `gb`; file slides right · flame-colored author/hash strip  
- [x] **Snippets (v1)** — insert-mode Tab expands common triggers (`fn`, `for`, `if`, …)  
- [x] **Live-refresh** — external file edits auto-reload buffer (cursor preserved)  
- [x] **Stash manager** — Git workbench tab `9 Stash` · apply / drop / preview  
- [x] **Multi-cursor (v1)** — `Ctrl+D` next match · `Ctrl+Alt+j/k` column · Esc clear  
- [x] **Which-key full chord map** — Space leader + nested menus (`f/g/l/w/…`); `g`/`z`/`d`/`Ctrl+W`/… delayed popup; `]c`/`[c` git hunks  
- [x] **DAP debugger (v2)** — protocol fix · attach · Node TCP · REPL · launch.json · conditional BPs · mouse · see `docs/DAP-PLAN.md`  

```
xei-core/
  fold.rs      indent fold ranges + open/closed set
  snippets.rs  trigger → body expansion
  git.rs       + blame porcelain parse
  which_key.rs Space leader + prefix maps + delay
  dap.rs       DAP client (stdio JSON-RPC)
xei/ui.rs      breadcrumb row · fold markers · blame gutter · which-key · debug panel
```

### Phase 7 — Scale & IDE depth  ← **landed** (Suisei deferred)

- [x] **Call hierarchy** — `gC` / `gI` / `gH` / `SPC l c` / `:calls`; incoming/outgoing (LSP); Tab flip · Enter jump  
- [x] **Interactive rebase** — `:rebase [N]` / `SPC g r`; pick/reword/edit/squash/fixup/drop · reorder · run  
- [x] **Code lens** — LSP `textDocument/codeLens` EOL virtual text + `codeLens/resolve` (rust-analyzer-style lazy lenses); `:codelens` / `SPC t l`  
- [x] **PR review surface** — Enter on PR · `:pr N` · files + diff + review comments · checkout/browser; `gh` fetches on background threads, per-file diffs debounced, lists scroll with selection  
- [x] **Plugin hooks (limited)** — `~/.xei/hooks.toml` · `on_save` / `on_open` / `on_quit` · `{file}` placeholders (shell-quoted); runs on background threads, 10s timeout, quit hooks detached  
- [ ] Suisei desktop parity *(deferred)*  

```
xei-core/
  call_hierarchy.rs   panel state
  rebase.rs           interactive rebase planner + GIT_SEQUENCE_EDITOR
  pr_review.rs        PR files / comments / diff via gh
  hooks.rs            shell hooks from ~/.xei/hooks.toml
  lsp.rs              prepareCallHierarchy + codeLens
```

---

## Architecture notes (Phase 1)

```
xei-core/
  ops.rs          Operator, Motion, TextObject, ranges, apply
  app.rs          wires undo/redo, last_change, search dir
  buffer.rs       primitives
xei/
  event.rs        maps keys → ops (thin)
  ui.rs           mouse hit regions (tabs), chrome
```

**Rule:** prefer recording a `LastChange` in core so `.` works without re-parsing keys in the TUI.

---

## Success criteria (Phase 1)

1. `diw` / `ci"` / `yaw` / `d$` feel like Vim.  
2. `.` repeats the last delete/change/yank-op.  
3. `u` / `Ctrl+R` undo-redo pair works.  
4. `?` and `#` work; mouse can switch tabs.  
5. `cargo test -p xei-core` green; TUI builds.
