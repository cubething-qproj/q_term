//! A special [`Process`] used to control other processes.

use crate::prelude::*;
use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};

// Kernel equivalent: pty follower.
// Obviated by single-use.
/// Marker struct for shell entities. The systems associated with this
/// struct must be implemented outside this crate.
#[derive(Component, Reflect, Debug)]
#[relationship(relationship_target = ShellTarget)]
#[component(on_add = Self::on_add)]
pub struct Shell(pub Entity);
impl Shell {
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let mut cmds = world.commands();
        cmds.entity(ctx.entity)
            .insert(ForegroundProcess(ctx.entity));
    }
    /// Spawn a new process. This will foreground the process, placing any other active
    /// process in the background.
    pub fn spawn_process<T: Process>(shell: Entity, commands: &mut Commands) -> Entity {
        let process = commands.spawn((T::default(), ShellJob(shell))).id();
        Self::foreground_process(shell, process, commands);
        process
    }
    fn foreground_process(shell: Entity, process: Entity, commands: &mut Commands) {
        commands.entity(process).insert(ForegroundProcess(shell));
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
