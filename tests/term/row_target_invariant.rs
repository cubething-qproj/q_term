//! Regression test for the `q_term-row-target-bail` defect.
//!
//! Mimics the `examples/minimal.rs` lifecycle: spawn a [`Terminal`]
//! with the default (zero-sized) [`VtSize`] inserted by `#[require]`,
//! queue writes during `Startup`, then a couple of frames later
//! insert a real `VtSize` to simulate what `resize` does once the UI
//! layout has settled.
//!
//! Asserts the invariant that every entity reachable through
//! `VtLineTarget` resolves through `Query<&VtRowTarget, With<VtLine>>`
//! -- the precondition the `r!()` bail at
//! `q_term/active/src/data.rs:40` is supposed to guarantee. The
//! assertion runs every frame from frame 2 onward, and the test only
//! exits success once it has observed `REQUIRED_CLEAN_FRAMES`
//! consecutive clean frames. Any frame with a stale entry triggers
//! `AppExit::error` immediately.
//!
//! Two paths are known to violate the invariant:
//!
//! - `apply_reflow` despawns every `VtRow` of every `VtLine`,
//!   which trips Bevy's relationship `on_replace` hook
//!   (`bevy_ecs::relationship::Relationship::on_replace`): when the
//!   row collection becomes empty, the `VtRowTarget` component is
//!   removed from the line entity. Originally `flow_line` never
//!   re-spawned a row for zero-cell lines. Closed by `449bd1a`.
//! - `apply_reflow` despawns every `VtRow` *before* its early-exit
//!   check for `VtSize == 0x0`. The first reflow message after
//!   `Terminal` spawn fires with size `0x0` (the default `VtSize`
//!   inserted by `#[require]`), wipes every row that `process_input`
//!   just spawned, and leaves every `VtLine` without a `VtRowTarget`.

use crate::prelude::*;

const LONG_LINE: &str = "This is a really long line! It should be wrapping. Just checking :) How are you doing today? I'm doing pretty good myself.\n";

const FIRST_ASSERT_FRAME: u32 = 2;
const SET_VTSIZE_FRAME: u32 = 4;
const REQUIRED_CLEAN_FRAMES: u32 = 6;
const TIMEOUT_FRAME: u32 = 30;

#[derive(Resource, Default)]
struct ActiveTerm(Option<Entity>);

#[test]
fn vt_linetarget_entries_resolve_through_row_target_query() {
    let mut app = get_test_app();

    app.init_resource::<ActiveTerm>();

    app.add_systems(
        Startup,
        |mut commands: Commands, mut active: ResMut<ActiveTerm>| {
            // Mirror minimal.rs: do NOT insert a real VtSize here.
            // The default 0x0 from `#[require(VtSize)]` is what fires
            // the first TermReflowMsg and exposes the second bug
            // path.
            let term_id = commands.spawn(Terminal).id();
            active.0 = Some(term_id);

            for i in 0..20 {
                commands.write_message(StdOut::writeln(term_id, format!("{i}")));
            }
            commands.write_message(StdOut::write(
                term_id,
                "hello\nhere are multiple lines\n",
            ));
            commands.write_message(StdOut::write(
                term_id,
                "\x1b[31mthis is red text \x1b[47mwith a white background!\n",
            ));
            commands.write_message(StdOut::write(term_id, "still red and white...\n"));
            commands.write_message(StdOut::write(term_id, "\x1b[0mbut no longer :)\n"));
            commands.write_message(StdOut::write_spans(
                term_id,
                vec![
                    TermWrite::new("you can do multiple spans too, "),
                    TermWrite::new("with style\n"),
                ],
            ));
            commands.write_message(StdOut::writeln(term_id, LONG_LINE));
        },
    );

    app.add_step(
        0,
        |mut clean: Local<u32>,
         active: Res<ActiveTerm>,
         q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
         frames: Res<bevy::diagnostic::FrameCount>,
         mut commands: Commands| {
            let frame = frames.0;

            // Mimic `resize`: insert a real VtSize a few frames in,
            // simulating what happens once Bevy's layout pass finishes
            // measuring the VtUi node.
            if frame == SET_VTSIZE_FRAME
                && let Some(term) = active.0
            {
                commands.entity(term).insert(VtSize { cols: 80, rows: 24 });
            }

            let terminfo = r!(q_term.single());
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();

            if frame >= FIRST_ASSERT_FRAME && !lines.is_empty() {
                let stale: Vec<Entity> = terminfo
                    .line_target
                    .iter()
                    .filter(|line_id| q_rowtargets.get(*line_id).is_err())
                    .collect();
                if !commands.assert(
                    stale.is_empty(),
                    format!(
                        "frame {frame}: VtLineTarget contains {} entries with VtLine but no \
                         VtRowTarget: {stale:?}",
                        stale.len(),
                    ),
                ) {
                    return;
                }
                *clean += 1;
            }

            if *clean >= REQUIRED_CLEAN_FRAMES {
                commands.write_message(AppExit::Success);
                return;
            }

            if frame > TIMEOUT_FRAME {
                commands.assert(
                    false,
                    format!(
                        "frame {frame}: never observed {REQUIRED_CLEAN_FRAMES} consecutive clean \
                         frames (got {})",
                        *clean,
                    ),
                );
            }
        },
    );

    assert!(app.run().is_success());
}
