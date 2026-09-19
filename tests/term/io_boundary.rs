use bevy::color::palettes::{basic, css};

use crate::prelude::*;

#[derive(Resource, Clone, Copy)]
struct Term(Entity);

fn text(q_term: &Query<TermInfo>, q_lines: &Query<(Entity, &VtLine)>) -> Option<String> {
    let term = q_term.single().ok()?;
    Some(
        term.lines(q_lines)
            .map(|(_, line)| line.as_string())
            .collect(),
    )
}

#[test]
fn input_set_writes_are_processed_in_the_same_update() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.insert_resource(Term(term));
    });
    app.add_systems(
        Update,
        (|term: Res<Term>, mut commands: Commands, mut sent: Local<bool>| {
            if !*sent {
                commands.write_message(VtWriteMsg::new(term.0, b"ordered".to_vec()));
                *sent = true;
            }
        })
        .in_set(TerminalSystems::Input),
    );
    app.add_step(
        0,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            if text(&q_term, &q_lines).as_deref() == Some("ordered") {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn direct_bytes_render_without_a_foreground_peer() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.write_message(VtWriteMsg::new(term, b"hello".to_vec()));
    });
    app.add_step(
        0,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            if text(&q_term, &q_lines).as_deref() == Some("hello") {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn parser_persists_partial_escape_sequences_across_frames() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.insert_resource(Term(term));
        commands.write_message(VtWriteMsg::new(term, b"\x1b[38;2;255;0;".to_vec()));
    });
    app.add_step(
        0,
        |term: Res<Term>, mut commands: Commands, mut next: ResMut<NextState<Step>>| {
            commands.write_message(VtWriteMsg::new(term.0, b"0mX".to_vec()));
            next.set(Step(1));
        },
    );
    app.add_step(1, |q_lines: Query<&VtLine>, mut commands: Commands| {
        let Some(line) = q_lines.iter().find(|line| line.as_string() == "X") else {
            return;
        };
        r!(commands.assert(
            line.cells()[0].style.color == Color::srgb_u8(255, 0, 0),
            "split truecolor sequence did not retain parser state",
        ));
        commands.write_message(AppExit::Success);
    });
    assert!(app.run().is_success());
}

#[test]
fn parser_persists_partial_utf8_across_frames() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.insert_resource(Term(term));
        commands.write_message(VtWriteMsg::new(term, vec![0xf0, 0x9f]));
    });
    app.add_step(
        0,
        |term: Res<Term>, mut commands: Commands, mut next: ResMut<NextState<Step>>| {
            commands.write_message(VtWriteMsg::new(term.0, vec![0x98, 0x8e]));
            next.set(Step(1));
        },
    );
    app.add_step(
        1,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            if text(&q_term, &q_lines).as_deref() == Some("😎") {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn rendition_persists_across_frames() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.insert_resource(Term(term));
        commands.write_message(VtWriteMsg::new(term, b"\x1b[31m".to_vec()));
    });
    app.add_step(
        0,
        |term: Res<Term>, mut commands: Commands, mut next: ResMut<NextState<Step>>| {
            commands.write_message(VtWriteMsg::new(term.0, b"x".to_vec()));
            next.set(Step(1));
        },
    );
    app.add_step(1, |q_lines: Query<&VtLine>, mut commands: Commands| {
        let Some(line) = q_lines.iter().find(|line| line.as_string() == "x") else {
            return;
        };
        r!(commands.assert(
            line.cells()[0].style.color == css::DARK_RED.into(),
            "rendition state did not persist across frames",
        ));
        commands.write_message(AppExit::Success);
    });
    assert!(app.run().is_success());
}

#[test]
fn rich_helper_encodes_into_the_ordered_byte_stream() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.write_message(VtWriteMsg::new(term, b"a".to_vec()));
        commands.write_message(VtWriteMsg::new(
            term,
            term_writes_to_ansi(vec![TermWrite::new("b").with_color(basic::GREEN)]),
        ));
        commands.write_message(VtWriteMsg::new(term, b"c".to_vec()));
    });
    app.add_step(0, |q_lines: Query<&VtLine>, mut commands: Commands| {
        let Some(line) = q_lines.iter().find(|line| line.as_string() == "abc") else {
            return;
        };
        r!(commands.assert(
            line.cells()[1].style.color == basic::GREEN.into(),
            "rich helper style was not encoded",
        ));
        r!(commands.assert(
            line.cells()[2].style.color == basic::GREEN.into(),
            "rich helper bytes were not ordered with adjacent writes",
        ));
        commands.write_message(AppExit::Success);
    });
    assert!(app.run().is_success());
}

