use crate::prelude::*;

/// Holds the bare entity spawned in `Startup` so each step's system can
/// look it up by id.
#[derive(Resource)]
struct Target(Entity);

/// Verifies the pending-input retry path end to end:
///
/// 1. A `TermStdOut` written against an entity that lacks
///    `Terminal` and `VtSize` causes `process_input` to attach a
///    `PendingTermInput` rather than dropping the writes.
/// 2. Once the target gains the prerequisites for `TermInfo` to
///    resolve, `drain_pending` removes the component and re-emits
///    the original message; `process_input` then applies the writes
///    so the buffer reflects them.
#[test]
fn pending_input_attach_and_drain() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn_empty().id();
        commands.insert_resource(Target(target));
        commands.write_message(TermStdOut::write(target, "Hello, world!"));
    });

    // Step 0: wait for `process_input` to attach `PendingTermInput`,
    // assert its contents, then make the target resolvable and advance
    // to step 1.
    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermInput>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let Ok(pending) = q_pending.get(target.0) else {
                return;
            };
            let queued: String = pending.writes.iter().map(|w| w.text.clone()).collect();
            r!(commands.assert(
                queued == "Hello, world!",
                format!("expected pending text \"Hello, world!\", got {queued:?}"),
            ));
            commands
                .entity(target.0)
                .insert((Terminal, VtSize { cols: 80, rows: 24 }));
            next.set(Step(1));
        },
    );

    // Step 1: poll until `drain_pending` removes the component and the
    // re-emitted writes have been applied to the buffer; then exit.
    app.add_step(
        1,
        |target: Res<Target>,
         q_pending: Query<&PendingTermInput>,
         q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         mut commands: Commands| {
            if q_pending.contains(target.0) {
                return;
            }
            let Ok(terminfo) = q_term.get(target.0) else {
                return;
            };
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }
            r!(commands.assert(
                lines.len() == 1,
                format!("expected 1 line, got {}", lines.len()),
            ));
            let (_, line) = &lines[0];
            r!(commands.assert(
                line.as_string() == "Hello, world!",
                format!("expected \"Hello, world!\", got {:?}", line.as_string()),
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
