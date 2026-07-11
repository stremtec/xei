# v3.0.6 — "GPU 가속 제대로" integration plan

> **Landed in 3.0.6:** GfxRegistry (diffed placements, ghost-free cleanup,
> caret-safe flush) · inline Markdown preview images (local files, cell-pixel
> sized, scroll-tracked) · per-feature toggles `gpu_graphics` /
> `gpu_hyperlinks` (+ Settings rows) · TIOCGWINSZ cell-size probe (w+h).
> **Deferred:** OSC8 sweep into list rows (ratatui cell model fights raw OSC),
> explorer thumbnails, placement-move optimization (a=p without re-upload),
> in-sync-window flush (G2 full), About caps read-out.

> Goal: graduate GPU-terminal support from scattered enhancements (pet, media
> preview, peek shadow) into a coherent, gated, everywhere-it-helps layer.
>
> Current base (562 lines): `term_caps.rs` (sync/undercurl/underline-color/
> OSC8/kitty-graphics detection), `gpu_frame.rs` (synchronized output, OSC8),
> `kitty_gfx.rs` (RGBA placement + b64 cache). Gates: `gpu_acc` config +
> `kitty_gfx::available` (7 call sites), `should_*` helpers.

## G1 — Images as first-class content (biggest visible win)

- **Inline images in the Markdown preview**: `![alt](path)` renders the actual
  picture inside the pretty view (Kitty placement anchored to the preview
  row; scroll moves/deletes placements). Sizing: fit column width, cap rows.
- Explorer: image thumbnail popup on selection (small placement, Esc/move to
  dismiss).
- **GfxRegistry**: one placement manager (id allocation, z-order, lifetime,
  frame-sync cleanup) replacing today's hardcoded ids (pet=42 etc.). All
  placements move through it so a resize/scroll/mode-switch can't strand
  ghost images.

## G2 — One synced frame, graphics included

- Today ratatui draws inside BEGIN/END sync but images are written *after* —
  visible as one-frame tearing on kitty/ghostty. Move placement writes inside
  the same sync window (registry flush hook in `draw_synced`).

## G3 — Text decoration everywhere it makes sense

- Undercurl + underline-color already on diagnostics — extend to: search
  matches (dotted), git conflict markers, spell-ish lint underlines from LSP
  tags.
- OSC8 hyperlinks: preview links, PR review URLs (comment rows), `gd` peek
  header path, breadcrumbs → clickable in supporting terminals.

## G4 — Detection & settings UX

- Settings → Setting: per-feature toggles under `gpu_acc` (graphics / sync /
  hyperlinks / undercurl) — one bad terminal shouldn't force all-off.
- Settings → About: caps read-out ("ghostty: sync ✓ gfx ✓ osc8 ✓ …").
- Protocol scope decision (owner call): Kitty-graphics only (ghostty/kitty/
  wezterm cover most users) vs adding iTerm2 inline-images and/or Sixel
  fallback. Recommendation: **Kitty only for 3.0.6**; revisit by demand.
- Non-goal: tmux passthrough (fragile; document as unsupported).

## G5 — Performance guardrails

- Decode/resize once per (path, cols) — extend the existing sig-cache pattern
  from media preview into GfxRegistry.
- Placement diffing: reposition instead of delete+re-upload when only the row
  changed (kitty `p=` placement move).
- Budget: no per-frame uploads at idle; registry flush is no-op when clean
  (fits the 3.0.3 dirty-render model).

## Order of work

1. GfxRegistry + synced flush (G2 foundation)
2. Preview inline images (G1) — the flagship feature
3. Settings toggles + About caps (G4)
4. OSC8 sweep + underline extensions (G3)
5. Explorer thumbnails (G1 stretch)

Release criteria: preview renders images in ghostty/kitty with clean scroll
behavior; zero ghost placements across mode switches/resize; all features
individually toggleable; no idle-CPU regression vs 3.0.3.
