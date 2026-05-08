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

/// The primary plugin for q_term
#[derive(Default, Debug)]
pub struct TerminalPlugin;
impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TermMsg>();
        // no matter how we structure this - before or after ui layout -
        // there will be a frame delay between updating the terminal size
        // and laying out the ui due to char width measurement
        app.configure_sets(
            PostUpdate,
            (
                TerminalSystems::Measure,
                TerminalSystems::Process,
                TerminalSystems::RenderPrep,
            )
                .chain()
                .after(ui_layout_system),
        );
        app.add_systems(
            PostUpdate,
            (
                update_font.in_set(TerminalSystems::Measure),
                update_char_width.in_set(TerminalSystems::Measure),
                resize.in_set(TerminalSystems::Measure),
                handle_messages.in_set(TerminalSystems::Process),
                scroll_viewport.in_set(TerminalSystems::Process),
                refresh_ui.in_set(TerminalSystems::RenderPrep),
            ),
        );
    }
}
