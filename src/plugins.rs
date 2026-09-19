//! The primary [`Plugin`] for q_term.
use bevy::{
    ecs::schedule::{InternedScheduleLabel, ScheduleLabel},
    ui::ui_layout_system,
};

use crate::prelude::*;

/// [`SystemSet`] slots for terminal systems, ordered by concern.
///
/// Variants run in declaration order within their schedule. `Input` is
/// reserved for shell-side input handling and is intentionally empty in
/// this crate.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerminalSystems {
    /// External input feeding the terminal. Empty in `q_term`; populated
    /// by downstream consumers (e.g. `shell`).
    Input,
    /// Measurement systems: font, glyph metrics, viewport sizing.
    Measure,
    /// Stateful processing: message handling, scrollback updates.
    Process,
    /// Prepare UI nodes for the next frame's render.
    Render,
}

/// The primary plugin for q_term.
///
/// Explicitly orders [`TerminalSystems::Input`] -> [`TerminalSystems::Measure`]
/// -> [`TerminalSystems::Process`] -> [`TerminalSystems::Render`] in `Update`.
/// UI refresh also runs in `PostUpdate` after `ui_layout_system`.
#[derive(Debug)]
pub struct TerminalPlugin {
    update_schedule: InternedScheduleLabel,
    post_update_schedule: InternedScheduleLabel,
}
impl Default for TerminalPlugin {
    fn default() -> Self {
        Self::new(Update, PostUpdate)
    }
}
impl TerminalPlugin {
    /// Configure the schedules for terminal processing and post-layout UI refresh.
    pub fn new(
        update_schedule: impl ScheduleLabel,
        post_update_schedule: impl ScheduleLabel,
    ) -> Self {
        Self {
            update_schedule: update_schedule.intern(),
            post_update_schedule: post_update_schedule.intern(),
        }
    }
}

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        use crate::msgs::term::*;
        use crate::systems::term::*;
        app.add_message::<VtReplyMsg>();
        app.add_message::<VtWriteMsg>();
        app.add_message::<TermScrollMsg>();
        app.add_message::<TermJumpToBottomMsg>();
        app.add_message::<TermReflowMsg>();
        app.add_message::<TermRedrawRequestedMsg>();

        app.init_resource::<VtScrollSensitivity>();
        app.init_resource::<BackgroundTerminalOutput>();
        app.init_resource::<PendingVtWriteCap>();

        app.configure_sets(
            self.update_schedule,
            (
                TerminalSystems::Input,
                TerminalSystems::Measure,
                TerminalSystems::Process,
                TerminalSystems::Render,
            )
                .chain(),
        );
        app.add_systems(
            self.update_schedule,
            (
                (update_font, update_char_width, resize).in_set(TerminalSystems::Measure),
                (
                    cleanup_removed_terminals,
                    drain_pending,
                    process_input,
                    apply_scroll,
                    apply_reflow,
                    scroll_viewport,
                )
                    .chain()
                    .in_set(TerminalSystems::Process),
                (update_cursor_display, flash_cursor).in_set(TerminalSystems::Render),
            ),
        );
        app.add_systems(
            self.post_update_schedule,
            refresh_ui
                .after(ui_layout_system)
                .in_set(TerminalSystems::Render),
        );
    }
}
