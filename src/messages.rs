use bevy::platform::collections::HashMap;

use crate::prelude::*;

/// Re-emit pending input/scroll messages once their target resolves.
///
/// Iterates entities carrying [`PendingTermInput`] or
/// [`PendingTermScroll`]. For each, attempts to resolve the target's
/// [`TermInfo`]; on success, removes the pending component and
/// re-emits the corresponding [`TermStdOut`] / [`TermScrollMsg`].
/// Entities whose [`TermInfo`] is still unresolvable retain their
/// pending component and are retried next frame.
///
/// Registered as the first system in [`TerminalSystems::Process`] so
/// the re-emitted messages are observed by `process_input` and
/// `apply_scroll` later in the same chain. Re-emits use
/// [`MessageWriter`] system params; entity cleanup uses [`Commands`].
pub fn drain_pending(
    mut commands: Commands,
    mut input: MessageWriter<TermStdOut>,
    mut scroll: MessageWriter<TermScrollMsg>,
    q_terminfo: Query<TermInfo>,
    q_pending_input: Query<(Entity, &PendingTermInput)>,
    q_pending_scroll: Query<(Entity, &PendingTermScroll)>,
) {
    trace!("drain_pending");
    for (entity, pending) in &q_pending_input {
        if q_terminfo.get(entity).is_ok() {
            commands.entity(entity).remove::<PendingTermInput>();
            input.write(TermStdOut {
                term: entity,
                writes: pending.writes.clone(),
            });
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

/// Drain [`TermStdOut`] writes and apply them via the ANSI parser.
///
/// Looks up each message's target [`TermInfo`], builds a [`Grid`],
/// runs the parser/performer, then syncs the grid back into the world.
/// Targets whose [`TermInfo`] cannot be resolved this frame have their
/// pending writes attached as a [`PendingTermInput`] component on the
/// target; `drain_pending` re-emits them once the target resolves.
/// Emits [`TermRedrawRequestedMsg`] per affected target.
pub fn process_input(
    mut stdout: MessageReader<TermStdOut>,
    mut stderr: MessageReader<TermStdErr>,
    mut commands: Commands,
    mut redraw_requested: MessageWriter<TermRedrawRequestedMsg>,
    mut stdin_writer: MessageWriter<'_, TermStdIn>,
    cap: Res<PendingTermInputCap>,
    q_terminfo: Query<TermInfo>,
    q_lines: Query<(Entity, &VtLine, &VtRowTarget)>,
    q_rows: Query<(Entity, &VtRow)>,
    q_fg: Query<Entity, With<ForegroundProcess>>,
) {
    trace!("process_input");
    let mut to_write: HashMap<Entity, Vec<&TermWrite>> = HashMap::new();
    for msg in stdout.read() {
        to_write.entry(msg.term).or_default().extend(&msg.writes);
    }
    for msg in stderr.read() {
        to_write.entry(msg.term).or_default().extend(&msg.writes);
    }
    let cap_bytes = cap.bytes;
    for (target, writes) in to_write {
        let terminfo = match q_terminfo.get(target) {
            Ok(t) if t.size.cols > 0 && t.size.rows > 0 => t,
            _ => {
                let writes_owned: Vec<TermWrite> = writes.iter().map(|w| (*w).clone()).collect();
                commands
                    .entity(target)
                    .entry::<PendingTermInput>()
                    .or_default()
                    .and_modify(move |mut pending| {
                        pending.push_writes(writes_owned, cap_bytes);
                    });
                continue;
            }
        };
        let fg_job = terminfo
            .shell_target
            .map(|t| t.target())
            .and_then(|shell_id| q_fg.get(shell_id).ok())
            .unwrap_or(Entity::PLACEHOLDER);

        let mut grid = Grid::new(&terminfo, fg_job, &q_lines, &q_rows);
        {
            let mut performer = AnsiPerformer::new(&mut grid, &mut stdin_writer, target);
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
        grid.sync(&mut commands);
        redraw_requested.write(TermRedrawRequestedMsg::new(target));
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
