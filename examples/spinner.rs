//! Minimal throbber / spinner example.
//!
//! Each tick rewrites the same line in place using `\r\x1b[2K`
//! (carriage return + Erase-in-Line mode 2) so that successive
//! variable-width status messages overwrite cleanly without leaving
//! trailing characters from the previous frame.

use bevy::{prelude::*, window::WindowResolution};
use q_term::prelude::*;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const LABELS: [&str; 4] = ["Loading", "Compiling crates", "Linking", "Done!"];

#[derive(Resource)]
struct Spinner {
    term_id: Entity,
    tick: Timer,
    frame: usize,
    label: usize,
    label_tick: Timer,
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(640, 200),
                ..default()
            }),
            ..default()
        }),
        TerminalPlugin,
    ));
    app.add_systems(Startup, setup);
    app.add_systems(Update, tick);
    app.run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let term_id = commands.spawn(Terminal).id();
    commands.spawn((
        Node {
            width: vw(100),
            height: vh(100),
            ..default()
        },
        BackgroundColor(Color::BLACK),
        VtUi::new(term_id),
    ));
    commands.insert_resource(Spinner {
        term_id,
        tick: Timer::from_seconds(0.1, TimerMode::Repeating),
        frame: 0,
        label: 0,
        label_tick: Timer::from_seconds(1.5, TimerMode::Repeating),
    });
}

fn tick(time: Res<Time>, mut s: ResMut<Spinner>, mut commands: Commands) {
    s.tick.tick(time.delta());
    s.label_tick.tick(time.delta());
    if s.label_tick.just_finished() {
        s.label = (s.label + 1) % LABELS.len();
    }
    if !s.tick.just_finished() {
        return;
    }
    s.frame = (s.frame + 1) % FRAMES.len();
    // `\r` returns to col 0; `\x1b[2K` wipes whatever was on the
    // previous frame so shorter labels don't leave a tail behind.
    commands.write_message(TermInputMsg::write(
        s.term_id,
        format!("\r\x1b[2K{} {}", FRAMES[s.frame], LABELS[s.label]),
    ));
}
