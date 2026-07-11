# Polish Backlog — bug audit (2026-07-10)

> **v3.0.3 optimization build (07-11):** delta-based undo (804MB→4.5MB per
> 300-edit session) + IN_RAM_MAX=50 SSD spill + `undo_caching` persistence;
> idle CPU 54.8%→8.0% (buffer version gate kills the per-frame O(file) join,
> dirty rendering w/ 100ms heartbeat + 700ms full-rate input window, terminal
> span run-grouping, per-row hoists, LSP sync version gate, thin-LTO profile).

> **Fix round 1 landed (same day):** A1 A2 A4 A5 A6 B1 B2 B3 C1 D1 done;
> A3 done as N-pane single-direction splits (max 4).
> **Round 1.5 (07-11, from live-terminal report):** terminal scrollback view
> was upside-down → bottom-anchored rewrite; CJK wide-char spacer double-render
> ("고정폭") → renderer skips spacer cells; wheel now forwards to inner apps
> (SGR when app enables mouse, DECCKM-aware arrows in alt-screen); arrow keys
> inside terminal honor DECCKM (was breaking less/vim arrows entirely).

> **Round 2 progress (07-11):** R1 ✅ (surface constrained to body — tab bar +
> status stay visible; fetch error shown inline in-panel + status sync),
> R2 ✅ (tab-chip + row hit regions, click guard), R3 ✅ (`↑N · type/wheel↓ →
> live` badge), **preview theme palette** ✅ (`to_ratatui_style` now derives
> all ~30 colors from the active theme; headings grade accent→fg).
> **Release decision: first official version = 1.0.0** (semver; supersedes
> the 2.x internal numbering — bump all four version sites accordingly).

> **Round 3 (07-11):** R5 ✅ (diff filter exact-token match + test), R6 ✅
> (CSI-6n cursor query skipped unless Kitty graphics active), R7 ✅ (per-pane
> cursors — Pane stores (row,col), saved/restored on focus switch + test),
> R8 ✅ partial (`C-w h/j/k/l` directional focus for any pane count; ≥3-pane
> drag-resize and mixed directions remain out), R10 ✅ (stray `test.rs`
> deleted), R11 ✅ (README features/keybinds/XLC tables + Settings Help synced:
> DAP v2, multi-split, wrap pan, `:pr`, hooks, code lens, terminal scrollback).
> **R4 resolved as wontfix**: scrollback non-reflow on resize matches
> xterm/tmux behavior; true reflow needs wrap-continuation tracking — post-1.0
> if ever. **R9 deferred** (focus-stack helper — architectural, post-1.0).

## Round 2 — remaining polish (priority order)

**Medium**
- **R5 (=B6)** `filter_diff_for_path` substring false-positives (`a.rs` ⊂ `xa.rs`).
- **R6 per-frame CSI 6n**: `get_cursor_position()` every frame = blocking
  round-trip per frame on slow terminals; skip when no Kitty graphics active.
- **R7 split pane cursor**: panes share the buffer cursor (Pane stores
  tab+scroll only) — decide per-pane cursors (Vim parity) or document.
- **R8 multi-split gaps**: ≥3 panes are equal-size only (no drag-resize),
  no mixed directions, no `C-w h/j/k/l` directional focus.
- **R9 (=C3)** focus/Esc-stack helper — each surface hand-rolls its Esc
  return path; one helper kills a bug class.

**Cleanup**
- **R10** stray `test.rs` at repo root — delete before packaging.
- **R11 docs sync**: README lags shipped features — DAP v2 (REPL, attach,
  launch.json, conditional BPs, F6 pause), multi-split (×4), wrap_lines +
  zh/zl/zH/zL, `:pr` review surface, hooks.toml, code lens, terminal
  scrollback/mouse forwarding. Settings→Help shortcut list too.
- **R12** suisei parity (Phase 7 leftover) — big; out of release scope.

## Packaging — v2.6.0 release checklist

Current: **everything uncommitted** (~21k lines / 55 files over HEAD v2.5.1).

1. **Commit the working set** — split into logical commits (core engine
   modules / DAP / git surfaces / UI polish / docs) or one feature mega-commit.
