use crate::prelude::*;

/// Holds the live, not-yet-ready terminal spawned in `Startup`.
#[derive(Resource)]
struct Target(Entity);

#[test]
fn closed_entity_does_not_retain_pending_scroll() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn_empty().id();
        commands.insert_resource(Target(target));
        commands.write_message(TermViewportMsg::scroll(target, 5));
    });
    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermViewportMsgs>,
         mut commands: Commands| {
            r!(commands.assert(
                !q_pending.contains(target.0),
                "closed entity retained pending terminal scroll",
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

#[test]
fn viewport_messages_retain_cross_operation_order() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn(Terminal).id();
        commands.insert_resource(Target(target));
        commands.write_message(TermViewportMsg::jump_bottom(target));
        commands.write_message(TermViewportMsg::scroll(target, 5));
        commands.write_message(TermViewportMsg::scroll(target, -5));
    });
    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermViewportMsgs>,
         mut commands: Commands| {
            let Ok(pending) = q_pending.get(target.0) else {
                return;
            };
            let expected = [
                TermViewportMsg::jump_bottom(target.0),
                TermViewportMsg::scroll(target.0, 5),
                TermViewportMsg::scroll(target.0, -5),
            ];
            r!(commands.assert(
                pending.messages().iter().eq(expected.iter()),
                format!("viewport message order changed: {:?}", pending.messages()),
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

#[test]
fn pending_viewport_messages_evict_oldest_at_cap() {
    let mut app = get_test_app();
    app.insert_resource(PendingTermViewportCap(
        std::mem::size_of::<TermViewportMsg>(),
    ));

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn(Terminal).id();
        commands.insert_resource(Target(target));
        commands.write_message(TermViewportMsg::scroll(target, 1));
        commands.write_message(TermViewportMsg::scroll(target, 2));
    });
    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermViewportMsgs>,
         mut commands: Commands| {
            let Ok(pending) = q_pending.get(target.0) else {
                return;
            };
            r!(commands.assert(
                pending.messages().front() == Some(&TermViewportMsg::scroll(target.0, 2)),
                format!(
                    "expected oldest viewport message eviction: {:?}",
                    pending.messages()
                ),
            ));
            r!(commands.assert(
                pending.len_bytes() == std::mem::size_of::<TermViewportMsg>(),
                "pending viewport storage exceeded its cap",
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

#[test]
fn jump_to_bottom_waits_for_terminal_readiness() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn((Terminal, VtScrollPos(5))).id();
        commands.insert_resource(Target(target));
        commands.write_message(TermViewportMsg::jump_bottom(target));
    });
    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermViewportMsgs>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let Ok(pending) = q_pending.get(target.0) else {
                return;
            };
            r!(commands.assert(
                pending.messages().front() == Some(&TermViewportMsg::jump_bottom(target.0)),
                "jump-to-bottom was not retained while pending",
            ));
            commands
                .entity(target.0)
                .insert(VtSize { cols: 80, rows: 24 });
            next.set(Step(1));
        },
    );
    app.add_step(
        1,
        |target: Res<Target>, q_pos: Query<&VtScrollPos>, mut commands: Commands| {
            let Ok(pos) = q_pos.get(target.0) else {
                return;
            };
            if pos.0 == 0 {
                commands.write_message(AppExit::Success);
            }
        },
    );

    assert!(app.run().is_success());
}

#[test]
fn pending_scroll_attach_and_drain() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = commands.spawn(Terminal).id();
        commands.insert_resource(Target(target));
        commands.write_message(TermViewportMsg::scroll(target, 5));
    });

    app.add_step(
        0,
        |target: Res<Target>,
         q_pending: Query<&PendingTermViewportMsgs>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let Ok(pending) = q_pending.get(target.0) else {
                return;
            };
            r!(commands.assert(
                pending.messages().front() == Some(&TermViewportMsg::scroll(target.0, 5)),
                format!("expected pending scroll 5, got {:?}", pending.messages()),
            ));
            commands
                .entity(target.0)
                .insert(VtSize { cols: 80, rows: 24 });
            next.set(Step(1));
        },
    );

    app.add_step(
        1,
        |target: Res<Target>,
         q_pending: Query<&PendingTermViewportMsgs>,
         q_term: Query<TermInfo>,
         mut commands: Commands| {
            if q_pending.contains(target.0) {
                return;
            }
            if !q_term.get(target.0).is_ok_and(|term| term.ready.is_some()) {
                return;
            }
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
