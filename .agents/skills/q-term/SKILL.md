---
name: q-term
description: Deep dive on q_term (lib/q_term), the bevy_ui terminal emulator — Terminal entity and its Vt* components, VtUi frontend, VtWriteMsg/VtReplyMsg/TermInputMsg/TermViewportMsg, LineDiscipline, TermWrite styled spans, supported ANSI/VT subset, TerminalSystems ordering, readiness buffering and background-output policy. Use when rendering text in a terminal, writing escape sequences, extending the VT parser, or debugging terminal layout/scroll/cursor.
---

# q_term

Source: `lib/q_term/src` (`data/` types, `msgs/term.rs` message handling,
`systems/term.rs` measure/render, `ansi.rs` parser + grid). Examples:
`examples/{minimal,input,palette,spinner}.rs`. Tests: `tests/term/*.rs`
(one file per behavior — find the nearest). `use q_term::prelude::*;`.

q_term knows nothing about processes or shells. It turns bytes into a grid.

## Setup

```rust
app.add_plugins(TerminalPlugin::default());          // or ::new(MySchedule)
let term = commands.spawn(Terminal).id();            // backend state
commands.spawn((Node { width: vw(100), height: vh(100), ..default() },
                BackgroundColor(Color::BLACK),
                VtUi::new(term)));                   // bevy_ui frontend
commands.write_message(VtWriteMsg::new(term, b"hello\r\n".to_vec()));
```

- `Terminal` (immutable marker) requires the backend: `VtCursor`, `VtModes`,
  `VtSize`, `VtLineTarget`, `VtViewport`, `VtScrollPos`, `VtTabStop`, parser
  and render state. Close a terminal by **despawning** the entity.
- `VtUi(term)` is the only frontend (docs mentioning `VtUi2d` are stale). Its
  `on_add` spawns a hidden `VtCharWidth` measurer, a `VtUiGrid` text child and
  a `VtUiCursor` child. Style text through `TextFont`/`TextColor`/
  `LineHeight` on the `VtUi` entity; `VtCursorStyle`, `VtCursorColor`,
  `VtStrobeTimer` on the cursor child.
- `VtSize` is computed from the node size / char width / line height
  (HiDPI-corrected). Inserting `VtSize` removes `VtReady` and requests reflow.

## Messages

| message | direction | notes |
| --- | --- | --- |
| `VtWriteMsg { term, from, bytes }` | in | `new` (direct) / `from_peer(term, from, bytes)` |
| `VtReplyMsg { term, bytes }` | out | protocol replies (DSR, DA) |
| `TermInputMsg { term, input: TermInput }` | in | `Text/Submit/Erase/Eof`; **q_term itself doesn't consume it** — q_shell's line discipline does |
| `TermViewportMsg::{scroll(term, delta), jump_bottom(term)}` | in | positive delta = older content |
| `TermReflowMsg`, `TermRedrawRequestedMsg` | internal | reflow on resize; redraw request |

- Writes before the terminal is `VtReady` are queued in `PendingVtWrites`,
  capped by `PendingVtWriteCap` (1 MiB default, oldest evicted with a warn).
  Viewport messages likewise (`PendingTermViewportCap`).
- Writes to a non-`Terminal` entity are dropped with a warning.
- `BackgroundTerminalOutput::Suppress` drops writes whose `from` isn't the
  terminal's foreground peer (`VtForegroundProcess` relationship,
  `VtForegroundProcessTarget` on the terminal). Default `Allow`.

Styled output without hand-written escapes:

```rust
let bytes = term_writes_to_ansi([
    TermWrite::new("ok ").with_color(css::GREEN),
    TermWrite::new("plain").reset_style(true),
]);
```

`TermInfo` is a `QueryData` bundling the terminal's components, with helpers
`write`, `write_from`, `write_spans`, `lines`, `rows`, `viewport_rows`.

## Supported VT subset (`ansi.rs`)

- C0: BEL (logged), BS (non-destructive), HT, LF (new line, col 0), CR.
- CSI: CUU/CUD/CUF/CUB, CNL/CPL, CHA, CUP/HVP, ED, EL, ECH, ICH, DCH, DSR,
  DA, SGR (reset, 24-bit fg/bg via `;` or `:`; 256-color and conceal
  unsupported), SM/RM for DECTCEM (`?25`) and DECAWM (`?7`).
- No OSC, no ESC dispatch, no raw mode/TUI support. Unsupported codes are
  ignored with an `info_once!`.

Adding a mode: add a field to `VtModes`, then handle it in `set_modes` in
`ansi.rs` (the single dispatch site).

## Data model

```text
Terminal ─VtLineTarget→ VtLine(cells) ─VtRowTarget→ VtRow{offset}  (wrapped rows)
Terminal ─VtViewport→ VtViewportRow   (visible rows; may also carry VtRow)
```

`VtCursor { row, col, pending_wrap }` is viewport-relative (top-left origin).
`VtScrollPos(0)` = bottom. Scroll input accumulates in `VtScrollAccumulator`
scaled by `VtScrollSensitivity { line, pixel }`.

## Scheduling

`TerminalSystems::{Input, Measure, Process, Render}` chained in the
configured schedule (default `Update`). `Input` is empty in q_term — it's the
slot for consumers (q_shell). `Render` runs `refresh_ui` before `PostUpdate`
so new spans lay out the same frame.

## Gotchas

- Process output through q_shell conventionally uses `\r\n` (see q_shell
  examples); prefer that for shell-facing text.
- Line discipline (`LineDiscipline::Canonical { buffer } | Raw`) is a
  component on the terminal, but editing/echo is implemented in q_shell, and
  `Raw` is not implemented.
- Tests needing layout use the full app with UI; see
  `tests/term/hello_world.rs` / `resize_hidpi.rs` for setup.
