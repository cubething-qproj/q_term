//! Minimal input-buffer example: a chat-log style readline.
//!
//! Submitted lines accumulate above. The active prompt is redrawn in
//! place on every keystroke via `\r\x1b[2K> {buffer}`, so backspace
//! and Enter behave like a classic shell prompt.
//!
//! `q_term` reserves [`TerminalSystems::Input`] for shell-side input
//! and ships no built-in keyboard binding -- this example wires
//! [`KeyboardInput`] directly to demonstrate the seam.
//!
//! Controls:
//! * Printable keys / Space / Backspace / Enter: edit the prompt.
//! * **Alt+Down**: cycle the cursor style (Block → Beam → Underline).

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
    fg: Entity,
    buffer: String,
}

fn write(term: Entity, from: Entity, text: impl ToString) -> TermStdOut {
    TermStdOut {
        term,
        from,
        message: vec![TermWrite::new(text)],
    }
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
    app.add_systems(Update, (on_key, cycle_cursor_style));
    app.run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let term_id = commands.spawn(Terminal).id();
    let fg = commands.spawn(VtForegroundProcess::new(term_id)).id();
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
    commands.write_message(write(term_id, fg, PROMPT));
    commands.insert_resource(Input {
        term_id,
        fg,
        buffer: String::new(),
    });
}

fn on_key(
    mut events: MessageReader<KeyboardInput>,
    mut input: ResMut<Input>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    use bevy::input::keyboard::Key;

    // Modifier-bound shortcuts (Alt+...) are handled in their own
    // system to avoid leaking into the prompt buffer.
    let alt_held = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);

    let mut dirty = false;
    let mut submitted: Option<String> = None;

    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if alt_held {
            // Don't fold Alt-chorded keys into the buffer.
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
        commands.write_message(write(
            input.term_id,
            input.fg,
            format!("\r\x1b[2K{PROMPT}{line}\n{PROMPT}"),
        ));
    } else if dirty {
        commands.write_message(write(
            input.term_id,
            input.fg,
            format!("\r\x1b[2K{PROMPT}{}", input.buffer),
        ));
    }
}

/// Alt+Down cycles the active cursor style: Block → Beam → Underline
/// → Block. Demonstrates that `VtCursorStyle` is a live, per-cursor
/// component the host app can mutate at any time;
/// `update_cursor_display` picks up the change on the next render
/// tick.
fn cycle_cursor_style(
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<&mut VtCursorStyle, With<VtUiCursor>>,
) {
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    if !(alt && keys.just_pressed(KeyCode::ArrowDown)) {
        return;
    }
    for mut style in q.iter_mut() {
        *style = match *style {
            VtCursorStyle::Block => VtCursorStyle::Beam,
            VtCursorStyle::Beam => VtCursorStyle::Underline,
            VtCursorStyle::Underline => VtCursorStyle::Block,
        };
    }
}
