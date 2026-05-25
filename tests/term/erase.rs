//! Tests for ED (`CSI J`) and EL (`CSI K`).
//!
//! Semantics target the xterm/VT100 model:
//! - Mode 0: from cursor to end (of line / screen).
//! - Mode 1: from start (of line / screen) to cursor, **inclusive**.
//! - Mode 2: entire line / screen.
//!
//! Erased cells are replaced with spaces. The cursor does not move.

use bevy::color::palettes::basic;

use crate::prelude::*;

/// Spawn a terminal sized `cols x rows`, write `input`, then on the
/// first step run `check`. `check` returns `true` on success.
fn erase_test(
    cols: usize,
    rows: usize,
    input: &'static str,
    check: impl Fn(TermInfoItem, &[(Entity, &VtLine)], &mut Commands) -> bool + Send + Sync + 'static,
) {
    let mut app = get_test_app();
    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands.entity(term_id).insert(VtSize { cols, rows });
        commands.write_message(TermStdOut::write(term_id, input));
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
// EL (CSI K) — Erase in Line
// ---------------------------------------------------------------------------

/// `\x1b[0K` erases from the cursor (inclusive) to end of line.
/// "ABCDEFG", CUP to col 4 (cursor on 'D'), then EL0 → "ABC    ".
#[test]
fn el0_erase_to_end_of_line() {
    erase_test(10, 3, "ABCDEFG\x1b[1;4H\x1b[0K", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(s == "ABC    ", format!("EL0 expected 'ABC    ', got {s:?}"),))
    });
}

/// `\x1b[1K` erases from start of line through the cursor (inclusive).
/// "ABCDEFG", CUP to col 4 (cursor on 'D'), then EL1 → "    EFG".
/// Cursor cell is included in the erased range per the xterm spec.
#[test]
fn el1_erase_to_start_of_line() {
    erase_test(10, 3, "ABCDEFG\x1b[1;4H\x1b[1K", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(s == "    EFG", format!("EL1 expected '    EFG', got {s:?}"),))
    });
}

/// `\x1b[2K` erases the entire current line.
/// "ABCDEFG", CUP to col 4, then EL2 → 7 spaces.
#[test]
fn el2_erase_entire_line() {
    erase_test(10, 3, "ABCDEFG\x1b[1;4H\x1b[2K", |_, lines, commands| {
        let s = lines[0].1.as_string();
        r!(commands.assert(s == "       ", format!("EL2 expected '       ', got {s:?}"),))
    });
}

/// EL must not move the cursor.
#[test]
fn el_preserves_cursor() {
    erase_test(10, 3, "ABCDEFG\x1b[1;4H\x1b[2K", |terminfo, _, commands| {
        r!(commands.assert(
            terminfo.cursor.row == 0 && terminfo.cursor.col == 3,
            format!(
                "EL must preserve cursor at (0, 3), got ({}, {})",
                terminfo.cursor.row, terminfo.cursor.col,
            ),
        ))
    });
}

/// Erased cells take the current SGR background colour (xterm behaviour).
/// Write "ABC", set bg to bright red (`\x1b[101m`), home cursor, EL2 →
/// three space cells whose `style.background` is `basic::RED`.
#[test]
fn el2_uses_current_bg_color() {
    erase_test(
        10,
        3,
        "ABC\x1b[101m\x1b[1;1H\x1b[2K",
        |_, lines, commands| {
            let cells = lines[0].1.cells();
            let red = Color::from(basic::RED);
            r!(commands.assert(
                cells.len() == 3
                    && cells
                        .iter()
                        .all(|c| c.value == ' ' && c.style.background == red),
                format!("EL2 expected 3 red-bg spaces, got {cells:?}"),
            ))
        },
    );
}

// ---------------------------------------------------------------------------
// ED (CSI J) — Erase in Display
// ---------------------------------------------------------------------------

