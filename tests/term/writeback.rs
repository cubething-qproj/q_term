//! Tests for reverse-channel ANSI replies on `TermStdIn`.
//!
//! When the parser sees a query sequence on stdout, it must format the
//! reply bytes and emit them on the `TermStdIn` message channel, which
//! corresponds to the application's stdin (see `q_term`'s plumbing
//! docs in `data.rs`). A real shell or loopback adapter consumes those
//! bytes on the other end.
//!
//! Each test feeds a query sequence as `TermStdOut`, then polls for
//! `TermStdIn` messages addressed to the same terminal and asserts on
//! the concatenated reply bytes. Polling (rather than a single-frame
//! read) accommodates the same bootstrap delay that the rest of the
//! test suite tolerates while `VtSize` / `VtViewport` settle.

use crate::prelude::*;

/// Spawn a terminal sized `cols x rows`, feed `input` as stdout, and
/// keep polling each frame until the accumulated bytes received on
/// `TermStdIn` for that terminal equal `expect`. The test runner's
/// timeout backstops a missing reply.
fn writeback_test(cols: usize, rows: usize, input: &'static str, expect: &'static [u8]) {
    let mut app = get_test_app();
    app.add_systems(Startup, move |mut commands: Commands| {
        let TestTerm { term, fg } = spawn_test_term(&mut commands, VtSize { cols, rows });
        commands.write_message(write(term, fg, input));
    });
    app.add_step(
        0,
        move |q_term: Query<TermInfo>,
              mut reader: MessageReader<TermStdIn>,
              mut acc: Local<Vec<u8>>,
              mut commands: Commands| {
            let Ok(terminfo) = q_term.single() else {
                return;
            };
            for msg in reader.read() {
                if msg.term == terminfo.id {
                    acc.extend(msg.message.bytes());
                }
            }
            if acc.len() < expect.len() {
                return; // still waiting for the reply to arrive
            }
            if acc.as_slice() == expect {
                commands.write_message(AppExit::Success);
            } else {
                error!(
                    expected = %String::from_utf8_lossy(expect),
                    actual = %String::from_utf8_lossy(&acc),
                    "writeback bytes did not match",
                );
                commands.write_message(AppExit::error());
            }
        },
    );
    assert!(app.run().is_success());
}

// ---------------------------------------------------------------------------
// DSR — Device Status Report
// ---------------------------------------------------------------------------

/// `CSI 6n` with the cursor at the home position (0,0 internally)
/// should reply with `CSI 1 ; 1 R` — DSR coordinates are 1-indexed on
/// the wire even though our internal cursor is 0-indexed.
#[test]
fn dsr_cursor_at_origin_reports_1_1() {
    writeback_test(20, 5, "\x1b[6n", b"\x1b[1;1R");
}

/// After writing two characters the cursor has advanced to column 2
/// (0-indexed), which DSR reports as column 3.
#[test]
fn dsr_cursor_after_writes_reports_current_position() {
    writeback_test(20, 5, "hi\x1b[6n", b"\x1b[1;3R");
}

/// CUP takes 1-indexed coordinates on the wire; DSR must report them
/// back symmetrically. This pins down the indexing convention for both
/// directions in one assertion.
#[test]
fn dsr_cursor_after_cup_reports_target_position() {
    writeback_test(40, 20, "\x1b[5;10H\x1b[6n", b"\x1b[5;10R");
}

/// `CSI 5n` is the status-report query: terminal replies with a
/// constant `CSI 0 n` ("ready, no malfunction").
#[test]
fn dsr_status_reports_ok() {
    writeback_test(20, 5, "\x1b[5n", b"\x1b[0n");
}

// ---------------------------------------------------------------------------
// DA - Primary Device Attributes
// ---------------------------------------------------------------------------
//
// `CSI c` (equivalently `CSI 0 c`) asks the terminal to identify
// itself. The reply is constrained to numeric VT-family codes (no
// free-form name -- that's XTVERSION's job).
//
// We claim `CSI ? 62 ; 22 c` = VT220 base + ANSI color, which is an
// honest description of what the parser currently implements (SGR
// with palette + 24-bit color, no scrolling regions or alt screen).
//
// Secondary (`CSI > c`) and tertiary (`CSI = c`) DA forms use
// intermediate bytes and are out of scope here.

#[test]
fn da_primary_no_param_replies_vt220_color() {
    writeback_test(20, 5, "\x1b[c", b"\x1b[?62;22c");
}

#[test]
fn da_primary_explicit_zero_replies_vt220_color() {
    writeback_test(20, 5, "\x1b[0c", b"\x1b[?62;22c");
}
