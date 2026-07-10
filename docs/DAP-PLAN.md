# DAP v2 — Audit & Development Plan

> **Progress (2026-07-10):** D1–D3 ✅ · D4 nearly complete (REPL · launch.json ·
> conditional BPs · **attach** · **Node TCP**).  
> Remaining: suisei parity (deferred).
>
> Status of `xei-core/src/dap.rs` as of 2026-07-10, plus the
> phased plan to make debugging actually reliable and VS Code-grade.
>
> v1 surface today: `DapClient` (stdio Content-Length transport, mpsc reader
> thread, poll-based like LSP), adapters debugpy / dlv / lldb-dap, F5/F9/F10/F11,
> `Mode::Debug` bottom panel (Stack · Vars · BPs · Console), gutter ● / ▶,
> `SPC d *` which-key, XLC `:dap*` commands.

---

## Part 1 — Audit: what is wrong with v1

### A. Protocol correctness (blocks real sessions)

| # | Problem | Where | Impact |
|---|---------|-------|--------|
| A1 | **Init sequence is inverted.** Spec order: `initialize` → resp → **`launch`** → adapter emits `initialized` → `setBreakpoints*` → `configurationDone` → launch resp. v1 fires setBreakpoints → configurationDone → launch immediately after the initialize *response* ("pragmatic fallback" always wins because debugpy/lldb-dap only emit `initialized` **after** receiving `launch`). The real `initialized`-event path is dead code. | `dap.rs` `handle_response(Initialize)` / `finish_config_and_launch` | debugpy rejects/ignores `configurationDone` before launch; BPs set before launch may never verify. The #1 fix. |
| A2 | **`stop()` SIGKILLs the adapter right after writing `disconnect`.** No grace period. | `stop()` → `shutdown_quiet()` | debugpy's launcher/debuggee get orphaned; dlv leaves build artifacts. Need: send `terminate`/`disconnect`, wait for `terminated` (≤1s), then kill. |
| A3 | **No `threads` request, single-thread only.** `thread_id` comes only from the `stopped` event; if the event omits `threadId` (legal with `allThreadsStopped`) the whole session is stuck — no stack, no step. | `handle_event("stopped")` | Fails on real multithreaded targets (Go immediately). |
| A4 | **`breakpoint` event ignored** — post-launch verification never updates UI; BPs can show ○ forever (worse combined with A1). | `handle_event` | Misleading UI. |
| A5 | State machine holes: state forced to `Running` inside `finish_config_and_launch` before launch resp; `exited` treated as full shutdown (final `output` events can be lost); `Ending` unreachable. | `dap.rs` | Wrong status display, lost output. |
| A6 | `pending` map leak: `finish_config_and_launch` allocs a `PendingKind::Launch` id and discards it (`let _ = id`). Response-id fallback also reads the adapter-side `seq`, which can mis-match pending entries. | `finish_config_and_launch`, `handle_response` | Slow leak + potential wrong dispatch. |
| A7 | `read_loop` parses each body JSON twice, `handle_raw` a third time. | transport | Waste; parse once, pass `Value`. |
| A8 | `setExceptionBreakpoints` never sent — debugpy won't break on uncaught exceptions at all. | launch flow | Missing core behavior. |

### B. Feature gaps (vs VS Code / nvim-dap baseline)

- **B1 No program args / env / cwd override** — can't debug anything that takes arguments. No launch.json support.
- **B2 No `pause`** — a looping program can only be killed.
- **B3 No evaluate/REPL/watch** — Console is output-only.
- **B4 No attach mode.**
- **B5 No conditional breakpoints / logpoints.**
- **B6 Breakpoints don't track edits** — insert/delete lines above a BP and it silently points at the wrong line (git-gutter already solves this pattern). Not persisted in session either.
- **B7 Rust launch is a guess** — returns `target/debug/<name>` even when it doesn't exist ("user may cargo build"); lldb-dap then fails cryptically. No build step, no test-binary support.
- **B8 Node path is broken by design** — `js-debug-adapter` is a TCP DAP server, not stdio; the current spawn can never work.
- **B9 Variables pane is broken**: `expand_variable` *replaces* the flat list with children (no back, no indent, no collapse); scope switching dead — `move_focus` updates `selected_scope` but never re-requests variables, and `select_scope()` is never called by any frontend.

### C. UI/UX

- **C1 Panel is modal.** `panel_open` and `Mode::Debug` are welded together: while the panel is open you cannot edit, and Esc destroys the panel instead of just returning focus. VS Code separates *visible* from *focused*.
- **C2 Gutter render burns syscalls**: per visible row per frame — `has_breakpoint` calls `fs::canonicalize`, `is_debug_line` calls it twice more. At ~100fps × 50 rows that's thousands of canonicalize syscalls/sec during a session. Needs a canonical-path cache (compute once per frame / per open).
- **C3 Console**: tail-only, no scrollback; `focus_row` selection is invisible when above the tail.
- **C4 Stack pane j/k spams** scopes+variables requests on every keypress (should fire on Enter / debounced).
- **C5 No mouse** (panel tabs, frames, gutter-click BP toggle) — everything else in xei is mouse-first.
- **C6 No status-bar DAP badge**; stopped line has ▶ glyph but no line background highlight; BP+stopped on same line hides ▶ (● wins).
- **C7 `flat_bps()` clones all paths every frame.**

### D. Architecture / consistency

