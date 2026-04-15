mod term;

pub mod prelude {
    pub use super::get_test_app;
    pub use bevy::prelude::*;
    pub use q_term::prelude::*;
    pub use q_test_harness::prelude::*;
}

use bevy::{
    image::TextureAtlasPlugin,
    input::InputPlugin,
    picking::PickingSettings,
    render::texture::TexturePlugin,
    text::TextPlugin,
    ui::UiPlugin,
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