2. **Version bumps**: `xei-core/Cargo.toml`, `xei/Cargo.toml` (+lock),
   `xei/package.json`, `xei/install.js` (`VERSION = "v2.5.1"` hardcoded!).
3. **Release binaries**: `install.js`/`install.sh` download GitHub Release
   assets per target triple — need darwin x64/arm64, linux, windows builds
   uploaded to the `v2.6.0` release. No `.github/workflows` exists → manual,
   or add a release CI workflow (recommended).
4. **Publish order**: cargo publish `xei-core` → `xei-editor` (path dep needs
   a version), then `npm publish`, then bump the Homebrew tap formula.
5. **Pre-release smoke**: real-terminal manual pass (split×preview, terminal
   scroll/CJK, wrap toggle, PR surface, DAP session), `cargo test`, suisei
   build, fresh-install test of npm/brew paths.

> ✔ = root cause confirmed in code, not just observed behavior.

## A. Reported issues — root causes

### A1 ✔ Preview swallows the whole screen in split mode
- **Where:** `xei/src/ui.rs:144,163` — `preview_active` branches *before* the
  split layout and calls `draw_preview_pane(f, app, main_rect)` on the entire
  main rect.
- **Fix:** move the preview branch *inside* `draw_editor_split_or_single` so it
  replaces only the focused pane's rect (same in-pane transform, smaller area).
  Mouse-scroll routing (B3) should follow.

### A2 ✔ `Ctrl+W q` closes the wrong pane (inverted Vim semantics)
- **Where:** `xei-core/src/app.rs:3018` `close_split()` — keeps the **focused**
  pane's tab as the survivor. Vim's `C-w q` closes the focused window and moves
  you into the *other* one.
- **Fix:** keep `panes[1 - focus]`; also mirror the pane-terminal cleanup at
  `app.rs:3026` (currently keeps the terminal only when it lives in the focused
  pane — must flip too). Minor: `C-w q` when not split is a no-op; Vim quits
  the window (could map to `:q` behavior).

### A3 ✔ Split works only once (no nesting / >2 panes)
- **Where:** `xei-core/src/split.rs` — `SplitState { kind: SplitKind, panes: [Pane; 2], focus: 0|1 }`
  is architecturally capped at two panes with one direction.
- **Fix (medium refactor):** replace with a split tree (or `Vec<Pane>` + layout
  vec): `Node::Leaf(Pane) | Node::Split { kind, ratio, children }`. Touches:
  ui.rs `draw_editor_split_or_single`, `pane_hit_regions`, drag-resize
  (`split_sep_hit`), Ctrl+W chord (w cycle → directional `h/j/k/l` moves),
  pane-bound terminal, `close_split`, session restore.

### A4 ✔ No mouse scroll in the internal terminal (Ctrl+Shift+T / side panel)
- **Where:** `xei/src/event.rs:43-76` — ScrollUp/Down handle Explorer, Preview,
  GitWorkbench by *mode*, then unconditionally scroll the editor buffer. No
  terminal branch at all; pointer position is ignored.
- **Existing plumbing:** `term.rs` already has `scrollback` + `scroll_offset` +
  `scroll_up/down` (wired only to PageUp/PageDown at `event.rs:4157`).
- **Fix:** route by pointer rect: if over terminal rect (side panel
  `terminal_separator_x`, pane-bound window, or full window) →
  `app.terminal.scroll_up/down(3)`.

### A5 ✔ Enter on a PR (workbench) wrecks the UI
- **Where:** `xei/src/ui.rs:3325` `draw_pr_review` — paints its background with
  a style-only `Block` (`f.render_widget(Block::default().style(bg), area)`).
  ratatui's `Block` style pass **recolors cells but does not clear symbols**,
  so the dense git-workbench text underneath stays visible through the PR
  surface → "broken" look.
- **Fix:** `f.render_widget(Clear, area)` first (the pattern every popup in
  this file already uses — 12 call sites). Also decide surface extent: it
  currently covers tab bar + status line too (B5).

