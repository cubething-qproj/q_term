use crate::prelude::*;
use bevy::color::palettes::{basic, css};

/// Spawn a terminal, write `input`, then run `check` on the first step.
/// `check` receives terminfo and the collected lines.
fn ansi_test(
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
// Cursor movement defaults (regression tests for bugs #3, A)
// ---------------------------------------------------------------------------

/// `\x1b[A` with omitted param should move cursor up 1, not 0.
#[test]
fn cursor_up_default() {
    ansi_test(80, 24, "A\n\x1b[A", |terminfo, _, commands| {
        r!(commands.assert(terminfo.cursor.row == 0, "CUU default should move up 1"))
    });
}

/// `\x1b[B` with omitted param should move cursor down 1, not 0.
#[test]
fn cursor_down_default() {
    ansi_test(80, 24, "A\x1b[B", |terminfo, _, commands| {
        r!(commands.assert(terminfo.cursor.row == 1, "CUD default should move down 1"))
    });
}

/// `\x1b[C` with omitted param should move cursor right 1, not 0.
#[test]
fn cursor_forward_default() {
    ansi_test(80, 24, "A\x1b[C", |terminfo, _, commands| {
        r!(commands.assert(
            terminfo.cursor.col == 2,
            "CUF default should move right 1 (start at 1 after 'A')",
        ))
    });
}

/// `\x1b[D` with omitted param should move cursor left 1, not 0.
#[test]
fn cursor_back_default() {
    ansi_test(80, 24, "AB\x1b[D", |terminfo, _, commands| {
        r!(commands.assert(
            terminfo.cursor.col == 1,
            "CUB default should move left 1 (start at 2 after 'AB')",
        ))
    });
}

/// `\x1b[E` (CNL) with omitted param should move cursor down 1 and to col 0.
#[test]
fn cnl_default() {
    ansi_test(80, 24, "ABC\x1b[E", |terminfo, _, commands| {
        r!(commands.assert(terminfo.cursor.row == 1, "CNL default should move down 1"));
        r!(commands.assert(terminfo.cursor.col == 0, "CNL should reset col to 0"))
    });
}

/// `\x1b[F` (CPL) with omitted param should move cursor up 1 and to col 0.
#[test]
fn cpl_default() {
    ansi_test(80, 24, "A\nBCD\x1b[F", |terminfo, _, commands| {
        r!(commands.assert(terminfo.cursor.row == 0, "CPL default should move up 1"));
        r!(commands.assert(terminfo.cursor.col == 0, "CPL should reset col to 0"))
    });
}

// ---------------------------------------------------------------------------
// Cursor clamping / wrapping (regression tests for bugs #7, B)
// ---------------------------------------------------------------------------

/// CUF at the last column should not advance past `cols - 1`.
#[test]
fn cursor_forward_clamp() {
    // 10 columns; write 9 chars ('A' at col 0..8, cursor at col 9), then CUF 5
    ansi_test(10, 24, "AAAAAAAAA\x1b[5C", |terminfo, _, commands| {
        commands.assert(
            terminfo.cursor.col == 9,
            "CUF should clamp to last column (cols-1 = 9)",
        )
    });
}

/// CUB at col 0 should wrap to the previous line's last column.
#[test]
fn cursor_back_wrap_not_past_end() {
    // After "A", cursor is at col 1. CUB 1 (wrap=false) should clamp at col 0.
    ansi_test(10, 24, "A\x1b[D", |terminfo, _, commands| {
        commands.assert(
            terminfo.cursor.col == 0,
            "CUB at col 0 without wrap should clamp to 0",
        )
    });
}

// ---------------------------------------------------------------------------
// SGR colors (regression tests for bug #4 + coverage)
// ---------------------------------------------------------------------------

/// SGR 40–47 and 100–107 set background palette colors correctly.
#[test]
fn palette_background() {
    let mut app = get_test_app();

    let expected: [(Srgba, Srgba); 8] = [
        (basic::BLACK, basic::GRAY),
        (css::DARK_RED, basic::RED),
        (basic::GREEN, basic::LIME),
        (basic::OLIVE, basic::YELLOW),
        (basic::NAVY, basic::BLUE),
        (basic::PURPLE, basic::FUCHSIA),
        (basic::TEAL, css::AQUA),
        (basic::SILVER, basic::WHITE),
    ];

    // Build: \x1b[40mA\x1b[41mB...\x1b[47mH\x1b[100mI...\x1b[107mP\x1b[0m!
    let mut input = String::new();
    for i in 0..8u8 {
        input.push_str(&format!("\x1b[{}m{}", 40 + i, (b'A' + i) as char));
    }
    for i in 0..8u8 {
        input.push_str(&format!("\x1b[{}m{}", 100 + i, (b'I' + i) as char));
    }
    input.push_str("\x1b[0m!");

    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });
        commands.write_message(StdOut::write(term_id, &input));
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

            let (_, line) = &lines[0];
            let cells = line.cells();
            r!(commands.assert(cells.len() == 17, "Expected 17 cells"));

            for i in 0..8usize {
                let cell = &cells[i];
                let (expected_dark, _) = expected[i];
                r!(commands.assert(
                    cell.style.background == Color::from(expected_dark),
                    format!(
                        "Standard bg color {} (ESC[{}m) mismatch: {:?} != {:?}",
                        i,
                        40 + i,
                        cell.style.background,
                        Color::from(expected_dark)
                    )
                ));
            }

            for i in 0..8usize {
                let cell = &cells[8 + i];
                let (_, expected_bright) = expected[i];
                r!(commands.assert(
                    cell.style.background == Color::from(expected_bright),
                    format!(
                        "Bright bg color {} (ESC[{}m) mismatch: {:?} != {:?}",
                        i,
                        100 + i,
                        cell.style.background,
                        Color::from(expected_bright)
                    )
                ));
            }

            // '!' — after reset
            if cells[16].value != '!' || cells[16].style != VtCellStyle::default() {
                r!(commands.assert(
                    cells[16].style == VtCellStyle::default(),
                    "Final '!' should have default style"
                ));
            }

            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

