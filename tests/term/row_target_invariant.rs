//! Regression test for the `q_term-row-target-bail` defect.
//!
//! Mimics the `examples/minimal.rs` write pattern (many `writeln` plus
//! ANSI plus a long wrapping line, all queued from `Startup`) and
//! asserts the invariant that every entity reachable through
//! `VtLineTarget` resolves through `Query<&VtRowTarget, With<VtLine>>` --
//! i.e. the precondition the `r!()` bail at
//! `q_term/active/src/data.rs:40` is supposed to guarantee.
//!
//! Prior to the fix this fires because trailing empty `VtLine` entities
//! end up with `VtRowTarget` removed: `apply_reflow` despawns the last
//! `VtRow` (Bevy's relationship hook then auto-removes the now-empty
//! `VtRowTarget` per `bevy_ecs::relationship::Relationship::on_replace`),
//! and `flow_line` does not spawn a replacement row for a zero-cell
//! line. The fix ensures every `VtLine` retains at least one `VtRow`.

use crate::prelude::*;

const LONG_LINE: &str = "This is a really long line! It should be wrapping. Just checking :) How are you doing today? I'm doing pretty good myself.\n";

#[test]
fn vt_linetarget_entries_resolve_through_row_target_query() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });

        for i in 0..20 {
            commands.write_message(TermInputMsg::writeln(term_id, format!("{i}")));
        }
        commands.write_message(TermInputMsg::write(
            term_id,
            "hello\nhere are multiple lines\n",
        ));
        commands.write_message(TermInputMsg::write(
            term_id,
            "\x1b[31mthis is red text \x1b[47mwith a white background!\n",
        ));
        commands.write_message(TermInputMsg::write(term_id, "still red and white...\n"));
        commands.write_message(TermInputMsg::write(term_id, "\x1b[0mbut no longer :)\n"));
        commands.write_message(TermInputMsg::write_spans(
            term_id,
            vec![
                TermWrite::new("you can do multiple spans too, "),
                TermWrite::new("with style\n"),
            ],
        ));
        commands.write_message(TermInputMsg::writeln(term_id, LONG_LINE));
    });

    app.add_step(
        0,
        |q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
         q_rows: Query<(Entity, &VtRow)>,
         frames: Res<bevy::diagnostic::FrameCount>,
         mut commands: Commands| {
            let terminfo = r!(q_term.single());
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }
            // Wait a few frames so reflow / scroll churn has settled.
            if frames.0 < 4 {
                return;
            }

            let stale: Vec<Entity> = terminfo
                .line_target
                .iter()
                .filter(|line_id| q_rowtargets.get(*line_id).is_err())
                .collect();
            r!(commands.assert(
                stale.is_empty(),
                format!(
                    "VtLineTarget contains {} entries with VtLine but no VtRowTarget: {stale:?}",
                    stale.len(),
                ),
            ));

            // And `terminfo.rows()` should return one row per line at
            // cols=80 (none of the lines exceed 80 chars even after
            // padding).
            let rows: Vec<_> = terminfo.rows(&q_rowtargets, &q_rows).collect();
            r!(commands.assert(
                rows.len() >= lines.len(),
                format!(
                    "terminfo.rows() returned {} rows for {} lines",
                    rows.len(),
                    lines.len()
                ),
            ));

            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
