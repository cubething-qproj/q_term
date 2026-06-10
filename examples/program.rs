//! Minimal program example.

use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use q_term::impl_program_label;
use q_term::prelude::*;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct MyProg;
impl_program_label!(MyProg, "myprog");

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
        ProcessPlugin,
        EguiPlugin::default(),
        WorldInspectorPlugin::default(),
    ));
    app.register_program(MyProg);

    // Say hello once every second
    app.add_program_system(
        MyProg,
        Update,
        |id: In<Entity>,
         procs: Query<&Process>,
         mut commands: Commands,
         mut timer: Local<Option<Timer>>,
         time: Res<Time>| {
            let proc = procs.get(*id).unwrap();
            if timer.is_none() {
                *timer = Some(Timer::from_seconds(1., TimerMode::Repeating));
            }
            let t = timer.as_mut().unwrap();
            t.tick(time.delta());
            if t.just_finished() {
                let msg = format!("Hello from process {}!\n", *id);
                info!(msg);
                proc.write(&mut commands, msg);
            }
        },
    );

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
            ..default()
        },
        BackgroundColor(Color::BLACK),
        VtUi::new(term_id),
    ));
    let shell = commands.spawn(Shell { term: term_id }).id();
    commands.write_message(ShellSpawnMsg::new(MyProg, shell));
}