/// SGR 39 resets foreground to default; SGR 49 resets background to default.
/// Code 48 must NOT be caught as a palette color (regression for bug #4).
#[test]
fn sgr_default_fg_bg() {
    // Set fg to red (31), then reset fg (39) → should be default fg.
    // Set bg to green (42), then reset bg (49) → should be default bg.
    ansi_test(
        80,
        24,
        "\x1b[31;42mX\x1b[39mY\x1b[49mZ",
        |_, lines, commands| {
            let (_, line) = &lines[0];
            let cells = line.cells();
            r!(commands.assert(
                cells.len() == 3,
                format!("Expected 3 cells, got {}", cells.len()),
            ));

            // X: red fg, green bg
            r!(commands.assert(
                cells[0].style.color == Color::from(css::DARK_RED),
                "X fg should be dark red",
            ));
            r!(commands.assert(
                cells[0].style.background == Color::from(basic::GREEN),
                "X bg should be green",
            ));

            // Y: default fg (39 resets), still green bg
            r!(commands.assert(
                cells[1].style.color == VtCellStyle::default().color,
                "Y fg should be default after SGR 39",
            ));
            r!(commands.assert(
                cells[1].style.background == Color::from(basic::GREEN),
                "Y bg should still be green",
            ));

            // Z: default fg, default bg (49 resets)
            r!(commands.assert(
                cells[2].style.color == VtCellStyle::default().color,
                "Z fg should be default",
            ));
            r!(commands.assert(
                cells[2].style.background == VtCellStyle::default().background,
                "Z bg should be default after SGR 49",
            ))
        },
    );
}

/// Multiple SGR params in one sequence (e.g., `\x1b[31;42m`).
#[test]
fn sgr_combined_styles() {
    ansi_test(80, 24, "\x1b[31;42mA\x1b[0mB", |_, lines, commands| {
        let (_, line) = &lines[0];
        let cells = line.cells();
        r!(commands.assert(
            cells.len() == 2,
            format!("Expected 2 cells, got {}", cells.len()),
        ));

        // 'A': red fg + green bg
        r!(commands.assert(
            cells[0].style.color == Color::from(css::DARK_RED),
            "A fg should be dark red",
        ));
        r!(commands.assert(
            cells[0].style.background == Color::from(basic::GREEN),
            "A bg should be green",
        ));

        // 'B': reset
        r!(commands.assert(
            cells[1].style == VtCellStyle::default(),
            "B should have default style after reset",
        ))
    });
}

