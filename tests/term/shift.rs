//! Tests for ECH (`CSI n X`), ICH (`CSI n @`), DCH (`CSI n P`).
//!
//! All three operate on the cursor's current row only:
//! - **ECH** blanks `n` cells starting at the cursor without shifting.
//! - **ICH** shifts cells right by `n`, dropping any pushed past the
//!   right edge of the row, then blanks the freshly-vacated cells.
//! - **DCH** shifts cells right of `cursor + n` left by `n`, then
//!   blanks the `n` cells at the right edge.
//!
//! Blanks adopt the current SGR style (xterm behaviour). None of these
//! sequences move the cursor.

use bevy::color::palettes::basic;

use crate::prelude::*;

/// Same harness as `erase.rs`: spawn a `cols x rows` terminal, feed
/// `input`, then on step 0 run `check`. `check` returns `true` on
/// success.
fn shift_test(
    cols: usize,
    rows: usize,
    input: &'static str,
    check: impl Fn(TermInfoItem, &[(Entity, &VtLine)], &mut Commands) -> bool + Send + Sync + 'static,
) {
    let mut app = get_test_app();
    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands.entity(term_id).insert(VtSize { cols, rows });
        commands.write_message(StdOut::write(term_id, input));
    });
    app.add_step(
        0,
        move |q_term: Query<TermInfo>,
              q_lines: Query<(Entity, &VtLine)>,
              mut commands: Commands| {
            let terminfo = r!(q_term.single());
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }
            if check(terminfo, &lines, &mut commands) {
                commands.write_message(AppExit::Success);
            } else {
                commands.write_message(AppExit::error());
            }
        },
    );
    assert!(app.run().is_success());
}

// ---------------------------------------------------------------------------
// ECH (CSI n X) — Erase Character
// ---------------------------------------------------------------------------

/// `\x1b[3X` blanks three cells at the cursor without shifting the tail.
/// "ABCDEFG", CUP to col 3 (cursor on 'C'), ECH 3 → "AB   FG".
#[test]
fn ech_blanks_n_cells_no_shift() {
    shift_test(10, 3, "ABCDEFG\x1b[1;3H\x1b[3X", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(s == "AB   FG", format!("ECH expected 'AB   FG', got {s:?}")))
    });
}

/// ECH count defaults to 1 when omitted.
#[test]
fn ech_default_count_is_one() {
    shift_test(10, 3, "ABCDEFG\x1b[1;3H\x1b[X", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(
            s == "AB DEFG",
            format!("ECH default expected 'AB DEFG', got {s:?}")
        ))
    });
}

/// ECH clamps the count to the row's right edge.
#[test]
fn ech_clamps_past_row_end() {
    shift_test(5, 3, "ABCDE\x1b[1;3H\x1b[99X", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(
            s == "AB   ",
            format!("ECH clamp expected 'AB   ', got {s:?}")
        ))
    });
}

/// ECH must not move the cursor.
#[test]
fn ech_preserves_cursor() {
    shift_test(10, 3, "ABCDEFG\x1b[1;3H\x1b[3X", |terminfo, _, commands| {
        r!(commands.assert(
            terminfo.cursor.row == 0 && terminfo.cursor.col == 2,
            format!(
                "ECH must preserve cursor at (0, 2), got ({}, {})",
                terminfo.cursor.row, terminfo.cursor.col,
            ),
        ))
    });
}

/// Blanked cells take the current SGR background colour.
#[test]
fn ech_uses_current_bg_color() {
    shift_test(
        10,
        3,
        "ABCDEFG\x1b[101m\x1b[1;3H\x1b[3X",
        |_, lines, commands| {
            let red = Color::from(basic::RED);
            let cells = lines[0].1.cells();
            let ok = cells.len() >= 5
                && cells[2..5]
                    .iter()
                    .all(|c| c.value == ' ' && c.style.background == red);
            r!(commands.assert(
                ok,
                format!("ECH expected 3 red-bg spaces at [2..5], got {cells:?}")
            ))
        },
    );
}

