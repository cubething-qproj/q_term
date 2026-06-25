use bevy::{color::palettes::css, prelude::*, window::WindowResolution};
use q_term::prelude::*;

const LONG_LINE: &str = "This is a really long line! It should be wrapping. Just checking :) How are you doing today? I'm doing pretty good myself.\n";

fn write(term: Entity, from: Entity, text: impl ToString) -> TermStdOut {
    TermStdOut {
        term,
        from,
        message: vec![TermWrite::new(text)],
    }
}
fn writeln(term: Entity, from: Entity, text: impl ToString) -> TermStdOut {
    let mut s = text.to_string();
    s.push('\n');
    write(term, from, s)
}
fn write_spans(term: Entity, from: Entity, message: Vec<TermWrite>) -> TermStdOut {
    TermStdOut {
        term,
        from,
        message,
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resizable: true,
                resolution: WindowResolution::new(800, 600),
                ..Default::default()
            }),
            ..Default::default()
        }),
        TerminalPlugin,
    ));
    app.add_plugins((
        bevy_inspector_egui::bevy_egui::EguiPlugin::default(),
        bevy_inspector_egui::quick::WorldInspectorPlugin::new(),
    ));
    app.add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Camera2d);
        let term_id = commands.spawn(Terminal).id();
        let fg = commands.spawn(VtForegroundProcess::new(term_id)).id();
        commands.spawn((
            Node {
                width: vw(100),
                height: vh(100),
                ..Default::default()
            },
            // to prevent flashing on resize
            BackgroundColor(Color::BLACK),
            VtUi::new(term_id),
        ));

        // test scrolling...
        // Hide the cursor (DECTCEM) — these examples are display-only.
        commands.write_message(write(term_id, fg, "\x1b[?25l"));
        for i in 0..20 {
            commands.write_message(writeln(term_id, fg, format!("{i}")));
        }

        // Simple writes do not clear style or add newlinew to the end of their writes.
        // They can be considered "raw" and typically aren't going to be your goto.
        commands.write_message(write(term_id, fg, "hello\nhere are multiple lines\n"));
        commands.write_message(write(
            term_id,
            fg,
            "\x1b[31mthis is red text \x1b[47mwith a white background!\n",
        ));
        commands.write_message(write(term_id, fg, "still red and white...\n"));
        commands.write_message(write(term_id, fg, "\x1b[0mbut no longer :)\n"));
        // Writing spans is another way to directly manipulate the buffer.
        commands.write_message(write_spans(
            term_id,
            fg,
            vec![
                TermWrite::new("you can do multiple spans too, "),
                TermWrite::new("with style 😎\n")
                    .with_color(css::GREEN)
                    .with_background(css::BISQUE),
            ],
        ));
        // ... but writing lines is probably what you're looking for.
        commands.write_message(writeln(term_id, fg, LONG_LINE));

        // commands.write_message(TermScrollMsg::new(term_id, 10));
        // commands.write_message(TermScrollMsg::new(term_id, -5));
    });
    app.add_systems(PostUpdate, |mut ran: Local<bool>| {
        if *ran {
            // commands.write_message(AppExit::Success);
        } else {
            *ran = true;
        }
    });
    app.run();
}