/// `\x1b[0m` resets all styles back to default.
#[test]
fn sgr_reset() {
    ansi_test(
        80,
        24,
        "\x1b[38;2;100;200;50;48;2;10;20;30mX\x1b[0mY",
        |_, lines, commands| {
            let (_, line) = &lines[0];
            let cells = line.cells();
            r!(commands.assert(
                cells.len() == 2,
                format!("Expected 2 cells, got {}", cells.len()),
            ));

            // X: custom colors
            r!(commands.assert(
                cells[0].style.color == Color::srgb_u8(100, 200, 50),
                "X fg should be srgb(100,200,50)",
            ));
            r!(commands.assert(
                cells[0].style.background == Color::srgb_u8(10, 20, 30),
                "X bg should be srgb(10,20,30)",
            ));

            // Y: default after reset
            r!(commands.assert(
                cells[1].style == VtCellStyle::default(),
                "Y should have default style after SGR 0",
            ))
        },
    );
}

// ---------------------------------------------------------------------------
// Control codes and wrapping
// ---------------------------------------------------------------------------

/// LF creates a new line and moves cursor down; CR resets col to 0.
#[test]
fn line_feed_and_carriage_return() {
    ansi_test(80, 24, "ABC\nDEF\rG", |terminfo, lines, commands| {
        r!(commands.assert(
            lines.len() == 2,
            format!("LF should create a second line, got {}", lines.len()),
        ));

        let (_, line0) = &lines[0];
        r!(commands.assert(
            line0.as_string() == "ABC",
            format!("line 0 should be 'ABC', got '{}'", line0.as_string()),
        ));

        // After "DEF\rG": CR resets col, 'G' overwrites 'D'
        let (_, line1) = &lines[1];
        r!(commands.assert(
            line1.as_string() == "GEF",
            format!(
                "line 1 should be 'GEF' (CR+overwrite), got '{}'",
                line1.as_string()
            ),
        ));

        r!(commands.assert(
            terminfo.cursor.row == 1,
            format!("cursor row should be 1, got {}", terminfo.cursor.row),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 1,
            format!("cursor col should be 1, got {}", terminfo.cursor.col),
        ))
    });
}

