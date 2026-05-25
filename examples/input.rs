//! Minimal input-buffer example: a chat-log style readline.
//!
//! Submitted lines accumulate above. The active prompt is redrawn in
//! place on every keystroke via `\r\x1b[2K> {buffer}`, so backspace
//! and Enter behave like a classic shell prompt.
//!
//! `q_term` reserves [`TerminalSystems::Input`] for shell-side input
//! and ships no built-in keyboard binding -- this example wires
//! [`KeyboardInput`] directly to demonstrate the seam.

use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
    window::WindowResolution,
};
use q_term::prelude::*;

const PROMPT: &str = "> ";

#[derive(Resource)]
struct Input {
    term_id: Entity,
    buffer: String,
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(720, 400),
                ..default()
            }),
            ..default()
        }),
        TerminalPlugin,
    ));
    app.add_systems(Startup, setup);
    app.add_systems(Update, on_key);
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
    // Initial prompt.
    commands.write_message(TermInputMsg::write(term_id, PROMPT));
    commands.insert_resource(Input {
        term_id,
        buffer: String::new(),
    });
}

fn on_key(
    mut events: MessageReader<KeyboardInput>,
    mut input: ResMut<Input>,
    mut commands: Commands,
) {
    use bevy::input::keyboard::Key;

    let mut dirty = false;
    let mut submitted: Option<String> = None;

    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Enter => {
                submitted = Some(std::mem::take(&mut input.buffer));
                dirty = true;
            }
            Key::Backspace => {
                if input.buffer.pop().is_some() {
                    dirty = true;
                }
            }
            Key::Space => {
                input.buffer.push(' ');
                dirty = true;
            }
            Key::Character(s) => {
                input.buffer.push_str(s);
                dirty = true;
            }
            _ => {}
        }
    }

    if let Some(line) = submitted {
        // Commit current prompt into scrollback, then emit a fresh
        // empty prompt below it. `\r\x1b[2K` is redundant here
        // because the newline starts us on virgin space, but keeping
        // it makes the redraw idiom uniform for the live-edit path.
        commands.write_message(TermInputMsg::write(
            input.term_id,
            format!("\r\x1b[2K{PROMPT}{line}\n{PROMPT}"),
        ));
    } else if dirty {
        commands.write_message(TermInputMsg::write(
            input.term_id,
            format!("\r\x1b[2K{PROMPT}{}", input.buffer),
        ));
    }
}
