use crate::prelude::*;

/// Payload type used to exercise the generic [`CommandMsg<T>`] bus.
#[derive(Clone, Debug, Reflect)]
struct TestCmd {
    tag: u32,
}

/// Counts how many `CommandMsg<TestCmd>` messages the reader has seen.
#[derive(Resource, Default, Debug)]
struct ReadCount(u32);

/// Marker resource carrying the terminal entity the producer addresses.
#[derive(Resource)]
struct TermTarget(Entity);

fn produce_command(target: Res<TermTarget>, mut commands: Commands, mut sent: Local<bool>) {
    if *sent {
        return;
    }
    *sent = true;
    commands.write_message(CommandMsg::new(target.0, TestCmd { tag: 42 }));
}

fn read_command(
    mut reader: MessageReader<CommandMsg<TestCmd>>,
    mut count: ResMut<ReadCount>,
    mut commands: Commands,
) {
    for msg in reader.read() {
        r!(commands.assert(
            msg.command.tag == 42,
            "CommandMsg payload should round-trip unchanged",
        ));
        count.0 += 1;
    }
}

/// Verifies that a `CommandMsg<T>` written and read inside the same
/// schedule (`TerminalSystems::Input` in `PreUpdate`) is observed by
/// the reader on the same tick. Mirrors the writer→reader seam that
/// `quell` and `shell` rely on.
#[test]
fn command_msg_same_schedule_writer_reader() {
    let mut app = get_test_app();
    register_command_msg::<TestCmd>(&mut app);
    app.init_resource::<ReadCount>();

    app.add_systems(Startup, |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });
        commands.insert_resource(TermTarget(term_id));
    });

    app.add_systems(
        PreUpdate,
        (
            produce_command.in_set(TerminalSystems::Input),
            read_command
                .in_set(TerminalSystems::Input)
                .after(produce_command),
        ),
    );

    app.add_step(
        0,
        |count: Res<ReadCount>, mut commands: Commands| {
            // Producer sends exactly once; reader must observe it the
            // same tick because both systems live in the `Input` set.
            if count.0 == 0 {
                return;
            }
            r!(commands.assert(
                count.0 == 1,
                "Reader should observe exactly one CommandMsg",
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
