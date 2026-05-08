use bevy::ecs::schedule::{InternedScheduleLabel, ScheduleLabel};
use bevy::ui::ui_layout_system;

use crate::prelude::*;

/// [`SystemSet`] slots for terminal systems, ordered by concern.
///
/// Variants run in declaration order within their schedule. `Input` is
/// reserved for shell-side input handling and is intentionally empty in
/// this crate.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalSystems {
    /// External input feeding the terminal. Empty in `q_term`; populated
    /// by downstream consumers (e.g. `shell`).
    Input,
    /// Measurement systems: font, glyph metrics, viewport sizing.
    Measure,
    /// Stateful processing: message handling, scrollback updates.
    Process,
    /// Prepare UI nodes for the next frame's render.
    RenderPrep,
}

/// The primary plugin for q_term.
///
/// Routes [`TerminalSystems`] sets across three configurable schedules:
/// `pre` hosts [`TerminalSystems::Input`], `update` hosts the
/// [`TerminalSystems::Measure`] → [`TerminalSystems::Process`] chain,
/// and `post` hosts [`TerminalSystems::RenderPrep`] (after
/// `ui_layout_system`). [`Default`] selects `(PreUpdate, Update,
/// PostUpdate)`.
#[derive(Debug)]
pub struct TerminalPlugin {
    /// Schedule hosting [`TerminalSystems::Input`].
    pub pre: InternedScheduleLabel,
    /// Schedule hosting the [`TerminalSystems::Measure`] →
    /// [`TerminalSystems::Process`] chain.
    pub update: InternedScheduleLabel,
    /// Schedule hosting [`TerminalSystems::RenderPrep`].
    pub post: InternedScheduleLabel,
}

impl TerminalPlugin {
    /// Construct a [`TerminalPlugin`] that routes its sets across the
    /// given schedules.
    pub fn new(
        pre: impl ScheduleLabel,
        update: impl ScheduleLabel,
        post: impl ScheduleLabel,
    ) -> Self {
        Self {
            pre: pre.intern(),
            update: update.intern(),
            post: post.intern(),
        }
    }
}

impl Default for TerminalPlugin {
    fn default() -> Self {
        Self::new(PreUpdate, Update, PostUpdate)
    }
}

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TermMsg>();

        app.configure_sets(self.pre, TerminalSystems::Input);
        app.configure_sets(
            self.update,
            (TerminalSystems::Measure, TerminalSystems::Process).chain(),
        );
        app.configure_sets(
            self.post,
            TerminalSystems::RenderPrep.after(ui_layout_system),
        );

        app.add_systems(
            self.update,
            (
                update_font.in_set(TerminalSystems::Measure),
                update_char_width.in_set(TerminalSystems::Measure),
                resize.in_set(TerminalSystems::Measure),
                handle_messages.in_set(TerminalSystems::Process),
                scroll_viewport.in_set(TerminalSystems::Process),
            ),
        );
        app.add_systems(
            self.post,
            refresh_ui.in_set(TerminalSystems::RenderPrep),
        );
    }
}
