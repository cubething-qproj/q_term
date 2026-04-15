use bevy::ui::ui_layout_system;

use crate::prelude::*;

/// [`SystemSet`] for all terminal-related systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerminalSystems;

/// The primary plugin for q_term
#[derive(Default, Debug)]
pub struct TerminalPlugin;
impl Plugin for TerminalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TermMsg>();
        app.add_systems(
            PostUpdate,
            // no matter how we structure this - before or after ui layout -
            // there will be a frame delay between updating the terminal size
            // and laying out the ui due to char width measurement
            (
                update_font,
                update_char_width,
                resize,
                scroll_viewport,
                handle_messages,
                refresh_ui,
            )
                .chain()
                .after(ui_layout_system)
                .in_set(TerminalSystems),
        );
    }
}
