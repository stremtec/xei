# Beyond cell-grid TUI — quality on GPU terminals

xei today is a **character-cell TUI** (crossterm + ratatui). That model is portable and fast, but it caps visual quality: no sub-cell glyphs, no true fractional scroll, limited anti-aliasing, no native images/shaders.

This note sketches how to keep the **same engine** (`xei-core`) while unlocking better fidelity on GPU terminals (Ghostty, Kitty, WezTerm, iTerm2, Foot with protocols, …).

---

## 1. What “GPU terminal” actually buys us

| Capability | Who | What it enables for an editor |
|------------|-----|-------------------------------|
| **Kitty graphics protocol** | Kitty, Ghostty, WezTerm, … | RGBA bitmaps in the grid → soft glyphs, UI chrome, previews |
| **Sixel** | Some older/alt terminals | Lower-quality images; fallback only |
| **iTerm2 inline images** | iTerm2 | Same idea as Kitty, different escape sequence |
| **Undercurl / styled underlines** | Kitty, Ghostty, WezTerm | Squiggly diagnostics like VS Code |
| **RGB truecolor + styled underline colors** | Most modern | Theme fidelity |
| **Synchronized output** (`DEC 2026`) | Kitty, Ghostty, WezTerm | Tear-free full redraws |
| **Focus / paste / bracketed paste** | Wide | Correctness, not beauty |
| **Hyperlinks OSC 8** | Kitty, Ghostty, WezTerm | Clickable peek/definition links |
| **Text sizing / Unicode width edge cases** | Varies | CJK/emoji still need careful measurement |

GPU compositing itself is **inside the terminal emulator**. Our job is to emit richer **protocols** and, optionally, a second rendering backend that is not pure cells.

---

## 2. Layered strategy (recommended)

### Phase A — Stay cell-based, use every escape we can (low risk)

Ship without leaving ratatui:

1. **Capability detect** once at startup (`TERM`, `TERM_PROGRAM`, query DA / kitty features).
2. **Synchronized output** around each frame paint → no flash on Ghostty/Kitty.
3. **Undercurl diagnostics** instead of plain underline when supported.
4. **OSC 8 hyperlinks** on peek / search hits / git commit SHAs.
5. **Kitty keyboard protocol** (progressive) for disambiguating `Ctrl+Shift+F` vs `Ctrl+F` on more hosts.
6. **Theme tokens** that map to truecolor RGB (already mostly true).

*Effort:* small. *Gain:* noticeably “modern IDE” feel without architecture change.

### Phase B — Hybrid: cell chrome + graphics overlays (medium)

Keep layout in character cells; paint **overlays** with Kitty graphics:

| Overlay | Source |
|---------|--------|
| Welcome / About art | Pre-rendered or vector→raster |
| Peek definition card | Soft shadow, code bitmap from skia/cosmic-text |
| Markdown preview (rich) | Optional HTML→image or GPU text layout |
| Minimap | Downscaled bitmap of buffer |
| Cursor / selection glow | Subpixel decoration |

Pattern:

```
each frame:
  1. ratatui → cell buffer (editor text, gutters, status)
  2. place/delete Kitty images by id for overlays
  3. synchronized output flush
```

Image ids must be **stable** across frames to avoid flicker; dirty only changed overlays.

*Effort:* medium. *Gain:* VS Code-like cards without abandoning TUI navigation.

### Phase C — Dual backend: `xei` TUI + `xei-gpu` / suisei path (high)

`xei-core` stays pure (buffer, LSP, git, ops). Frontends:

| Frontend | Backend |
|----------|---------|
| `xei` | cells + Phase A/B protocols |
| `suisei` (Tauri) | web or native GPU UI |
| future `xei-gpu` | winit + wgpu/cosmic-text full canvas |

Full canvas editor:

- **cosmic-text** / **parley** for shaping, ligatures, emoji
- **wgpu** or platform compositor for scroll, blur, shadows
- Still run **inside** a terminal only if we embed via graphics protocol as a single full-window image (possible but awkward for input). Prefer **desktop window** for Phase C, keep terminal as first-class for SSH/remote.

---

## 3. Concrete Ghostty / Kitty checklist for xei

### Detect

