use bevy::{prelude::*, window::WindowResolution};
use q_term::prelude::*;

const ANSI_NAMES: [&str; 8] = [
    "Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White",
];

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "q_term — ANSI palette".into(),
                resizable: true,
                resolution: WindowResolution::new(720, 480),
                ..Default::default()
            }),
            ..Default::default()
        }),
        TerminalPlugin,
    ));
    app.add_systems(Startup, setup);
    app.run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    let term_id = commands.spawn(Terminal).id();
    commands.spawn((
        Node {
            width: vw(100),
            height: vh(100),
            ..Default::default()
        },
        BackgroundColor(Color::BLACK),
        VtUi::new(term_id),
    ));

    let mut out = String::new();

    // Header
    out.push_str("  ANSI Color Palette\n");
    out.push_str("  ──────────────────\n\n");

    // Standard foreground (30–37)
    out.push_str("  Standard FG:  ");
    for i in 0..8u8 {
        out.push_str(&format!("\x1b[{}m {:>7} \x1b[0m", 30 + i, ANSI_NAMES[i as usize]));
    }
    out.push('\n');

    // Bright foreground (90–97)
    out.push_str("  Bright FG:    ");
    for i in 0..8u8 {
        out.push_str(&format!("\x1b[{}m {:>7} \x1b[0m", 90 + i, ANSI_NAMES[i as usize]));
    }
    out.push_str("\n\n");

    // Standard background (40–47) with contrasting fg
    out.push_str("  Standard BG:  ");
    for i in 0..8u8 {
        let fg = if i < 4 { "97" } else { "30" };
        out.push_str(&format!("\x1b[{fg};{}m {:>7} \x1b[0m", 40 + i, ANSI_NAMES[i as usize]));
    }
    out.push('\n');

    // Bright background (100–107) with contrasting fg
    out.push_str("  Bright BG:    ");
    for i in 0..8u8 {
        let fg = if i < 4 { "97" } else { "30" };
        out.push_str(&format!(
            "\x1b[{fg};{}m {:>7} \x1b[0m",
            100 + i,
            ANSI_NAMES[i as usize]
        ));
    }
    out.push_str("\n\n");

    // 24-bit truecolor gradient
    out.push_str("  24-bit:       ");
    for i in 0..32u8 {
        let r = (i as f32 / 31.0 * 255.0) as u8;
        let b = 255 - r;
        out.push_str(&format!("\x1b[48;2;{r};0;{b}m "));
    }
    out.push_str("\x1b[0m\n");

    commands.write_message(TermMsg::write(term_id, out));
}
