//! Minimal program example.

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};
use q_term::impl_program_label;
use q_term::prelude::*;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MySchedule;

#[derive(Clone, Copy)]
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
    ));
    app.register_program(MyProg);

    // Say hello once every second
    app.add_program_system(
        MyProg,
        Update,
        |proc: In<Process>,
         mut commands: Commands,
         mut timer: Local<Option<Timer>>,
         time: Res<Time>| {
            if timer.is_none() {
                timer = Some(Timer::from_seconds(1., TimerMode::Repeating)).into();
            }
            timer.as_ref().unwrap().tick(time.delta());
            if timer.just_finished() {
                proc.write(
                    &mut commands,
                    format!("Hello from process {}!", proc.entity),
                );
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
