//! A special [`Process`] used to control other processes.

use crate::prelude::*;
use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    platform::collections::HashMap,
};

// Kernel equivalent: pty follower.
// Obviated by single-use.
/// Marker struct for shell entities. The systems associated with this
/// struct must be implemented outside this crate.
#[derive(Component, Reflect, Debug)]
#[relationship(relationship_target = ShellTarget)]
#[component(on_add = Self::on_add)]
#[require(ForegroundProcessGroup)]
pub struct Shell {
    #[relationship]
    pub term: Entity,
}
impl Shell {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let mut cmds = world.commands();
        cmds.entity(ctx.entity)
            .insert(ForegroundProcess(ctx.entity));
    }
}

#[derive(Message, Debug)]
pub struct ShellSpawnMsg {
    pub prog: InternedProgramLabel,
    pub shell: Entity,
    pub argv: Vec<String>,
    pub environ: HashMap<String, String>,
}
impl ShellSpawnMsg {
    pub fn new(prog: impl ProgramLabel, shell: Entity) -> Self {
        Self {
            prog: prog.intern(),
            shell,
            argv: Vec::new(),
            environ: HashMap::new(),
        }
    }
    pub fn with_args(prog: impl ProgramLabel, shell: Entity, argv: Vec<String>) -> Self {
        Self {
            prog: prog.intern(),
            shell,
            argv,
            environ: HashMap::new(),
        }
    }
}

/// Attached to the [`Terminal`] when spawning a [`Shell`].
#[derive(Component, Reflect, Debug)]
#[relationship_target(relationship = Shell)]
pub struct ShellTarget(Entity);
impl ShellTarget {
    pub fn target(&self) -> Entity {
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

/// This group gets its [stdio](crate::data::io) piped directly to the [`Terminal`].
/// **Important:** this should _only_ be set by the shell.
#[derive(Component, Reflect, Debug, Default)]
#[relationship_target(relationship = ForegroundProcess)]
pub struct ForegroundProcessGroup {
    #[relationship_target]
    processes: Vec<Entity>,
}

/// Attached to a [`Process`] in the [`ForegroundProcessGroup`].
/// The inner value is a pointer to a [`Shell`] with an attached
/// [`ForegroundProcessGroup`]
#[derive(Component, Reflect, Debug)]
#[relationship(relationship_target = ForegroundProcessGroup)]
pub struct ForegroundProcess(pub(crate) Entity);
