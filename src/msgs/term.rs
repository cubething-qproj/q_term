//! Buffered message handling.

use bevy::platform::collections::HashMap;

use crate::prelude::*;

/// Remove terminal-local transient state after [`Terminal`] removal.
pub fn cleanup_removed_terminals(
    mut commands: Commands,
    mut removed: RemovedComponents<Terminal>,
    q_terminals: Query<(), With<Terminal>>,
) {
    for entity in removed.read() {
        if q_terminals.contains(entity) {
            continue;
        }
        if let Ok(mut entity) = commands.get_entity(entity) {
            entity.remove::<(
                PendingVtWrites,
                PendingTermScroll,
                VtParserState,
                VtRenderState,
            )>();
        }
    }
}

/// Re-emit pending ingress and scroll messages once their terminal is ready.
pub fn drain_pending(
    mut commands: Commands,
    mut input: MessageWriter<VtWriteMsg>,
    mut scroll: MessageWriter<TermScrollMsg>,
    q_terminfo: Query<TermInfo>,
    q_pending_input: Query<(Entity, &PendingVtWrites)>,
    q_pending_scroll: Query<(Entity, &PendingTermScroll)>,
) {
    trace!("drain_pending");
    for (entity, pending) in &q_pending_input {
        if q_terminfo
            .get(entity)
            .is_ok_and(|term| term.size.cols > 0 && term.size.rows > 0)
        {
            commands.entity(entity).remove::<PendingVtWrites>();
            for msg in pending.chunks() {
                input.write(msg.clone());
            }
        }
    }
    for (entity, pending) in &q_pending_scroll {
        if q_terminfo.get(entity).is_ok() {
            commands.entity(entity).remove::<PendingTermScroll>();
            scroll.write(TermScrollMsg {
                term: entity,
                delta: pending.delta,
            });
        }
    }
}