// ---------------------------------------------------------------------------
// ICH (CSI n @) — Insert Character
// ---------------------------------------------------------------------------

/// `\x1b[2@` shifts cells at the cursor right by 2, dropping the tail
/// that falls off the right edge. "ABCDEFG" (7 cells, cols=7), CUP to
/// col 3 (cursor on 'C'), ICH 2 → "AB  CDE" (F and G fall off).
#[test]
fn ich_shifts_right_drops_tail() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[2@", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(s == "AB  CDE", format!("ICH expected 'AB  CDE', got {s:?}")))
    });
}

/// ICH count defaults to 1 when omitted.
#[test]
fn ich_default_count_is_one() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[@", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(
            s == "AB CDEF",
            format!("ICH default expected 'AB CDEF', got {s:?}")
        ))
    });
}

/// ICH with `n` >= remaining cells blanks the rest of the row.
#[test]
fn ich_count_past_row_blanks_all() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[99@", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(
            s == "AB     ",
            format!("ICH clamp expected 'AB     ', got {s:?}")
        ))
    });
}

/// ICH must not move the cursor.
#[test]
fn ich_preserves_cursor() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[2@", |terminfo, _, commands| {
        r!(commands.assert(
            terminfo.cursor.row == 0 && terminfo.cursor.col == 2,
            format!(
                "ICH must preserve cursor at (0, 2), got ({}, {})",
                terminfo.cursor.row, terminfo.cursor.col,
            ),
        ))
    });
}

/// Inserted blanks adopt the current SGR background colour.
#[test]
fn ich_uses_current_bg_color() {
    shift_test(
        7,
        3,
        "ABCDEFG\x1b[101m\x1b[1;3H\x1b[2@",
        |_, lines, commands| {
            let red = Color::from(basic::RED);
            let cells = lines[0].1.cells();
            let ok = cells.len() >= 4
                && cells[2..4]
                    .iter()
                    .all(|c| c.value == ' ' && c.style.background == red);
            r!(commands.assert(
                ok,
                format!("ICH expected 2 red-bg spaces at [2..4], got {cells:?}")
            ))
        },
    );
}

/// ICH on a *short* logical line (cells_len < cols) must preserve the
/// in-storage tail and only drop content that falls past the right
/// margin. cols=10, line "ABC", cursor at col 0, ICH 2 → "  ABC"
/// followed by trailing blanks (right margin is at col 10).
#[test]
fn ich_short_line_preserves_in_storage_tail() {
    shift_test(10, 3, "ABC\x1b[1;1H\x1b[2@", |_, lines, commands| {
        let s = lines[0].1.as_string();
        // Trim only trailing whitespace -- leading blanks are the inserted cells.
        let trimmed = s.trim_end();
        r!(commands.assert(
            trimmed == "  ABC",
            format!("ICH short-line expected '  ABC' (rtrimmed), got {s:?}"),
        ))
    });
}

// ---------------------------------------------------------------------------
// DCH (CSI n P) — Delete Character
// ---------------------------------------------------------------------------

/// `\x1b[2P` deletes 2 cells at the cursor; later cells shift left and
/// the trailing 2 cells become blanks. "ABCDEFG" (cols=7), CUP to col 3
/// (cursor on 'C'), DCH 2 → "ABEFG  ".
#[test]
fn dch_shifts_left_pads_tail() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[2P", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(s == "ABEFG  ", format!("DCH expected 'ABEFG  ', got {s:?}")))
    });
}

/// DCH count defaults to 1 when omitted.
#[test]
fn dch_default_count_is_one() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[P", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(
            s == "ABDEFG ",
            format!("DCH default expected 'ABDEFG ', got {s:?}")
        ))
    });
}