#[test]
fn allow_policy_renders_a_different_source_peer() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        commands.spawn(VtForegroundProcess::new(term));
        let background = commands.spawn_empty().id();
        commands.write_message(VtWriteMsg::from_peer(
            term,
            background,
            b"background".to_vec(),
        ));
    });
    app.add_step(
        0,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            if text(&q_term, &q_lines).as_deref() == Some("background") {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn suppress_policy_only_filters_a_different_source_peer() {
    let mut app = get_test_app();
    app.insert_resource(BackgroundTerminalOutput::Suppress);
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands
            .spawn((Terminal, VtSize { cols: 20, rows: 5 }))
            .id();
        let foreground = commands.spawn(VtForegroundProcess::new(term)).id();
        let background = commands.spawn_empty().id();
        commands.write_message(VtWriteMsg::from_peer(term, background, b"drop".to_vec()));
        commands.write_message(VtWriteMsg::from_peer(term, foreground, b"keep".to_vec()));
        commands.write_message(VtWriteMsg::new(term, b" direct".to_vec()));
    });
    app.add_step(
        0,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            if text(&q_term, &q_lines).as_deref() == Some("keep direct") {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn pending_ingress_evicts_oldest_whole_chunks() {
    let mut app = get_test_app();
    app.insert_resource(PendingVtWriteCap(3));
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands.spawn(Terminal).id();
        commands.write_message(VtWriteMsg::new(term, b"ab".to_vec()));
        commands.write_message(VtWriteMsg::new(term, b"cd".to_vec()));
    });
    app.add_step(
        0,
        |pending: Query<&PendingVtWrites>, mut commands: Commands| {
            let Ok(pending) = pending.single() else {
                return;
            };
            r!(commands.assert(pending.len() == 2, "pending byte count exceeded cap"));
            r!(commands.assert(
                pending.chunks()[0].bytes == b"cd",
                "pending queue did not evict the oldest whole chunk",
            ));
            commands.write_message(AppExit::Success);
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn terminal_despawn_discards_pending_ingress() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands.spawn(Terminal).id();
        commands.insert_resource(Term(term));
        commands.write_message(VtWriteMsg::new(term, b"pending".to_vec()));
    });
    app.add_step(
        0,
        |term: Res<Term>,
         pending: Query<&PendingVtWrites>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            if !pending.contains(term.0) {
                return;
            }
            commands.entity(term.0).despawn();
            next.set(Step(1));
        },
    );
    app.add_step(
        1,
        |pending: Query<&PendingVtWrites>, mut commands: Commands| {
            if pending.is_empty() {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn terminal_reinserted_before_cleanup_keeps_fresh_parser_state() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands.spawn(Terminal).id();
        commands.insert_resource(Term(term));
        commands.write_message(VtWriteMsg::new(term, b"pending".to_vec()));
    });
    app.add_step(
        0,
        |term: Res<Term>,
         pending: Query<&PendingVtWrites>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            if !pending.contains(term.0) {
                return;
            }
            commands
                .entity(term.0)
                .remove::<Terminal>()
                .insert((Terminal, VtSize { cols: 20, rows: 5 }));
            next.set(Step(1));
        },
    );
    app.add_step(
        1,
        |q_term: Query<TermInfo>, q_lines: Query<(Entity, &VtLine)>, mut commands: Commands| {
            if text(&q_term, &q_lines).as_deref() == Some("pending") {
                commands.write_message(AppExit::Success);
            }
        },
    );
    assert!(app.run().is_success());
}

#[test]
fn terminal_component_removal_clears_pending_ingress() {
    let mut app = get_test_app();
    app.add_systems(Startup, |mut commands: Commands| {
        let term = commands.spawn(Terminal).id();
        commands.insert_resource(Term(term));
        commands.write_message(VtWriteMsg::new(term, b"pending".to_vec()));
    });
    app.add_step(
        0,
        |term: Res<Term>,
         pending: Query<&PendingVtWrites>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            if !pending.contains(term.0) {
                return;
            }
            commands.entity(term.0).remove::<Terminal>();
            next.set(Step(1));
        },
    );
    app.add_step(
        1,
        |term: Res<Term>,
         entities: Query<Entity>,
         pending: Query<&PendingVtWrites>,
         mut commands: Commands| {
            if !entities.contains(term.0) || pending.contains(term.0) {
                return;
            }
            commands.write_message(AppExit::Success);
        },
    );
    assert!(app.run().is_success());
}