```text
$TERM_PROGRAM = ghostty | iTerm.app | WezTerm
$KITTY_WINDOW_ID set → Kitty family
Query: CSI ? u  (keyboard), graphics query G
```

Store `TerminalCaps { undercurl, graphics_kitty, sync_output, hyperlinks, … }`.

### Render path hooks (today’s code)

| Site | Hook |
|------|------|
| `xei/src/main.rs` draw loop | Begin/end synchronized update |
| `ui.rs` diagnostics | Undercurl style when caps allow |
| `draw_peek` / palette | Optional graphics-backed card |
| Pretty preview | Phase B: render MD to image for tables/math later |
| Inlay hints | Already dim italic cells; later subpixel “ghost” text via graphics |

### Input

Ghostty/Kitty handle multi-modifier chords more reliably with **Kitty keyboard protocol**. Enable on detect so `Ctrl+Shift+F` / `Ctrl+W` never collide with legacy CSI-u gaps.

---

## 4. What not to do

- **Don’t** require GPU terminal for basic editing — ASCII path must stay excellent over `tmux` + `ssh` + stock Terminal.app.
- **Don’t** put business logic in the GPU frontend — `xei-core` remains the product brain.
- **Don’t** full-window bitmap every frame at 120 Hz without dirty regions — bandwidth and battery will suffer even on Ghostty.
- **Don’t** assume Sixel quality for text — only for rare images.

---

## 5. Suggested roadmap slice

| Sprint | Deliverable |
|--------|-------------|
| **S1** | ✅ `TerminalCaps` + synchronized output + colored undercurl diagnostics + `gpu_acc` gate |
| **S2** | OSC 8 links on peek / workspace search / git SHAs (partial: caps + path ↗) |
| **S3** | Kitty keyboard protocol progressive enhancement |
| **S4** | Peek / About card via Kitty graphics (shadow + crisp text) |
| **S5** | Optional minimap graphics overlay |
| **S6** | Evaluate cosmic-text path for `suisei` parity; share line metrics with TUI soft-wrap |

### S1 implementation map (landed)

| Piece | Location |
|-------|----------|
| Caps detect | `xei/src/term_caps.rs` |
| Sync frames | `xei/src/gpu_frame.rs` + `main.rs` draw loop |
| ~~Session undercurl SGR `4:3`~~ | **Removed** — sticky `4:3` painted waves on every padding cell (Ghostty/Kitty) |
| Colored diag underline | `ui::diag_underline_style` + ratatui `underline-color` (per-span only) |
| User toggle | `gpu_acc` in Settings / `~/.xei.toml` |
| Status | `GPU` badge when `App::gpu_active()` |

---

## 6. Mental model

```
                 ┌──────────── xei-core ────────────┐
                 │  buffers · LSP · git · search    │
                 └───────────────┬──────────────────┘
                                 │
           ┌─────────────────────┼─────────────────────┐
           ▼                     ▼                     ▼
     cell renderer         hybrid renderer        GPU window
     (ratatui)             (cells+Kitty img)      (suisei/wgpu)
     portable SSH          Ghostty/Kitty joy      max fidelity
```

**Principle:** progressive enhancement. Default is a great cell TUI; Ghostty/Kitty users automatically get undercurl, tear-free frames, then soft overlays; desktop shell takes the rest.

---

## 7. Relation to P0 just landed

P0 (splits, workspace search, symbols, peek, inlays) is still **cell-native**. That is correct: features first, chrome second.

Next quality wins that pair well with P0:

1. Peek card → Phase B graphics (biggest “wow” for definition UX).
2. Workspace search list → hyperlinks + smoother scroll (sync output).
3. Inlay hints → subpixel ghost labels only if caps allow; else keep italic cells.

### User toggle (`Ctrl+,` → Setting)

```toml
# ~/.xei.toml
gpu_acc = true    # false = force plain cell TUI
```

Aliases accepted on load: `gpu_acceleration`, `graphics` (`auto`/`kitty`/`ghostty` → on, `off` → off).

In Settings: **gpu_acc** row under Editor — Enter toggles, `s` saves. Runtime flag is `App.gpu_acc`; render paths should gate Phase A/B enhancements on this.
