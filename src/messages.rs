use bevy::platform::collections::HashMap;

use crate::prelude::*;

/// Re-queue a [`TermScrollMsg`] up to a fixed retry budget; warn and
/// drop after.
///
/// Used by [`apply_scroll`] when the target [`TermInfo`] has not been
/// resolved yet on the same frame the message was written. Removed in
/// the next stage when [`PendingTermInput`]-style attachment replaces
/// the retry loop.
fn retry_message(commands: &mut Commands, msg: TermScrollMsg, reason: &str) {
    if msg.retry_count < 10 {
        // try again next frame
        commands.write_message(TermScrollMsg {
            retry_count: msg.retry_count + 1,
            ..msg
        });
    } else {
        warn!(?msg, reason, "Dropped terminal message");
    }
}

/// Drain [`TermInputMsg`] writes and apply them via the ANSI parser.
///
/// Looks up each message's target [`TermInfo`], builds a [`Grid`],
/// runs the parser/performer, then syncs the grid back into the world.
/// Targets whose [`TermInfo`] cannot be resolved this frame are
/// dropped with a warning (replaced by a pending-attachment scheme in
/// a follow-up). Emits [`TermBufferMutatedMsg`],
/// [`TermCursorMovedMsg`], and [`TermRedrawRequestedMsg`] per
/// affected target.
pub fn process_input(
    mut messages: MessageReader<TermInputMsg>,
    mut commands: Commands,
    q_terminfo: Query<TermInfo>,
    q_lines: Query<(Entity, &VtLine, &VtRowTarget)>,
    q_rows: Query<(Entity, &VtRow)>,
) {
    trace!("process_input");
    let mut to_write: HashMap<Entity, Vec<&TermWrite>> = HashMap::new();
    for msg in messages.read() {
        to_write.entry(msg.target).or_default().extend(&msg.writes);
    }
    for (target, writes) in to_write {
        let terminfo = match q_terminfo.get(target) {
            Ok(t) => t,
            Err(_) => {
                warn!(
                    ?target,
                    write_count = writes.len(),
                    "TermInfo unresolvable for Write target; dropping (a follow-up swaps this for PendingTermInput)"
                );
                continue;
            }
        };
        let mut grid = Grid::new(&terminfo, &q_lines, &q_rows);
        {
            let mut performer = AnsiPerformer::new(&mut grid);
            let mut stream = AnsiParser::new();
            for spawner in writes {
                if let Some(style) = spawner.style {
                    performer.reset_style(style);
                } else if spawner.reset_style {
                    performer.reset_style(VtCellStyle::default());
                }
                for byte in spawner.text.as_bytes() {
                    stream.advance(&mut performer, *byte);
                }
            }
        }
        let cursor = grid.cursor();
        grid.sync(&mut commands);
        commands.write_message(TermBufferMutatedMsg::new(target));
        commands.write_message(TermCursorMovedMsg::new(target, cursor));
        commands.write_message(TermRedrawRequestedMsg::new(target));
    }
}

/// Apply [`TermScrollMsg`] / [`TermJumpToBottomMsg`] to viewport state.
///
/// For each scroll, computes the clamped scroll offset and writes a
/// new [`VtScrollPos`] onto the target. Jumps unconditionally reset
/// the scroll position to the bottom. Emits a
/// [`TermRedrawRequestedMsg`] per affected target.
pub fn apply_scroll(
    mut scrolls: MessageReader<TermScrollMsg>,
    mut jumps: MessageReader<TermJumpToBottomMsg>,
    mut commands: Commands,
    q_terminfo: Query<TermInfo>,
    q_rows: Query<(Entity, &VtRow)>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
) {
    trace!("apply_scroll");
    for msg in scrolls.read() {
        let terminfo = match q_terminfo.get(msg.target) {
            Ok(t) => t,
            Err(_) => {
                retry_message(&mut commands, msg.clone(), "no terminfo");
                continue;
            }
        };
        let num_rows = terminfo
            .rows(&q_rowtargets, &q_rows)
            .collect::<Vec<_>>()
            .len();
        let pos = terminfo
            .scroll_pos
            .saturating_sub_signed(msg.delta)
            .clamp(0, num_rows.saturating_sub(terminfo.size.rows));
        commands.entity(terminfo.id).insert(VtScrollPos(pos));
        commands.write_message(TermRedrawRequestedMsg::new(msg.target));
    }
    for msg in jumps.read() {
        commands.entity(msg.target).insert(VtScrollPos(0));
        commands.write_message(TermRedrawRequestedMsg::new(msg.target));
    }
}

/// Apply [`TermReflowMsg`] by reflowing each unique target's buffer.
///
/// Promoted from a one-shot helper so reflow participates in the
/// `Process` chain like any other consumer. Despawns the existing
/// row/viewport caches, then rebuilds them from the logical lines.
/// Emits [`TermBufferMutatedMsg`] and [`TermRedrawRequestedMsg`] per
/// affected target.
pub fn apply_reflow(
    mut messages: MessageReader<TermReflowMsg>,
    mut commands: Commands,
    q_terminfo: Query<TermInfo>,
    q_lines: Query<(Entity, &VtLine)>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
) {
    trace!("apply_reflow");
    let mut targets: Vec<Entity> = vec![];
    for msg in messages.read() {
        if !targets.contains(&msg.target) {
            targets.push(msg.target);
        }
    }
    for target in targets {
        let terminfo = match q_terminfo.get(target) {
            Ok(t) => t,
            Err(_) => continue,
        };
        // clear terminal display cache (only rows belonging to this terminal)
        for (line_id, _) in terminfo.lines(&q_lines) {
            if let Ok(row_target) = q_rowtargets.get(line_id) {
                for &row_id in row_target.entities() {
                    commands.entity(row_id).despawn();
                }
            }
        }
        commands.entity(target).despawn_related::<VtViewport>();
        // exit early if nothing to do
        if terminfo.size.cols == 0 || terminfo.size.rows == 0 {
            continue;
        }
        // reflow
        let rows = terminfo
            .lines(&q_lines)
            .fold(vec![], |mut res, (line_id, line)| {
                let mut rows = flow_line(&mut commands, &terminfo, line_id, line);
                res.append(&mut rows);
                res
            });
        let row_ids = rows
            .into_iter()
            .rev()
            .skip(terminfo.scroll_pos.0)
            .take(terminfo.size.rows)
            .collect::<Vec<_>>();
        for id in row_ids.into_iter().rev() {
            commands.entity(id).insert(VtViewportRow::new(terminfo.id));
        }
        commands.write_message(TermBufferMutatedMsg::new(target));
        commands.write_message(TermRedrawRequestedMsg::new(target));
    }
}

/// Takes a [`VtLine`] and returns a vec of newly spawned [`VtRow`]s.
fn flow_line(
    commands: &mut Commands,
    terminfo: &TermInfoItem<'_, '_>,
    line_id: Entity,
    line: &VtLine,
) -> Vec<Entity> {
    trace!("flow line");
    let mut res = vec![];
    if terminfo.size.cols == 0 || terminfo.size.rows == 0 {
        return res;
    }
    let mut offset = 0;
    while offset < line.cells().len() {
        let new_row = VtRow::new(line_id, offset);
        let id = commands.spawn(new_row).id();
        res.push(id);
        offset += terminfo.size.cols;
    }
    res
}
