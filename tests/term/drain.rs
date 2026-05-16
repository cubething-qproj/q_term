use bevy::ecs::message::Messages;

use crate::prelude::*;

/// Number of `TermBufferMutatedMsg` entries written from `Startup` for the
/// drain test. Three is enough to make a non-rotation regression visible
/// while staying well under any per-buffer growth thresholds.
const SEED_COUNT: u32 = 3;

/// Verifies that `TermBufferMutatedMsg` writes with no registered reader
/// are rotated out by Bevy's default double-buffer drain. Confirms the
/// cutover did not leave behind a manual `messages.clear()` and that the
/// bus relies on Bevy's own update cycle.
///
/// `q_test_harness` initialises `Step(0)` with no auto-transition, so a
/// system gated on `add_step(2, ...)` would deadlock; per the schema's
/// fallback for that case, this is a smoke check that polls every frame
/// for `oldest_message_count` to advance past the seeded batch. The two
/// `message_update_system` rotations that produce that signal are
/// signalled out of `FixedPostUpdate`, so the loop has to run long
/// enough for `Time<Fixed>` to advance - the harness's
/// `TestRunnerTimeout` (2s) bounds the wait.
#[test]
fn term_buffer_mutated_drains_without_reader() {
    let mut app = get_test_app();

    // Note: no Terminal entity is spawned. Otherwise the `VtSize::on_insert`
    // hook would queue a `TermReflowMsg`, which `apply_reflow` consumes and
    // converts into another `TermBufferMutatedMsg` on frame 1 - leaving a
    // stray entry in the buffer past the second rotation and defeating the
    // no-reader assumption.
    app.add_systems(Startup, |mut commands: Commands| {
        for _ in 0..SEED_COUNT {
            commands.write_message(TermBufferMutatedMsg::new(Entity::PLACEHOLDER));
        }
    });

    app.add_step(
        0,
        |messages: Res<Messages<TermBufferMutatedMsg>>, mut commands: Commands| {
            // Once the bus's oldest reachable id moves past the seeded
            // range, `message_update_system` has rotated the buffers
            // enough times to drop the original batch. If this never
            // happens, `TestRunnerTimeout` aborts the run.
            if messages.oldest_message_count() >= SEED_COUNT as usize {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}
