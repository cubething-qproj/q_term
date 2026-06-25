mod term;

pub mod prelude {
    pub use super::get_test_app;
    pub use super::helpers::*;
    pub use bevy::prelude::*;
    pub use q_term::prelude::*;
    pub use q_term::systems::term::*;
    pub use q_test_harness::prelude::*;
}

/// Test-only helpers shared via the `prelude` module.
pub mod helpers {
    use bevy::prelude::*;
    use q_term::prelude::*;

    /// `(terminal, foreground_process)` pair stashed by [`spawn_test_term`].
    /// Step systems pull this out of the `World` instead of plumbing
    /// the entities through closure captures.
    #[derive(Resource, Clone, Copy, Debug)]
    pub struct TestTerm {
        pub term: Entity,
        pub fg: Entity,
    }

    /// Spawn a `Terminal` of the given size with a foreground process
    /// pointing at it, insert a [`TestTerm`] resource, and return the
    /// pair for inline use.
    pub fn spawn_test_term(commands: &mut Commands, size: VtSize) -> TestTerm {
        let term = commands.spawn((Terminal, size)).id();
        let fg = commands.spawn(VtForegroundProcess::new(term)).id();
        commands.insert_resource(TestTerm { term, fg });
        TestTerm { term, fg }
    }

    /// Build a plain-text [`TermStdOut`] addressed to `term` from the
    /// given foreground process. `from` is mandatory: a write whose
    /// `from` doesn't match the terminal's `VtForegroundProcess` is
    /// dropped by `process_input`, so tests must thread the right
    /// entity through.
    pub fn write(term: Entity, from: Entity, text: impl ToString) -> TermStdOut {
        TermStdOut {
            term,
            from,
            message: vec![TermWrite::new(text)],
        }
    }

    /// Same as [`write`] but appends a trailing newline -- the common
    /// case for line-oriented test output.
    pub fn writeln(term: Entity, from: Entity, text: impl ToString) -> TermStdOut {
        let mut s = text.to_string();
        s.push('\n');
        TermStdOut {
            term,
            from,
            message: vec![TermWrite::new(s)],
        }
    }

    /// Build a multi-span [`TermStdOut`]. See [`write`] for the
    /// `from`-filter contract.
    pub fn write_spans(term: Entity, from: Entity, message: Vec<TermWrite>) -> TermStdOut {
        TermStdOut {
            term,
            from,
            message,
        }
    }
}

use bevy::{
    image::TextureAtlasPlugin, input::InputPlugin, picking::PickingSettings,
    render::texture::TexturePlugin, text::TextPlugin, ui::UiPlugin,
};
use prelude::*;

/// Creates a headless Bevy app with all plugins necessary for q_term testing.
/// Mirrors the `test_harness` function in `q_term::test`.
pub fn get_test_app() -> App {
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
        TerminalPlugin,
    ));
    app.insert_resource(TestRunnerTimeout(2.));
    app
}
