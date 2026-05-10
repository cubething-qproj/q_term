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
/// `ui_layout_system`).
#[derive(Debug)]
pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TermInputMsg>();
        app.add_message::<TermScrollMsg>();
        app.add_message::<TermJumpToBottomMsg>();
        app.add_message::<TermReflowMsg>();
        app.add_message::<TermBufferMutatedMsg>();
        app.add_message::<TermCursorMovedMsg>();
        app.add_message::<TermRedrawRequestedMsg>();
        app.add_message::<TermFocusChangedMsg>();

        app.add_systems(
            Update,
            (
                (update_font, update_char_width, resize).in_set(TerminalSystems::Measure),
                (
                    drain_pending,
                    process_input,
                    apply_scroll,
                    apply_reflow,
                    scroll_viewport,
                )
                    .chain()
                    .in_set(TerminalSystems::Process),
            )
                // Outer `.chain()` orders the `Measure` set before the
                // `Process` set. The `TerminalSystems` enum carries no
                // intrinsic ordering between sets, so this is
                // load-bearing despite the inner chain inside `Process`.
                .chain(),
        );
        app.add_systems(
            PostUpdate,
            refresh_ui
                .after(ui_layout_system)
                .in_set(TerminalSystems::RenderPrep),
        );
    }
}
