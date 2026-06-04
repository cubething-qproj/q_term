//! Basic data types required for process execution
use bevy::{
    ecs::{
        define_label,
        intern::Interned,
        schedule::{InternedScheduleLabel, ScheduleLabel},
        system::SystemId,
    },
    platform::collections::{HashMap, hash_map::Entry},
};

use crate::prelude::*;

#[test]
fn example() {
    use crate::prelude::*;
    let mut app = App::new();

    #[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct MySchedule;

    #[derive(Clone, Copy)]
    struct MyProg;
    impl_program_label!(MyProg, "myprog");

    let term = app.world_mut().spawn(Terminal).id();
    let shell = app.world_mut().spawn(Shell).id();

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

    app.add_systems(Startup, |mut commands: Commands| {
        Shell::spawn_process::<MyProg>(shell, &mut commands);
    });
}

/// A [`Resource`] which tracks registered a [`Program`] through its
/// [`ProgramLabel`].
#[derive(Resource, Default, Deref, DerefMut)]
pub struct Programs(HashMap<InternedProgramLabel, ProgramData>);
impl Programs {
    pub fn get(&self, label: impl ProgramLabel) -> Option<&ProgramData> {
        self.0.get(&label.intern())
    }
    pub fn get_mut(&mut self, label: impl ProgramLabel) -> Option<&mut ProgramData> {
        self.0.get_mut(&label.intern())
    }
    pub fn contains(&self, label: impl ProgramLabel) -> bool {
        self.0.contains_key(&label.intern())
    }
    pub fn entry(
        &mut self,
        label: impl ProgramLabel,
    ) -> Entry<'_, InternedProgramLabel, ProgramData> {
        self.0.entry(label.intern())
    }
}

/// Data associated with a [`Program`]. Specifically, [`SystemId`]s mapped to [`ScheduleLabel`]s.
/// Note that this currently only accepts **one** system per label.
pub type ProgramData = HashMap<InternedScheduleLabel, SystemId<In<Process>, ()>>;

define_label!(
    /// A [`Process`] is a potentially long-running [`Schedule`] which is
    /// managed by a [`Shell`]. Processes should be run by running [`Shell::spawn_process`].
    /// The process dies when this component is removed.
    /// Lifecycle hooks are a good way to implement de/initialization behaviors.
    // TODO: inline doctest here
    // TODO: Process should have reference to its own entity.
    // Cannot assume singleton entity, multiple instances of the same processes
    // are likely to be spawned (job control).
    // TODO: Piping? Need file descriptors if so. Probably a relationship (ProcessFd<const CHANNEL: u8)
    ProgramLabel,
    PROGRAM_LABEL_INTERNER,
    extra_methods: {
        /// Name of the process, used to run it on the command line.
        fn name(&self) -> ProcessName;
    },
    extra_methods_impl: {
        /// Name of the process, used to run it on the command line.
        fn name(&self) -> ProcessName {
            ProcessName::new("PLACEHOLDER")
        }
    }
);

/// Shorthand for Interned<dyn ProgramLabel>
pub type InternedProgramLabel = Interned<dyn ProgramLabel>;

/// A [`Program`] is a set of instructions which is instantiated by spawning a
/// [`Process`]. Where the [`Process`] is the [`Component`], this is the [`System`]
/// manager.
pub trait Program {
    /// Signal overriding behavior.
    /// Returns a HashMap from the signal to its override command.
    /// By default, SIGINT, SIGQUIT, SIGTERM, and SIGHUP all despawn the entity.
    /// Use [`ProcessSignalOverride`] to define the schedule.
    fn trap(&self, _kind: Sig) -> Option<Schedule> {
        None
    }
}

// TODO: Derive macro for ProgramLabel
#[macro_export]
macro_rules! impl_program_label {
    ($t:ty, $name:literal) => {
        impl ProgramLabel for $t {
            fn name() -> ProcessName {
                ProcessName::new($name)
            }
            fn dyn_clone(&self) -> Box<dyn ProgramLabel> {
                Box::new(self.clone())
            }
        }
    };
}

/// An instantiated process; a running program. Modify its behavior by
/// implementing its [`ProgramLabel`]
#[derive(Component, Clone)]
#[component(immutable)]
pub struct Process {
    /// The currently attached entity ID.
    /// Equivalent to UNIX pid.
    pub entity: Entity,
    /// The [`ProgramLabel`] associated with this [`Process`].
    /// Determines what this process _does_.
    pub prog: InternedProgramLabel,
    /// Signal catching behavior
    pub signal_overrides: HashMap<Sig, SystemId>,
    /// Argument values.
    pub argv: Vec<String>,
    /// Environment variables.
    pub environ: HashMap<String, String>,
    /// stdin
    pub fd0: Entity,
    /// stdout
    pub fd1: Entity,
    /// stderr
    pub fd2: Entity,
}
impl Process {
    /// Writes to stdout (the entity at self.fd1)
    pub fn write(&self, commands: &mut Commands, msg: impl ToString) {
        let msg = StdOut::write(self.fd1, msg);
        commands.write_message(msg);
    }
    /// Writes to stderr (the entity at self.fd2)
    pub fn write_err(&self, commands: &mut Commands, msg: impl ToString) {
        let msg = StdOut::write(self.fd2, msg);
        commands.write_message(msg);
    }
}

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

pub trait ProgramAppExt {
    fn register_program(&mut self, prog: impl ProgramLabel);
    // NOTE: This is similar to a ScreenScope
    fn add_program_system<M>(
        &mut self,
        prog: impl ProgramLabel + Clone,
        schedule: impl ScheduleLabel,
        system: impl IntoSystem<In<Process>, (), M> + 'static,
    );
}
impl ProgramAppExt for App {
    fn register_program(&mut self, prog: impl ProgramLabel) {
        self.world_mut().init_resource::<Programs>();
        self.world_mut()
            .resource_mut::<Programs>()
            .0
            .insert(prog.intern(), ProgramData::default());
    }
    fn add_program_system<M>(
        &mut self,
        prog: impl ProgramLabel + Clone,
        schedule: impl ScheduleLabel,
        system: impl IntoSystem<In<Process>, (), M> + 'static,
    ) {
        self.init_resource::<Programs>();
        let id = self.register_system(system);
        let mut res = self.world_mut().resource_mut::<Programs>();
        let data = res.entry(prog.clone()).or_default();
        data.insert(schedule.intern(), id);
    }
}

pub fn run_programs<S: ScheduleLabel + Default>(
    mut commands: Commands,
    q_procs: Query<&Process>,
    progs: Res<Programs>,
) {
    for proc in q_procs.iter() {
        let pdata = c!(progs.0.get(&proc.prog));
        let sysid = c!(pdata.get(&S::default().intern()));
        commands.run_system_with(*sysid, proc.clone());
    }
}

macro_rules! impl_run_progs {
    ($app:ident, $($sched:ident),+) => {
        $(
            $app.add_systems($sched, run_programs::<$sched>);
        )+
    };
}

pub fn plugin(app: &mut App) {
    impl_run_progs!(
        app,
        PreUpdate,
        Update,
        PostUpdate,
        FixedPreUpdate,
        FixedUpdate,
        FixedPostUpdate
    );
}
