//! Systems for programs.

use crate::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, platform::collections::HashMap, prelude::*};

pub fn run_programs<S: ScheduleLabel + Default>(
    mut commands: Commands,
    q_procs: Query<(Entity, &Process)>,
    progs: Res<Programs>,
) {
    for (entity, proc) in q_procs.iter() {
        trace!("{:?}: Running {:?}", S::default(), proc.prog.name());
        let pdata = c!(progs.0.get(&proc.prog));
        let sysid = cq!(pdata.get(&S::default().intern()));
        commands.run_system_with(*sysid, entity);
    }
}

/// Spawns a [`Process`] based on a recieved [`ShellSpawnMsg`]
pub fn spawn_process(
    mut commands: Commands,
    q_terminfo: Query<TermInfo>,
    q_shell: Query<(Entity, &Shell)>,
    mut reader: MessageReader<ShellSpawnMsg>,
) {
    for msg in reader.read() {
        let (shell_id, shell) = c!(q_shell.get(msg.shell));
        let term = cq!(q_terminfo.get(shell.term));
        let term_id = term.id;
        let mut entt = commands.spawn_empty();
        let val = (
            Process {
                prog: msg.prog,
                argv: msg.argv.clone(),
                environ: msg.environ.clone(),
                fd0: term_id,
                fd1: term_id,
                fd2: term_id,
                signal_overrides: HashMap::new(), // TODO: How??
            },
            ShellJob(shell_id),
            ForegroundProcess(shell_id),
        );
        debug!("Spawned process {val:?}");
        entt.insert(val);
    }
}