/// `\x1b[0J` erases from the cursor (inclusive) to end of screen.
/// Three lines "AAA", "BBB", "CCC"; CUP to row 2 col 2 (cursor on
/// middle 'B'); ED0 → line 0 unchanged, line 1 "B  ", line 2 blank.
#[test]
fn ed0_erase_to_end_of_screen() {
    erase_test(
        5,
        3,
        "AAA\nBBB\nCCC\x1b[2;2H\x1b[0J",
        |_, lines, commands| {
            let l0 = lines[0].1.as_string();
            let l1 = lines[1].1.as_string();
            let l2 = lines[2].1.as_string();
            r!(commands.assert(l0 == "AAA", format!("ED0 line 0 unchanged, got {l0:?}")));
            r!(commands.assert(
                l1 == "B  ",
                format!("ED0 line 1 expected 'B  ', got {l1:?}"),
            ));
            r!(commands.assert(
                l2.trim().is_empty(),
                format!("ED0 line 2 should be blank, got {l2:?}"),
            ))
        },
    );
}

/// `\x1b[1J` erases from start of screen through the cursor (inclusive).
/// CUP to row 2 col 2; ED1 → line 0 blank, line 1 "  B", line 2 unchanged.
#[test]
fn ed1_erase_to_start_of_screen() {
    erase_test(
        5,
        3,
        "AAA\nBBB\nCCC\x1b[2;2H\x1b[1J",
        |_, lines, commands| {
            let l0 = lines[0].1.as_string();
            let l1 = lines[1].1.as_string();
            let l2 = lines[2].1.as_string();
            r!(commands.assert(
                l0.trim().is_empty(),
                format!("ED1 line 0 should be blank, got {l0:?}"),
            ));
            r!(commands.assert(
                l1 == "  B",
                format!("ED1 line 1 expected '  B', got {l1:?}"),
            ));
            r!(commands.assert(l2 == "CCC", format!("ED1 line 2 unchanged, got {l2:?}")))
        },
    );
}

/// `\x1b[2J` erases the entire viewport.
#[test]
fn ed2_erase_entire_screen() {
    erase_test(
        5,
        3,
        "AAA\nBBB\nCCC\x1b[2;2H\x1b[2J",
        |_, lines, commands| {
            r!(commands.assert(
                lines.iter().all(|(_, l)| l.as_string().trim().is_empty()),
                format!(
                    "ED2 should blank every line, got {:?}",
                    lines.iter().map(|(_, l)| l.as_string()).collect::<Vec<_>>(),
                ),
            ))
        },
    );
}

/// Erased cells across every blanked row carry the current SGR background.
/// Fill three lines, set bg to bright green (`\x1b[102m`), CUP to row 2
/// col 2, then ED2 → every cell in every line is a space on a
/// `basic::LIME` background.
#[test]
fn ed2_uses_current_bg_color() {
    erase_test(
        5,
        3,
        "AAA\nBBB\nCCC\x1b[102m\x1b[2;2H\x1b[2J",
        |_, lines, commands| {
            let green = Color::from(basic::LIME);
            let ok = lines.iter().all(|(_, l)| {
                l.cells()
                    .iter()
                    .all(|c| c.value == ' ' && c.style.background == green)
            });
            r!(commands.assert(
                ok,
                format!(
                    "ED2 expected all-green-bg spaces, got {:?}",
                    lines
                        .iter()
                        .map(|(_, l)| l.cells().to_vec())
                        .collect::<Vec<_>>(),
                ),
            ))
        },
    );
}

/// ED must not move the cursor.
#[test]
fn ed_preserves_cursor() {
    erase_test(
        5,
        3,
        "AAA\nBBB\nCCC\x1b[2;2H\x1b[2J",
        |terminfo, _, commands| {
            r!(commands.assert(
                terminfo.cursor.row == 1 && terminfo.cursor.col == 1,
                format!(
                    "ED must preserve cursor at (1, 1), got ({}, {})",
                    terminfo.cursor.row, terminfo.cursor.col,
                ),
            ))
        },
    );
}
