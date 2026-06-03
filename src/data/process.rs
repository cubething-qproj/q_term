//! Basic data types required for process execution
use bevy::ecs::schedule::ScheduleLabel;

use crate::prelude::*;

#[test]
fn doctest_process() {
    use crate::prelude::*;
    let mut app = App::new();

    #[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct MySchedule;

    #[derive(Component)]
    #[component(on_add = Self::on_add, on_remove = Self::on_remove)]
    struct MyProcess;
    impl MyProcess {
        fn on_add(mut world: DeferredWorld, ctx: HookCtx) {
            info!("Initializing MyProcess");
        }
        fn on_remove(mut world: DeferredWorld, ctx: HookCtx) {
            info!("Deinitializing MyProcess");
        }
    }
    impl Process for MyProcess {
        fn name() -> &'static str {
            "my_process"
        }
        fn schedule_label() -> impl ScheduleLabel {
            MySchedule
        }
        fn catch_signal(mut commands: Commands, this: Entity, signal: impl CatchableSignal) {
            match signal.kind() {
                SignalKind::Int => {
                    commands.stdout_from(this, "Can't kill me!");
                }
                _ => {}
            }
        }
    }

    let term = app.world_mut().spawn(Terminal).id();
    let shell = app.world_mut().spawn(Shell).id();
    schedule.add_systems(move |mut commands: Commands, terminfo: Query<TermInfo>| {
        let terminfo = terminfo.get(term).unwrap();
        terminfo.write(&mut commands, "hi\n");
    });
    app.register_process::<MyProcess>();
    app.add_systems(Startup, |mut commands: Commands| {
        Shell::spawn_process::<MyProcess>(shell, &mut commands);
    });
}

/// A [`Process`] is a potentially long-running [`Schedule`] which is
/// managed by a [`Shell`]. Processes should be run by running [`Shell::spawn_process`].
/// The process dies when this component is removed.
/// Lifecycle hooks are a good way to implement de/initialization behaviors.
// TODO: inline doctest here
// TODO: Process should have reference to its own entity.
// Cannot assume singleton entity, multiple instances of the same processes
// are likely to be spawned (job control).
// TODO: Piping? Need file descriptors if so. Probably a relationship (ProcessFd<const CHANNEL: u8)
pub trait Process: Component + Default + Reflect {
    /// Name of the process, used to run it on the command line.
    fn name() -> ProcessName;

    /// The schedule assocaited with this [`Process`].
    /// By default this schedule will run on [`Update`].
    /// Override which step this runs on with [Self::runs_on].
    fn schedule_label() -> impl ScheduleLabel;

    /// Override to change which schedule this subschedule runs on.
    /// Defaults to [`Update`]
    fn runs_on() -> impl ScheduleLabel {
        Update
    }

    /// Signal catching behavior.
    /// Defaults: [SIGINT], [SIGQUIT], [SIGTERM], [SIGHUP] all despawn the entity.
    fn catch_signal(mut commands: Commands, this: Entity, signal: impl CatchableSignal) {
        use SignalKind::*;
        match signal.kind() {
            Int | Quit | Term | Hup => {
                commands.entity(this).despawn();
            }
            _ => {}
        }
    }
}

pub enum ProcessCommands {
    /// Write to stdout. The pty systems will determine where this gets piped to.
    StdOut { this: Entity, message: String },
}
impl Command for ProcessCommands {
    fn apply(self, world: &mut World) {
        match self {
            ProcessCommands::StdOut { this, message } => {
                let e = r!(world.get_entity(this));
            }
        }
    }
}
impl ProcessCommands {}

/// The name of a [`Process`]. This type exists to ensure validity on construction.
/// In particular, process names must not contain whitespace.
pub struct ProcessName(&'static str);
impl ProcessName {
    pub fn new(name: &'static str) -> Self {
        if name.split_whitespace().count() > 1 {
            panic!("Process name must not contain whitespace.");
        }
        Self(name)
    }
    pub fn name(&self) -> &'static str {
        self.0
    }
}

/// Marker for a process owned by a [`Shell`].
#[derive(Component, Reflect, Debug)]
#[relationship(relationship_target = ShellJobTarget)]
pub struct ShellJob(pub Entity);

/// The [`Shell`] which owns this [`Job`].
#[derive(Component, Reflect, Debug)]
#[relationship_target(relationship = ShellJob)]
pub struct ShellJobTarget(Entity);

/// The focused program. Could be the shell or any other process.
/// This is the mechanism behind blocking shell input.
/// All messages sent via [`StdIn`]
/// **Important:** this should _only_ be set by the shell.
#[derive(Component, Reflect, Debug)]
#[relationship(relationship_target = ForegroundJobTarget)]
pub struct ForegroundProcess(
    /// The shell for whom this process is the foreground.
    pub Entity,
);

/// Attached to the [`Shell`] which owns this [`ForegroundJob`]
#[derive(Component, Reflect, Debug)]
#[relationship_target(relationship = ForegroundProcess)]
pub struct ForegroundJobTarget(
    /// The foreground process.
    Entity,
);