/// Writing past `cols` wraps to the next line.
#[test]
fn line_wrapping() {
    // 5-column terminal, write "ABCDEFGH" (8 chars).
    // Wrapping creates a new logical line after column 5.
    ansi_test(5, 24, "ABCDEFGH", |terminfo, lines, commands| {
        r!(commands.assert(
            lines.len() == 2,
            format!("wrapping should create 2 lines, got {}", lines.len()),
        ));

        let (_, line0) = &lines[0];
        r!(commands.assert(
            line0.as_string() == "ABCDE",
            format!("line 0 should be 'ABCDE', got '{}'", line0.as_string()),
        ));

        let (_, line1) = &lines[1];
        r!(commands.assert(
            line1.as_string() == "FGH",
            format!("line 1 should be 'FGH', got '{}'", line1.as_string()),
        ));

        r!(commands.assert(
            terminfo.cursor.row == 1,
            format!(
                "cursor row after wrapping should be 1, got {}",
                terminfo.cursor.row
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 3,
            format!(
                "cursor col after 'H' should be 3, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}

/// Verifies that 24-bit ANSI color escapes (SGR 38;2;r;g;b for foreground,
/// 48;2;r;g;b for background) produce VtCells with the correct styles.
///
/// Input: "\x1b[38;2;255;0;128;48;2;0;64;255mHi\x1b[0m!"
///   - "Hi" should be fg=(255,0,128) bg=(0,64,255)
///   - "!" should be default style (reset via SGR 0)
#[test]
fn truecolor() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });
        commands.write_message(StdOut::write(
            term_id,
            "\x1b[38;2;255;0;128;48;2;0;64;255mHi\x1b[0m!",
        ));
    });

    app.add_step(
        0,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            let terminfo = r!(q_term.single());
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }

            let (_, line) = &lines[0];

            r!(commands.assert(
                line.as_string() == "Hi!",
                format!("Expected 'Hi!', got '{}'", line.as_string()),
            ));

            let cells = line.cells();
            r!(commands.assert(
                cells.len() == 3,
                format!("Expected 3 cells, got {}", cells.len()),
            ));

            let colored_style = VtCellStyle {
                color: Color::srgb_u8(255, 0, 128),
                background: Color::srgb_u8(0, 64, 255),
            };
            // 'H'
            r!(commands.assert(
                cells[0].value == 'H' && cells[0].style == colored_style,
                "cell[0] should be 'H' with colored style",
            ));
            // 'i'
            r!(commands.assert(
                cells[1].value == 'i' && cells[1].style == colored_style,
                "cell[1] should be 'i' with colored style",
            ));
            // '!' — after SGR reset
            r!(commands.assert(
                cells[2].value == '!' && cells[2].style == VtCellStyle::default(),
                "cell[2] should be '!' with default style",
            ));

            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

/// Verifies standard (30–37) and bright (90–97) ANSI palette foreground
/// colors, plus SGR 0 reset.
///
/// Tests all 8 standard + 8 bright foreground colors in a single line.
#[test]
fn palette_foreground() {
    let mut app = get_test_app();

    // Standard ANSI palette: (standard, bright)
    let expected: [(Srgba, Srgba); 8] = [
        (basic::BLACK, basic::GRAY),     // 0: black
        (css::DARK_RED, basic::RED),     // 1: red
        (basic::GREEN, basic::LIME),     // 2: green
        (basic::OLIVE, basic::YELLOW),   // 3: yellow
        (basic::NAVY, basic::BLUE),      // 4: blue
        (basic::PURPLE, basic::FUCHSIA), // 5: magenta
        (basic::TEAL, css::AQUA),        // 6: cyan
        (basic::SILVER, basic::WHITE),   // 7: white
    ];

    // Build input: \x1b[30mA\x1b[31mB...\x1b[37mH\x1b[90mI...\x1b[97mP\x1b[0mQ
    let mut input = String::new();
    for i in 0..8u8 {
        input.push_str(&format!("\x1b[{}m{}", 30 + i, (b'A' + i) as char));
    }
    for i in 0..8u8 {
        input.push_str(&format!("\x1b[{}m{}", 90 + i, (b'I' + i) as char));
    }
    input.push_str("\x1b[0m!");

    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });
        commands.write_message(StdOut::write(term_id, &input));
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

            let (_, line) = &lines[0];
            let cells = line.cells();
            // 8 standard + 8 bright + 1 reset char = 17
            r!(commands.assert(
                cells.len() == 17,
                format!("Expected 17 cells, got {}", cells.len())
            ));

            // Check standard colors (30–37)
            for i in 0..8usize {
                let cell = &cells[i];
                let (expected_dark, _) = expected[i];
                r!(commands.assert(
                    cell.style.color == Color::from(expected_dark),
                    format!("Standard fg color {} (ESC[{}m) mismatch", i, 30 + i)
                ));
                r!(commands.assert(
                    cell.style.background == VtCellStyle::default().background,
                    format!("Standard bg color {} (ESC[{}m) mismatch", i, 40 + i)
                ));
            }

            // Check bright colors (90–97)
            for i in 0..8usize {
                let cell = &cells[8 + i];
                let (_, expected_bright) = expected[i];
                r!(commands.assert(
                    cell.style.color == Color::from(expected_bright),
                    format!("Bright fg color {} (ESC[{}m) mismatch", i, 90 + i)
                ));
            }

            // '!' — after reset
            r!(commands.assert(
                cells[16].value == '!',
                format!("expected '!', got '{}'", cells[16].value)
            ));
            r!(commands.assert(
                cells[16].style == VtCellStyle::default(),
                "expected default style after reset"
            ));

            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

/// Verifies that CUP (cursor absolute position, `ESC[{row};{col}H`) can place
/// characters at arbitrary, non-sequential positions in the grid, and that a
/// later write overwrites earlier content at the same position.
#[test]
fn cursor_write_arbitrary_positions() {
    let mut app = get_test_app();

    // CUP is 1-indexed: ESC[row;colH
    // Write 'A' at (0,0), 'B' at (5,10), 'C' at (0,0) to overwrite 'A'.
    let input = concat!(
        "\x1b[1;1H",
        "A", // row 0, col 0
        "\x1b[6;11H",
        "B", // row 5, col 10
        "\x1b[1;1H",
        "C",         // row 0, col 0 — overwrites 'A'
        "\x1b[3;8H", // move cursor to row 2, col 7 (no write)
    );

    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 40, rows: 24 });
        commands.write_message(StdOut::write(term_id, input));
    });

    app.add_step(
        0,
        move |q_term: Query<TermInfo>,
              q_lines: Query<(Entity, &VtLine)>,
              mut commands: Commands| {
            let terminfo = q_term.single();
            if let Err(e) = terminfo {
                error!(?e);
                commands.write_message(AppExit::error());
                return;
            }
            let terminfo = terminfo.unwrap();
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }

            let cell_at = |row: usize, col: usize| -> Option<VtCell> {
                lines
                    .get(row)
                    .and_then(|(_, line)| line.cells().get(col).copied())
            };

            // 'C' overwrote 'A' at (0, 0)
            let c = cell_at(0, 0).expect("cell at (0,0)");
            r!(commands.assert(
                c.value == 'C',
                format!("overwrite: expected 'C', got '{}'", c.value)
            ));

            // 'B' at (5, 10)
            let c = cell_at(5, 10).expect("cell at (5,10)");
            r!(commands.assert(c.value == 'B', format!("expected 'B', got '{}'", c.value)));

            // Final cursor position: CUP(3,8) → row 2, col 7
            r!(commands.assert(
                terminfo.cursor.row == 2,
                format!("final cursor row: expected 2, got {}", terminfo.cursor.row)
            ));
            r!(commands.assert(
                terminfo.cursor.col == 7,
                format!("final cursor col: expected 7, got {}", terminfo.cursor.col)
            ));

            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

// ---------------------------------------------------------------------------
// BS (0x08) and HT (0x09) — issue #21
// ---------------------------------------------------------------------------

/// Spawn a terminal with an explicit `VtTabStop` width, write `input`, then
/// run `check` on the first step.
fn ansi_test_with_tabstop(
    cols: usize,
    rows: usize,
    tabstop: usize,
    input: &'static str,
    check: impl Fn(TermInfoItem, &[(Entity, &VtLine)], &mut Commands) -> bool + Send + Sync + 'static,
) {
    let mut app = get_test_app();
    app.add_systems(Startup, move |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols, rows })
            .insert(VtTabStop(tabstop));
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

/// BS moves the cursor left by one column. It is non-destructive: the cell
/// under the previous cursor position is not erased.
#[test]
fn backspace_moves_cursor_left() {
    ansi_test(80, 24, "AB\x08", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        r!(commands.assert(
            line.as_string() == "AB",
            format!(
                "BS should not erase; line should be 'AB', got '{}'",
                line.as_string()
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 1,
            format!(
                "BS should move cursor to col 1, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}

/// BS is non-destructive; a subsequent print overwrites the cell at the new
/// cursor position.
#[test]
fn backspace_then_print_overwrites() {
    ansi_test(80, 24, "AB\x08C", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        r!(commands.assert(
            line.as_string() == "AC",
            format!("expected 'AC' (BS + overwrite), got '{}'", line.as_string()),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 2,
            format!(
                "cursor col after overwrite should be 2, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}

/// BS at column 0 clamps; it does not wrap to the previous line.
#[test]
fn backspace_at_col_zero_clamps() {
    // "A\x08\x08": after 'A' cursor is at col 1; first BS → col 0; second
    // BS must clamp (no underflow, no row wrap).
    ansi_test(80, 24, "A\x08\x08", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        r!(commands.assert(
            line.as_string() == "A",
            format!("BS should not erase 'A'; got '{}'", line.as_string()),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 0,
            format!("BS at col 0 should clamp to 0, got {}", terminfo.cursor.col),
        ));
        r!(commands.assert(
            terminfo.cursor.row == 0,
            format!(
                "BS at col 0 should stay on row 0, got {}",
                terminfo.cursor.row
            ),
        ))
    });
}

/// BS clears the pending-wrap latch. After filling the row, the next print
/// would normally wrap to a new line; a BS before that print must cancel the
/// wrap and overwrite the last cell of the same row.
#[test]
fn backspace_clears_pending_wrap() {
    ansi_test(5, 24, "ABCDE\x08X", |terminfo, lines, commands| {
        r!(commands.assert(
            lines.len() == 1,
            format!(
                "BS should cancel pending wrap; expected 1 line, got {}",
                lines.len()
            ),
        ));
        let (_, line) = &lines[0];
        r!(commands.assert(
            line.as_string() == "ABCDX",
            format!("expected 'ABCDX', got '{}'", line.as_string()),
        ));
        r!(commands.assert(
            terminfo.cursor.row == 0,
            format!("cursor should stay on row 0, got {}", terminfo.cursor.row),
        ))
    });
}

/// HT from column 0 advances to the next 8-column tab stop (col 8).
#[test]
fn tab_from_col_zero() {
    ansi_test(80, 24, "\tA", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        let cells = line.cells();
        r!(commands.assert(
            cells.get(8).map(|c| c.value) == Some('A'),
            format!(
                "HT should land 'A' at col 8, got line '{}'",
                line.as_string()
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 9,
            format!(
                "cursor col after 'A' should be 9, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}

/// HT from a non-aligned column advances to the next 8-column tab stop.
#[test]
fn tab_advances_to_next_stop() {
    ansi_test(80, 24, "ABC\tX", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        let cells = line.cells();
        r!(commands.assert(
            cells.get(8).map(|c| c.value) == Some('X'),
            format!(
                "HT should land 'X' at col 8, got line '{}'",
                line.as_string()
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 9,
            format!(
                "cursor col after 'X' should be 9, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}

/// HT from a column already on a tab stop advances to the *next* stop.
#[test]
fn tab_already_on_stop_advances() {
    ansi_test(80, 24, "01234567\tX", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        let cells = line.cells();
        r!(commands.assert(
            cells.get(16).map(|c| c.value) == Some('X'),
            format!(
                "HT on stop should land 'X' at col 16, got line '{}'",
                line.as_string()
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 17,
            format!(
                "cursor col after 'X' should be 17, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}

/// HT clamps to the last column when the next stop would exceed `cols - 1`.
#[test]
fn tab_clamps_at_last_column() {
    // 10-col term; after "012345678" cursor is at col 9 (last column).
    // HT must not advance past col 9; the following 'X' overwrites col 9.
    ansi_test(10, 24, "012345678\tX", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        let cells = line.cells();
        r!(commands.assert(
            cells.get(9).map(|c| c.value) == Some('X'),
            format!(
                "HT clamp: 'X' should be at col 9, got line '{}'",
                line.as_string()
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.row == 0,
            format!("cursor row should stay 0, got {}", terminfo.cursor.row),
        ))
    });
}

/// HT honors a configurable `VtTabStop` width (here, 4).
#[test]
fn tab_respects_configurable_width() {
    ansi_test_with_tabstop(80, 24, 4, "A\tB", |terminfo, lines, commands| {
        let (_, line) = &lines[0];
        let cells = line.cells();
        r!(commands.assert(
            cells.get(4).map(|c| c.value) == Some('B'),
            format!(
                "with tabstop=4, 'B' should be at col 4, got line '{}'",
                line.as_string()
            ),
        ));
        r!(commands.assert(
            terminfo.cursor.col == 5,
            format!(
                "cursor col after 'B' should be 5, got {}",
                terminfo.cursor.col
            ),
        ))
    });
}
