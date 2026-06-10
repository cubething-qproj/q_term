use crate::prelude::*;
use bevy::{platform::collections::HashMap, prelude::*};

/// Spawns a [`Process`] based on a recieved [`ShellSpawnMsg`]
pub fn spawn_process(
    mut commands: Commands,
    q_terminfo: Query<TermInfo>,
    q_shell: Query<(Entity, &Shell)>,
    mut reader: MessageReader<ShellSpawnMsg>,
) {
    for msg in reader.read() {
        let (shell_id, shell) = r!(q_shell.get(msg.shell));
        let term = r!(q_terminfo.get(shell.term));
        let term_id = term.id;
        let mut entt = commands.spawn_empty();
        let proc_id = entt.id();
        let val = (
            Process {
                entity: proc_id,
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
