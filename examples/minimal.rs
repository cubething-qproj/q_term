use bevy::{color::palettes::css, prelude::*, window::WindowResolution};
use q_term::prelude::*;

const LONG_LINE: &str = "This is a really long line! It should be wrapping. Just checking :) How are you doing today? I'm doing pretty good myself.\n";

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
        for i in 0..20 {
            commands.write_message(TermMsg::writeln(term_id, format!("{i}")));
        }

        // Simple writes do not clear style or add newlinew to the end of their writes.
        // They can be considered "raw" and typically aren't going to be your goto.
        commands.write_message(TermMsg::write(term_id, "hello\nhere are multiple lines\n"));
        commands.write_message(TermMsg::write(
            term_id,
            "\x1b[31mthis is red text \x1b[47mwith a white background!\n",
        ));
        commands.write_message(TermMsg::write(term_id, "still red and white...\n"));
        commands.write_message(TermMsg::write(term_id, "\x1b[0mbut no longer :)\n"));
        // Writing spans is another way to directly manipulate the buffer.
        commands.write_message(TermMsg::write_spans(
            term_id,
            vec![
                TermWrite::new("you can do multiple spans too, "),
                TermWrite::new("with style 😎\n")
                    .with_color(css::GREEN)
                    .with_background(css::BISQUE),
            ],
        ));
        // ... but writing lines is probably what you're looking for.
        commands.write_message(TermMsg::writeln(term_id, LONG_LINE));

        // commands.write_message(TermMsg::scroll(term_id, 10));
        // commands.write_message(TermMsg::scroll(term_id, -5));
    });
    app.add_systems(
        PostUpdate,
        |mut commands: Commands, mut ran: Local<bool>| {
            if *ran {
                // commands.write_message(AppExit::Success);
            } else {
                *ran = true;
            }
        },
    );
    app.run();
}
