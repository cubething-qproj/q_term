use bevy::{
    image::TextureAtlasPlugin,
    input::InputPlugin,
    picking::PickingSettings,
    render::texture::TexturePlugin,
    text::TextPlugin,
    ui::UiPlugin,
};

use crate::prelude::*;

/// Build a headless test app that mirrors `tests::get_test_app`, but
/// instantiates `TerminalPlugin::new(Update, Update, Update)` so every
/// terminal set runs in the same schedule. Validates that the
/// schedule-collapse case still drives a `TermMsg::write` to a visible
/// `VtLine`.
fn collapsed_test_app() -> App {
    let mut app = App::new();
    app.insert_resource(PickingSettings {
        is_window_picking_enabled: false,
        ..Default::default()
    });
    app.add_plugins((
        TestRunnerPlugin::default(),
        DefaultPickingPlugins,
        WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..Default::default()
        },
        InputPlugin,
        UiPlugin,
        TextPlugin,
        TextureAtlasPlugin,
        ImagePlugin::default(),
        TexturePlugin,
        TerminalPlugin::new(Update, Update, Update),
    ));
    app.insert_resource(TestRunnerTimeout(2.));
    app
}

/// When `pre == update == post`, the `Input` and `RenderPrep` sets
/// become unordered against the `Measure` → `Process` chain. We work
/// around that here by writing the `TermMsg` from `Startup` (which
/// always precedes any `Update`-phase work) rather than from a system
/// in `TerminalSystems::Input`.
#[test]
fn single_schedule_collapse() {
    let mut app = collapsed_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });
        commands.write_message(TermMsg::write(term_id, "X"));
    });

    app.add_step(
        0,
        |q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         mut commands: Commands| {
            let terminfo = r!(q_term.single());
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }
            assert_eq!(lines.len(), 1, "Expected exactly 1 line");
            let (_, line) = &lines[0];
            commands.assert(line.as_string() == "X", "expected line == \"X\"");
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