- **D1 `serde_json` entered xei-core** for dap.rs while lsp.rs stays hand-rolled. Decision: **keep serde_json for DAP** (client-initiated protocol, deep nesting — hand-rolling buys nothing) and record the rule: *hand-rolled parsing for LSP legacy, serde_json allowed for new protocol modules*. Alternative (port DAP to hand-rolled) costs days for zero user value.
- **D2 Tests are utility-only** (4 tests: toggle, lang detect, cargo name, label). Zero protocol-sequence coverage. dap.rs needs what lsp.rs never got: a **scripted mock adapter** (in-process: feed `handle_raw` directly, or a thread+pipe pair) asserting the full happy-path sequence and failure paths.
- **D3 suisei has zero DAP integration** (no poll, no UI) — known parity gap, keep out of scope until Phase D4.
- **D4 Dead API**: `DapClient::is_active`, `select_scope` unused by any frontend.

---

## Part 2 — How DAP hooks into today's xei

Integration points that already exist and stay (all follow the LSP pattern —
core owns state, frontends poll + render):

```
xei-core/src/dap.rs      DapClient — transport, state, BPs (headless-safe)
xei-core/src/app.rs      dap_* methods, Mode::Debug, dap_apply_stopped_location
xei/src/main.rs          app.dap.poll() + location_dirty jump   (per frame)
xei/src/event.rs         F5/F9/F10/F11 (all edit modes) · handle_debug() · SPC d *
xei/src/ui.rs            draw_editor_with_debug (bottom dock) · gutter ● ▶
```

New touch points the plan adds:

- `app.rs` edit paths → `dap.shift_breakpoints(path, row, ±n)` (mirror git-gutter's line-shift logic)
- `ui.rs` status bar → DAP badge; mouse hit-regions for panel + gutter BPs
- `session.rs` → persist breakpoints per path
- `main.rs` → nothing new (poll already in place)

---

## Part 3 — Phased plan

### Phase D1 — Protocol core repair *(make sessions correct)*  ← **done**

The whole phase is inside `dap.rs`; no UI changes.

1. **Sequencer rewrite** (fixes A1, A5, A6):
   - after initialize resp → send `launch` immediately; hold `configurationDone`
   - on `initialized` event → `setBreakpoints*` → `setExceptionBreakpoints`
     (default: uncaught) → `configurationDone`
   - fallback timer: if no `initialized` within ~2s, proceed anyway (non-compliant adapters)
   - introduce explicit `Launching` state; `Running` only on launch resp / `continued`
   - `configurationDone` gets its own `PendingKind::ConfigDone` (no leaked ids)
2. **Graceful shutdown** (A2): `terminate` if `supportsTerminateRequest` else
   `disconnect(terminateDebuggee)`; reap on `terminated` event or 1s deadline in
   `poll()` (deadline field, no blocking); only then kill. `exited` records exit
   code but does not tear down.
3. **`threads` request** (A3): fire on `stopped` without threadId and cache the
   list; pick stopped/first thread. Store `threads: Vec<(i64, String)>` for D3 UI.
4. **`breakpoint` event handling** (A4) + single-parse transport (A7) + drop the
   `seq` fallback in response matching (A6).
5. **Mock-adapter test harness** (D2): `tests` feed scripted `RawMsg` sequences
   through `handle_raw`; golden tests for happy path (debugpy-style ordering),
   missing-`initialized` fallback, failed launch, stopped-without-threadId,
   breakpoint re-verification, graceful stop.

*Done when:* `python3 -m debugpy.adapter` real-session smoke test (BP → stopped
→ step → continue → exit, no orphan process) passes; sequence tests green.

### Phase D2 — Reliability & daily usability  ← **done**

1. [x] **BP line tracking** (B6): `shift_breakpoints` on newline / `dd`; live
   adapter update; **persisted** to `~/.xei/breakpoints`.
2. [x] **Gutter perf** (C2): `canon_cache` + `has_breakpoint` / `current_line_for`.
3. [x] **Rust launch UX** (B7): missing binary → async `cargo build` → auto-launch.
4. [x] **Program args** (B1): `:DapLaunch <prog> [args…]` + `last_args` for restart.
5. [x] **`pause`** (B2): F6 + `SPC d p`.
6. [x] **Console scrollback** (C3): focus follows tail when at end.
7. [x] Debounce stack-pane requests (C4): j/k only moves focus; Enter loads scopes.

### Phase D3 — UI: VS Code feel  ← **done**

1. [x] **Split visible/focused** (C1): Esc unfocus · `q` close · Ctrl+Shift+D.
2. [x] **Variables tree** (B9): indent + expand/collapse.
3. [x] **Threads** (D1.3): resolve threadId via `threads` request.
4. [x] **Mouse** (C5): panel tab clicks · list row clicks · **gutter click = BP**.
5. [x] **Status bar badge** (C6) + BP+stopped `◉`.
6. [x] **Panel entrance animation**: slide-up ~200ms.

### Phase D4 — Depth  ← **done** (suisei deferred)

1. [x] **Evaluate/REPL** — Console pane input line + `evaluate` (repl context).
2. [x] **Conditional BPs / logpoints** — `:bp if <expr>` · `:bp log <msg>`.
3. [x] **launch.json subset** — parse `.vscode/launch.json` · `:DapConfig [name]` · `SPC d c`.
4. [x] **Attach mode** — `:DapAttach pid <n>` · `:DapAttach port <n> [python|node]` · launch.json attach.
5. [x] **Node TCP transport** — `js-debug-adapter --server=PORT` + TcpStream DAP; auto for `.js`/`.ts`.
6. [ ] **suisei parity** (deferred).

---

## Decisions taken

- **serde_json stays in xei-core for DAP** (documented exception to the
  hand-rolled-JSON rule; LSP module unchanged).
- Panel remains a **bottom dock on the editor z-layer** (no overlay), consistent
  with the in-pane transition preference.
- v1's public API (`dap_*` App methods, key map) is kept stable through D1–D2 so
  frontends don't churn; `is_active`/`select_scope` get used or deleted in D3.
