//! Systems for programs.

use crate::prelude::*;
use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

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
