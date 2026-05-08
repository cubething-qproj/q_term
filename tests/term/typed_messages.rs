use crate::prelude::*;

/// Smoke-test that every typed message stub introduced alongside
/// `TermMsg` is registered on the bus and round-trips through Bevy's
/// default drain without a reader. Construction here also exercises the
/// `Message` derives and ergonomic constructors. No producer or
/// consumer is wired yet; the messages should rotate out via
/// `message_update_system` between frames.
#[test]
fn typed_message_stubs_register_and_drain() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let target = Entity::PLACEHOLDER;
        commands.write_message(TermInputMsg::write(target, "x"));
        commands.write_message(TermInputMsg::writeln(target, "y"));
        commands.write_message(TermInputMsg::write_spans(
            target,
            vec![TermWrite::new("z")],
        ));
        commands.write_message(TermInputMsg::new(target, vec![TermWrite::new("w")]));
        commands.write_message(TermScrollMsg::new(target, -3));
        commands.write_message(TermJumpToBottomMsg::new(target));
        commands.write_message(TermReflowMsg::new(target));
        commands.write_message(TermBufferMutatedMsg::new(target));
        commands.write_message(TermCursorMovedMsg::new(target, VtCursor::new(0, 0)));
        commands.write_message(TermRedrawRequestedMsg::new(target));
        commands.write_message(TermFocusChangedMsg::new(target, true));
    });

    app.add_step(0, |mut commands: Commands| {
        commands.write_message(AppExit::Success);
    });

    assert!(app.run().is_success());
}
