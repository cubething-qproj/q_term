use crate::prelude::*;

/// Holds the bare entity spawned in `Startup` so each step's system can
/// look it up by id.
#[derive(Resource)]
struct Target(Entity);

/// Verifies the pending-scroll retry path end to end:
///
/// 1. A `TermScrollMsg` written against an entity that lacks
///    `Terminal` and `VtSize` causes `apply_scroll` to attach a
///    `PendingTermScroll` carrying the requested delta rather than
///    dropping the message.
/// 2. Once the target gains the prerequisites for `TermInfo` to
///    resolve, `drain_pending` removes the component and re-emits
///    the original message.
///
/// The asserted invariant is drainage rather than the resulting
/// `VtScrollPos`: with a freshly-attached terminal that has no
/// scrollback, the clamp range collapses to `[0, 0]` and the scroll
/// is a no-op regardless of `delta`. The actual scroll-position math
/// is covered elsewhere.
#[test]
fn pending_scroll_attach_and_drain() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn_empty().id();
        commands.insert_resource(Target(target));
        commands.write_message(TermScrollMsg::new(target, 5));
    });

    // Step 0: wait for `apply_scroll` to attach `PendingTermScroll`,
    // assert the queued delta, then make the target resolvable and
    // advance to step 1.
    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermScroll>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let Ok(pending) = q_pending.get(target.0) else {
                return;
            };
            r!(commands.assert(
                pending.delta == 5,
                format!("expected pending delta 5, got {}", pending.delta),
            ));
            commands
                .entity(target.0)
                .insert((Terminal, VtSize { cols: 80, rows: 24 }));
            next.set(Step(1));
        },
    );

    // Step 1: poll until `drain_pending` removes the component, then
    // exit. We do not assert on `VtScrollPos`: a terminal with no
    // scrollback rows clamps any delta to zero, so drainage is the
    // only visible signal that the pending message was consumed.
    app.add_step(
        1,
        |target: Res<Target>,
         q_pending: Query<&PendingTermScroll>,
         q_term: Query<TermInfo>,
         mut commands: Commands| {
            if q_pending.contains(target.0) {
                return;
            }
            // Ensure the target itself resolved as a terminal before
            // declaring drainage successful; otherwise an early exit
            // could mask the component never having been there.
            if q_term.get(target.0).is_err() {
                return;
            }
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
