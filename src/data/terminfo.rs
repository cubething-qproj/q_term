//! Public [`Terminal`] API.

use crate::prelude::*;
use bevy::ecs::query::QueryData;

/// Public API and query helpers for the [`Terminal`] entity.
#[derive(QueryData, Debug)]
pub struct TermInfo {
    pub id: Entity,
    pub cursor: &'static VtCursor,
    pub modes: &'static VtModes,
    pub line_target: &'static VtLineTarget,
    pub viewport: &'static VtViewport,
    pub size: &'static VtSize,
    pub scroll_pos: &'static VtScrollPos,
    pub tab_stop: &'static VtTabStop,
    pub fg_process: Option<&'static VtForegroundProcessTarget>,
}
impl<'w, 's> TermInfoItem<'w, 's> {
    #[inline(always)]
    pub fn lines<'a>(
        &self,
        q_lines: &'a Query<(Entity, &VtLine)>,
    ) -> impl Iterator<Item = (Entity, &'a VtLine)> {
        q_lines.iter_many(self.line_target.iter())
    }

    #[inline(always)]
    pub fn viewport_rows<'a>(
        &self,
        q_viewport_rows: &'a Query<(Entity, &VtViewportRow)>,
    ) -> impl Iterator<Item = (Entity, &'a VtViewportRow)> {
        q_viewport_rows.iter_many(self.viewport.iter())
    }

    pub fn rows<'a>(
        &self,
        q_row_targets: &'a Query<&VtRowTarget, With<VtLine>>,
        q_rows: &'a Query<(Entity, &VtRow)>,
    ) -> impl Iterator<Item = (Entity, &'a VtRow)> {
        self.line_target.iter().flat_map(|line_id| {
            let target = r!(q_row_targets.get(line_id).ok());
            q_rows.iter_many(target.entities()).collect::<Vec<_>>()
        })
    }

    /// Write text into this terminal's buffer. Supports ANSI.
    pub fn write(&self, commands: &mut Commands, from: Entity, value: impl ToString) {
        commands.write_message(TermStdOut {
            from,
            message: vec![TermWrite::new(value)],
            term: self.id,
        });
    }
    /// Write rich text spans into this terminal's buffer.
    pub fn write_spans(&self, commands: &mut Commands, from: Entity, spans: Vec<TermWrite>) {
        commands.write_message(TermStdOut {
            from,
            message: spans,
            term: self.id,
        });
    }
}
