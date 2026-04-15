use bevy::platform::collections::HashMap;

use crate::prelude::*;

fn retry_message(commands: &mut Commands, msg: TermMsg, reason: &str) {
    if msg.retry_count < 10 {
        // try again next frame
        commands.write_message(TermMsg {
            retry_count: msg.retry_count + 1,
            ..msg
        });
    } else {
        warn!(?msg, reason, "Dropped terminal message");
    }
}

pub fn handle_messages(
    mut messages: MessageReader<TermMsg>,
    mut commands: Commands,
    q_terminfo: Query<TermInfo>,
    q_lines: Query<(Entity, &VtLine, &VtRowTarget)>,
    q_rows: Query<(Entity, &VtRow)>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
) {
    trace!("handle window message");
    let mut to_reflow = vec![];
    let mut to_write = HashMap::<Entity, Vec<&TermWrite>>::new();
    for msg in messages.read() {
        match &msg.kind {
            TermMsgKind::Reflow => {
                if !to_reflow.contains(&msg.target) {
                    to_reflow.push(msg.target);
                }
            }
            // TODO Make a system to respond to changes in VtScrollPos
            TermMsgKind::Scroll(dir) => {
                let terminfo = q_terminfo.get(msg.target);
                if terminfo.is_err() {
                    retry_message(&mut commands, msg.clone(), "no terminfo");
                    continue;
                }
                let terminfo = terminfo.unwrap();
                let num_rows = terminfo
                    .rows(&q_rowtargets, &q_rows)
                    .collect::<Vec<_>>()
                    .len();
                let pos = terminfo
                    .scroll_pos
                    .saturating_sub_signed(*dir)
                    .clamp(0, num_rows.saturating_sub(terminfo.size.rows));
                commands.entity(terminfo.id).insert(VtScrollPos(pos));
            }
            TermMsgKind::JumpToBottom => {
                commands.entity(msg.target).insert(VtScrollPos(0));
            }
            TermMsgKind::Write(spawners) => {
                to_write.entry(msg.target).or_default().extend(spawners);
            }
        }
    }
    for target in to_reflow.into_iter() {
        commands.run_system_cached_with(reflow, target);
    }
    for (target, vec) in to_write {
        // do NOT want to clear if we can't get terminfo.
        // try again next frame.
        let terminfo = r!(q_terminfo.get(target));
        let mut grid = Grid::new(&terminfo, &q_lines, &q_rows);
        {
            let mut performer = AnsiPerformer::new(&mut grid);
            let mut stream = AnsiParser::new();
            for spawner in vec {
                // parse ansi
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
        grid.sync(&mut commands);
    }
    messages.clear();
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

/// Reflows the entire underlying buffer.
fn reflow(
    id: In<Entity>,
    terminfo: Query<TermInfo>,
    q_lines: Query<(Entity, &VtLine)>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
    mut commands: Commands,
) {
    trace!("Reflow");
    let terminfo = r!(terminfo.get(*id));
    // clear terminal display cache (only rows belonging to this terminal)
    for (line_id, _) in terminfo.lines(&q_lines) {
        if let Ok(row_target) = q_rowtargets.get(line_id) {
            for &row_id in row_target.entities() {
                commands.entity(row_id).despawn();
            }
        }
    }
    commands.entity(*id).despawn_related::<VtViewport>();
    // exit early if nothing to do
    if terminfo.size.cols == 0 || terminfo.size.rows == 0 {
        return;
    }
    // reflow
    let rows = terminfo
        .lines(&q_lines)
        .fold(vec![], |mut res, (line_id, line)| {
            let mut rows = flow_line(&mut commands, &terminfo, line_id, line);
            res.append(&mut rows);
            res
        });
    trace!("spawned rows");
    // spawn
    let row_ids = rows
        .into_iter()
        .rev()
        .skip(terminfo.scroll_pos.0)
        .take(terminfo.size.rows)
        .collect::<Vec<_>>();
    for id in row_ids.into_iter().rev() {
        commands.entity(id).insert(VtViewportRow::new(terminfo.id));
    }
    trace!("spawned viewport rows");
}