/// DCH with `n` >= remaining cells blanks the rest of the row.
#[test]
fn dch_count_past_row_blanks_all() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[99P", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(
            s == "AB     ",
            format!("DCH clamp expected 'AB     ', got {s:?}")
        ))
    });
}

/// DCH must not move the cursor.
#[test]
fn dch_preserves_cursor() {
    shift_test(7, 3, "ABCDEFG\x1b[1;3H\x1b[2P", |terminfo, _, commands| {
        r!(commands.assert(
            terminfo.cursor.row == 0 && terminfo.cursor.col == 2,
            format!(
                "DCH must preserve cursor at (0, 2), got ({}, {})",
                terminfo.cursor.row, terminfo.cursor.col,
            ),
        ))
    });
}

/// Trailing blanks adopt the current SGR background colour.
#[test]
fn dch_uses_current_bg_color() {
    shift_test(
        7,
        3,
        "ABCDEFG\x1b[101m\x1b[1;3H\x1b[2P",
        |_, lines, commands| {
            let red = Color::from(basic::RED);
            let cells = lines[0].1.cells();
            let ok = cells.len() == 7
                && cells[5..7]
                    .iter()
                    .all(|c| c.value == ' ' && c.style.background == red);
            r!(commands.assert(
                ok,
                format!("DCH expected 2 red-bg spaces at [5..7], got {cells:?}")
            ))
        },
    );
}

/// DCH on a *short* logical line must extend the row to the right
/// margin. xterm only styles the `n` cells at the right edge (the
/// trailing pad cells between `cells_len` and `row_end - n` were
/// already implicitly blank with default style). cols=7, line
/// "ABC", set bg to bright red, cursor at col 0, DCH 1 →
/// `'B','C'` then four default-bg blanks then one red-bg blank.
#[test]
fn dch_short_line_pads_to_margin_with_style() {
    shift_test(
        7,
        3,
        "ABC\x1b[101m\x1b[1;1H\x1b[1P",
        |_, lines, commands| {
            let red = Color::from(basic::RED);
            let default_bg = VtCellStyle::default().background;
            let cells = lines[0].1.cells();
            let layout_ok = cells.len() == 7
                && cells[0].value == 'B'
                && cells[1].value == 'C'
                && cells[2..6]
                    .iter()
                    .all(|c| c.value == ' ' && c.style.background == default_bg)
                && cells[6].value == ' '
                && cells[6].style.background == red;
            r!(commands.assert(
                layout_ok,
                format!(
                    "DCH short-line expected 'BC' + 4 default blanks + 1 red blank (len 7), got {cells:?}",
                ),
            ))
        },
    );
}

// ---------------------------------------------------------------------------
// Boundary cases shared by all three ops
// ---------------------------------------------------------------------------

/// On a brand-new terminal with no input, the cursor row has no
/// backing line; ECH / ICH / DCH must no-op and exit cleanly.
#[test]
fn ech_ich_dch_no_op_on_empty_grid() {
    // Issue the three sequences immediately on a fresh terminal --
    // nothing has been written, so `cursor_row_span` returns None.
    // We assert the harness reaches step 0 with zero lines, which
    // means no panic and no spurious line creation.
    let mut app = get_test_app();
    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 10, rows: 3 });
        commands.write_message(StdOut::write(term_id, "\x1b[3X\x1b[2@\x1b[2P"));
    });
    app.add_step(
        0,
        move |q_term: Query<TermInfo>,
              q_lines: Query<(Entity, &VtLine)>,
              mut commands: Commands| {
            let terminfo = r!(q_term.single());
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            // No content was ever written, so no VtLine should exist,
            // and the cursor should still be at the origin.
            let ok = lines.is_empty() && terminfo.cursor.row == 0 && terminfo.cursor.col == 0;
            if ok {
                commands.write_message(AppExit::Success);
            } else {
                commands.write_message(AppExit::error());
            }
        },
    );
    assert!(app.run().is_success());
}