/// Drain [`VtWriteMsg`] bytes and apply them through persistent VT state.
pub fn process_input(
    mut writes: MessageReader<VtWriteMsg>,
    mut commands: Commands,
    mut redraw_requested: MessageWriter<TermRedrawRequestedMsg>,
    mut reply_writer: MessageWriter<'_, VtReplyMsg>,
    output_policy: Res<BackgroundTerminalOutput>,
    pending_cap: Res<PendingVtWriteCap>,
    q_terminfo: Query<TermInfo>,
    mut q_parser: Query<(&mut VtParserState, &mut VtRenderState)>,
    q_lines: Query<(Entity, &VtLine, &VtRowTarget)>,
    q_rows: Query<(Entity, &VtRow)>,
) {
    trace!("process_input");
    let mut to_write: HashMap<Entity, Vec<&VtWriteMsg>> = HashMap::new();
    for msg in writes.read() {
        to_write.entry(msg.term).or_default().push(msg);
    }

    for (target_term, messages) in to_write {
        let Ok(terminfo) = q_terminfo.get(target_term) else {
            warn!(?target_term, "discarding write to closed terminal");
            continue;
        };
        if terminfo.size.cols == 0 || terminfo.size.rows == 0 {
            let cap = pending_cap.0;
            let messages = messages.into_iter().cloned().collect::<Vec<_>>();
            commands
                .entity(target_term)
                .entry::<PendingVtWrites>()
                .or_default()
                .and_modify(move |mut pending| {
                    for msg in messages {
                        pending.push(msg, cap);
                    }
                });
            continue;
        }

        let foreground = terminfo.fg_process.map(VtForegroundProcessTarget::process);
        let admitted = messages.into_iter().filter(|msg| {
            *output_policy == BackgroundTerminalOutput::Allow
                || msg.from.is_none()
                || foreground.is_none()
                || msg.from == foreground
        });
        let Ok((mut parser, mut render_state)) = q_parser.get_mut(target_term) else {
            warn!(?target_term, "terminal is missing parser state");
            continue;
        };

        let mut grid = Grid::new(&terminfo, &q_lines, &q_rows);
        {
            let mut performer =
                AnsiPerformer::new(&mut grid, &mut render_state, &mut reply_writer, target_term);
            for msg in admitted {
                for &byte in &msg.bytes {
                    parser.0.advance(&mut performer, byte);
                }
            }
        }
        grid.sync(&mut commands);
        redraw_requested.write(TermRedrawRequestedMsg::new(target_term));
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
    mut redraw_requested: MessageWriter<TermRedrawRequestedMsg>,
    q_terminfo: Query<TermInfo>,
    q_rows: Query<(Entity, &VtRow)>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
) {
    trace!("apply_scroll");
    for msg in scrolls.read() {
        let terminfo = match q_terminfo.get(msg.term) {
            Ok(t) => t,
            Err(_) => {
                let delta = msg.delta;
                commands
                    .entity(msg.term)
                    .entry::<PendingTermScroll>()
                    .or_default()
                    .and_modify(move |mut pending| {
                        pending.add_delta(delta);
                    });
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
        if pos != terminfo.scroll_pos.0 {
            commands.entity(terminfo.id).insert(VtScrollPos(pos));
            redraw_requested.write(TermRedrawRequestedMsg::new(msg.term));
        }
    }
    for msg in jumps.read() {
        let terminfo = match q_terminfo.get(msg.term) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if terminfo.scroll_pos.0 != 0 {
            commands.entity(msg.term).insert(VtScrollPos(0));
            redraw_requested.write(TermRedrawRequestedMsg::new(msg.term));
        }
    }
}

/// Apply [`TermReflowMsg`] by reflowing each unique target's buffer.
///
/// Promoted from a one-shot helper so reflow participates in the
/// `Process` chain like any other consumer. Despawns the existing
/// row/viewport caches, then rebuilds them from the logical lines.
/// Emits [`TermRedrawRequestedMsg`] per affected target.
pub fn apply_reflow(
    mut messages: MessageReader<TermReflowMsg>,
    mut commands: Commands,
    mut redraw_requested: MessageWriter<TermRedrawRequestedMsg>,
    q_terminfo: Query<TermInfo>,
    q_lines: Query<(Entity, &VtLine)>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
) {
    trace!("apply_reflow");
    let mut targets: Vec<Entity> = vec![];
    for msg in messages.read() {
        if !targets.contains(&msg.term) {
            targets.push(msg.term);
        }
    }
    for target in targets {
        let terminfo = match q_terminfo.get(target) {
            Ok(t) => t,
            Err(_) => continue,
        };
        // Bail before touching any row state when the terminal has
        // no displayable area. Despawning rows here -- as we used to
        // do unconditionally -- trips Bevy's relationship on_replace
        // hook (`bevy_ecs::relationship::Relationship::on_replace`):
        // when a `VtRowTarget` collection is drained the component
        // is removed from the line entity. The early-exit then skips
        // `flow_line`, leaving every `VtLine` without a
        // `VtRowTarget` and the per-frame `r!()` bail at
        // `q_term/active/src/data.rs:40` fires forever.
        //
        // The first `TermReflowMsg` after `Terminal` spawn always
        // carries size 0x0 (the `#[require(VtSize)]` default fires
        // `VtSize::on_insert` before `resize` has a real layout to
        // measure), so this path is hit on every cold start. Holding
        // the despawn until we know we will rebuild keeps the
        // invariant intact through the size-0 transient; when a real
        // size lands the next reflow despawns and rebuilds normally.
        if terminfo.size.cols == 0 || terminfo.size.rows == 0 {
            continue;
        }
        // clear terminal display cache (only rows belonging to this terminal)
        for (line_id, _) in terminfo.lines(&q_lines) {
            if let Ok(row_target) = q_rowtargets.get(line_id) {
                for &row_id in row_target.entities() {
                    commands.entity(row_id).despawn();
                }
            }
        }
        commands.entity(target).despawn_related::<VtViewport>();
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
        redraw_requested.write(TermRedrawRequestedMsg::new(target));
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
    // Always spawn at least one row per line. An empty line still
    // occupies a slot in the visual grid, and -- more importantly -- if
    // we leave a `VtLine` with zero `VtRow`s, Bevy's relationship
    // on_replace hook removes the now-empty `VtRowTarget` (see
    // `bevy_ecs::relationship::Relationship::on_replace`). That breaks
    // the invariant `terminfo.rows()` (q_term/active/src/data.rs:40)
    // relies on and produces a per-frame bail.
    let mut offset = 0;
    loop {
        let new_row = VtRow::new(line_id, offset);
        let id = commands.spawn(new_row).id();
        res.push(id);
        offset += terminfo.size.cols;
        if offset >= line.cells().len() {
            break;
        }
    }
    res
}