### A6 ✔ Theme color mismatches (hardcoded palettes)
Hardcoded `Color::Rgb` counts per draw fn (should read `app.theme.*`):
| fn | hardcoded colors |
|---|---|
| `draw_pr_review` | 20 |
| `draw_debug_panel` | 16 |
| `draw_statusline` | 13 |
| `draw_rebase_panel` | 10 |
| `draw_call_hierarchy` | 8 |
| `draw_editor` (BP ●, debug ▶ line colors) | 6 |
| `file_type_color` | 11 (semi-intentional; still clashes on light themes) |

On light themes (newspaper, solarized, sakura) these stay dark-navy with
low-contrast selection colors. **Fix:** swap to `app.theme` fields; extend
`theme.rs` with a small panel palette (panel_bg, panel_sel_bg/fg, accent,
ok/warn/err) so newer surfaces stop inventing colors.

## B. Additional findings — UI

- **B1 ✔ Double selection highlight in PR/Issue lists** —
  `ui.rs:2475` (`vi == pr_sel || pi == pr_sel`), same at `:2524` for issues:
  when a filter is active two rows can render as selected (visual index vs
  backing index). Use the visual index only.
- **B2 Mouse scroll is mode-gated, leaving gaps** — scrolling over Settings /
  SCM / Debug panel / PR review / Call hierarchy / Rebase scrolls the hidden
  editor buffer instead. Needs pointer-position routing (one dispatcher).
- **B3 Split ignores pointer for scroll** — wheel always scrolls the focused
  pane, even with the cursor over the other pane (`pane_hit_regions` already
  exist for clicks; reuse for wheel).
- **B4 PR review has no mouse hit regions** — clicks pass through to the editor
  underneath (cursor moves / drag-selects invisibly; discovered after Esc).
  Tabs 1/2/3, file rows, comment rows need hit vectors like `dap_tab_hits`.
- **B5 PR review covers tab bar + status line** — drawn over the full frame
  after the status line. If intended as fullscreen, fine; otherwise constrain
  to the body rect so global chrome stays.
- **B6 `filter_diff_for_path` substring match** (`pr_review.rs`) — fallback
  path filter uses `line.contains(path)`; `a.rs` also matches `xa.rs` diffs.

## C. Additional findings — behavior

- **C1 close_split pane-terminal cleanup follows the old (inverted) keep-side**
  — must flip together with A2.
- **C2 Wheel/PageUp keys in terminal**: keyboard scrollback exists but there's
  no visual indicator of "scrolled back N lines" (VS Code shows a badge);
  scroll_offset resets are scattered (`term.rs:305,333,686,707`).
- **C3 Mode-based Esc/focus stacks** are per-surface ad hoc (preview → normal,
  pr_review → workbench, debug → unfocus…). Works, but each new surface
  re-implements it; a small focus-stack helper would kill a bug class.

## D. Requested feature

### D1 Line wrap vs horizontal scroll option
Current: long lines always soft-wrap (`ui.rs:4458,4594` — `text_width`,
`visual_line_width`, wrap segment math).
Plan:
- `~/.xei.toml`: `wrap_lines = true` (default) | `false` (+ alias `wrap`).
  Parse/save in `config.rs`, field on `App`.
- Settings (`Ctrl+,` → Setting): toggle row "Wrap long lines".
- `wrap_lines = false` render path: single row per line, per-pane
  `col_offset: usize`; cursor-follow keeps the caret visible
  (`col_offset = clamp(col_offset, cursor_vis+1-width, cursor_vis)`);
  `zh`/`zl`/`zH`/`zL` + horizontal wheel (ScrollLeft/Right) nudge; `$`-motions
  and mouse click mapping must add `col_offset`; statusline `↔ N` indicator
  when panned.
- Touches: config.rs, settings.rs, app.rs (field + clamp on cursor move),
  ui.rs (both editor render paths + extra-cursors + click mapping),
  event.rs (wheel + z-chords), README.

## Suggested fix order

1. **A5 + A6(pr_review/debug/rebase/call-hierarchy) + B1** — one UI-polish PR:
   Clear + theme fields + selection fix (small diffs, big visible win).
2. **A2 + C1** — close_split semantics flip (tiny, behavioral).
3. **A4 + B2 + B3** — one mouse-routing dispatcher keyed on pointer rects.
4. **A1** — preview into focused pane.
5. **D1** — wrap option feature.
6. **A3** — split tree refactor (biggest; do last, after mouse dispatcher
   exists so hit regions generalize).
