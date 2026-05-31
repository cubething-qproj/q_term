use bevy::{
    input::mouse::MouseScrollUnit,
    text::{LineHeight, TextLayoutInfo},
};
use itertools::Itertools;

use crate::prelude::*;

// The font propogation / char width calculation is not ideal.
// This incurs a frame-delay between changing the font and rewrapping.
// denote a frame change with ';', successive events with '->', and concurrent events with ','
// update_font -> resize ; update_char_width -> resize
// This happens because the TerminalCharWidth entity's ComputedNode is updated the frame _after_ updating
// the text element. Cannot manually trigger a rerender here.
// We _could_ make the parent node invisible, then override the inserted text span's Visibility components.
//

/// Propogates font changes from the [`VtUi`] to the targeted TerminalCharWidth entity.
pub fn update_font(
    q_font: Query<(&TextFont, &VtCharWidthTarget), Changed<TextFont>>,
    q_cw: Query<Entity, With<VtCharWidth>>,
    mut commands: Commands,
) {
    trace!("update_font");
    for (font, target) in q_font {
        let cw_id = c!(q_cw.get(target.target()));
        commands.entity(cw_id).insert(font.clone());
    }
}

/// Measure the character width of the monospace font by using a detached,
/// invisible Node containing a single Text(" ")
pub fn update_char_width(
    q: Query<(Entity, &ComputedNode, &VtCharWidth), Changed<TextLayoutInfo>>,
    mut commands: Commands,
) {
    for (entity, node, cw) in q {
        // Convert physical -> logical pixels so this matches `LineHeight`
        // and the logical-pixel UI size used in `resize`.
        let width_logical = node.size().x * node.inverse_scale_factor();
        commands
            .entity(entity)
            .insert(VtCharWidth::new(cw.target(), width_logical));
    }
}

pub fn resize(
    q_ui: Query<(
        &VtUi,
        &ComputedNode,
        &TextFont,
        &LineHeight,
        &VtCharWidthTarget,
    )>,
    q_width: Query<&VtCharWidth>,
    q_size: Query<&VtSize>,
    mut commands: Commands,
) {
    trace!("resize");
    for (vt_ui, node, font, line_height, cw_target) in q_ui.iter() {
        let cw = c!(q_width.get(cw_target.entity()));
        // `ComputedNode::size()` is in physical pixels; convert to logical
        // pixels to match `LineHeight`/char-width units. Without this, a
        // HiDPI display reports e.g. 2x the rows that actually fit, which
        // makes `apply_scroll` clamp `max_scroll` to 0 and the viewport
        // never moves.
        let size = node.size() * node.inverse_scale_factor();
        let line_height = match line_height {
            LineHeight::Px(px) => *px,
            LineHeight::RelativeToFont(rel) => rel * font.font_size,
        };
        let cw_value = cw.value();
        c!(cw_value > 0.0);
        c!(line_height > 0.0);
        let cols = (size.x / cw_value).floor() as usize;
        let rows = (size.y / line_height).floor() as usize;
        let target = vt_ui.target();
        if let Ok(current) = q_size.get(target)
            && current.cols == cols
            && current.rows == rows
        {
            continue;
        }
        commands
            .entity(vt_ui.target())
            .insert(VtSize { cols, rows });
    }
}

#[derive(Debug, Bundle, PartialEq, Clone, Copy)]
struct TextSpanStyleBundle {
    color: TextColor,
    bg: TextBackgroundColor,
}
impl Default for TextSpanStyleBundle {
    fn default() -> Self {
        Self {
            color: TextColor(Color::WHITE),
            bg: TextBackgroundColor(Color::BLACK),
        }
    }
}

/// Given a row and its corresponding lines, spawn the text spans.
fn generate_textspan_ui(
    terminfo: &TermInfoItem,
    grid_id: Entity,
    row_and_line: Option<(&VtRow, &VtLine)>,
) -> Vec<(TextSpan, TextSpanStyleBundle, ChildOf)> {
    if let Some((row, line)) = row_and_line {
        trace!("spawn_row_ui:: {:?} >>{:?}", line.as_string(), row.offset);
        let mut spans = line
            .cells()
            .iter()
            .skip(row.offset)
            .take(terminfo.size.cols)
            .copied()
            .filter(|cell| cell.value != '\n')
            .pad_using(terminfo.size.cols, |_| VtCell::default())
            .fold(
                Vec::<(TextSpan, TextSpanStyleBundle, ChildOf)>::new(),
                |mut spans, cell| {
                    let color = TextColor(cell.style.color);
                    let bg = TextBackgroundColor(cell.style.background);
                    let style_bundle = TextSpanStyleBundle { color, bg };
                    let new = (TextSpan::new(cell.value), style_bundle, ChildOf(grid_id));
                    if let Some(last) = spans.last_mut() {
                        if last.1 != style_bundle {
                            spans.push(new);
                        } else {
                            last.0.0.push(cell.value);
                        }
                    } else {
                        spans.push(new);
                    };
                    spans
                },
            );
        if let Some((span, _, _)) = spans.last_mut()
            && !span.0.ends_with('\n')
        {
            span.0.push('\n')
        }
        spans
    } else {
        vec![(
            TextSpan::new(" ".repeat(terminfo.size.cols)),
            TextSpanStyleBundle::default(),
            ChildOf(grid_id),
        )]
    }
}

/// Translates from [`VtViewportRow`] entities to the [`VtUi`]-based render.
/// Drains [`TermRedrawRequestedMsg`] and rebuilds the [`TextSpan`]
/// children for each affected terminal's UI target. Targets are
/// de-duplicated within a frame.
pub fn refresh_ui(
    mut redraws: MessageReader<TermRedrawRequestedMsg>,
    q: Query<(TermInfo, &VtUiTarget)>,
    q_grid: Query<&VtUiGridTarget, With<VtUi>>,
    q_lines: Query<&VtLine>,
    q_viewport: Query<(&VtViewportRow, Option<Ref<VtRow>>)>,
    mut commands: Commands,
) {
    trace!("refresh_ui (spawn textspans)");
    let mut targets: Vec<Entity> = vec![];
    for msg in redraws.read() {
        if !targets.contains(&msg.term) {
            targets.push(msg.term);
        }
    }
    for target in targets {
        let (terminfo, ui_target) = match q.get(target) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ui_id = ui_target.target();
        let grid_id = match q_grid.get(ui_id) {
            Ok(g) => g.target(),
            Err(_) => continue,
        };
        commands.entity(grid_id).despawn_children();
        let mut spans = vec![];
        let mut row_count = 0;
        for (_, maybe_row) in q_viewport.iter_many(terminfo.viewport.iter()) {
            row_count += 1;
            let mut new_spans = if let Some(row) = maybe_row {
                let line = c!(q_lines.get(row.line()));
                generate_textspan_ui(&terminfo, grid_id, Some((row.as_ref(), line)))
            } else {
                generate_textspan_ui(&terminfo, grid_id, None)
            };
            spans.append(&mut new_spans);
        }
        // pad
        while row_count < terminfo.size.rows {
            row_count += 1;
            spans.push((
                TextSpan::new(" ".repeat(terminfo.size.cols) + "\n"),
                TextSpanStyleBundle::default(),
                ChildOf(grid_id),
            ));
        }
        commands.spawn_batch(spans);
    }
}

// todo: use input focus instead
pub(crate) fn on_scroll(
    trigger: On<Pointer<Scroll>>,
    mut q: Query<(
        &LineHeight,
        &TextFont,
        &VtUi,
        Option<&mut VtScrollAccumulator>,
    )>,
    sensitivity: Res<VtScrollSensitivity>,
    mut commands: Commands,
) {
    let (line_height, text_font, ui, acc) = r!(q.get_mut(trigger.entity));
    let line_delta = match trigger.unit {
        MouseScrollUnit::Line => trigger.y * sensitivity.line,
        MouseScrollUnit::Pixel => {
            let line_height = match line_height {
                LineHeight::Px(line_height) => *line_height,
                LineHeight::RelativeToFont(rel) => rel * text_font.font_size,
            };
            // Guard against zero/negative line height producing NaN/Inf.
            if line_height > 0.0 {
                (trigger.y / line_height) * sensitivity.pixel
            } else {
                0.0
            }
        }
    };

    // Accumulate fractional line deltas so trackpad pixel events (which are
    // routinely well under one line each) eventually trigger a scroll.
    let prev = acc.as_deref().map(|a| a.0).unwrap_or(0.0);
    let total = prev + line_delta;
    let whole = total.trunc();
    let remainder = total - whole;

    match acc {
        Some(mut a) => a.0 = remainder,
        None => {
            commands
                .entity(trigger.entity)
                .insert(VtScrollAccumulator(remainder));
        }
    }

    let whole_i = whole as isize;
    if whole_i != 0 {
        commands.write_message(TermScrollMsg::new(ui.target(), whole_i));
    }
}

pub(crate) fn scroll_viewport(
    mut commands: Commands,
    terminfo: Query<TermInfo, Changed<VtScrollPos>>,
    q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
    q_rows: Query<(Entity, &VtRow)>,
) {
    trace!("scroll_viewport");
    for terminfo in terminfo {
        commands
            .entity(terminfo.id)
            .remove_related::<VtViewportRow>(terminfo.viewport);
        let rows = terminfo.rows(&q_rowtargets, &q_rows).collect::<Vec<_>>();
        let to_insert = rows
            .into_iter()
            .rev()
            .skip(**terminfo.scroll_pos)
            .take(terminfo.size.rows)
            .collect::<Vec<_>>();
        to_insert.into_iter().rev().for_each(|(entity, _row)| {
            commands
                .entity(entity)
                .insert(VtViewportRow::new(terminfo.id));
        });
    }
}

pub fn update_cursor_display(
    q_cursor: Query<Ref<VtCursor>>,
    q_width: Query<Ref<VtCharWidth>>,
    q_ui: Query<(&VtUi, Ref<TextFont>, Ref<LineHeight>, &VtCharWidthTarget)>,
    mut q_cursor_ui: Query<(&ChildOf, &mut Node, Ref<VtCursorStyle>), With<VtUiCursor>>,
) {
    for (childof, mut node, style) in q_cursor_ui.iter_mut() {
        let (ui, font, lh, cwt) = c!(q_ui.get(childof.parent()));
        let width = c!(q_width.get(cwt.target()));
        let cursor = c!(q_cursor.get(ui.target()));

        if !cursor.is_changed()
            && !width.is_changed()
            && !lh.is_changed()
            && !font.is_changed()
            && !style.is_changed()
        {
            continue;
        }
        let width = width.value();
        let height = match *lh {
            LineHeight::Px(h) => h,
            LineHeight::RelativeToFont(pct) => pct * font.font_size,
        };
        let left = cursor.col as f32 * width;
        let mut top = cursor.row as f32 * height;
        let (width, height) = match *style {
            VtCursorStyle::Block => (width, height),
            VtCursorStyle::Beam => ((width / 8.).max(1.), height),
            VtCursorStyle::Underline => {
                top += 7. * height / 8.;
                (width, (height / 8.).max(1.))
            }
        };
        *node = Node {
            position_type: PositionType::Absolute,
            left: px(left),
            top: px(top),
            width: px(width),
            height: px(height),
            ..Default::default()
        };
    }
}

pub fn flash_cursor(
    time: Res<Time>,
    mut cursor: Query<
        (
            &ChildOf,
            &mut VtStrobeTimer,
            &VtCursorColor,
            &mut BackgroundColor,
        ),
        With<VtUiCursor>,
    >,
    q_ui: Query<&VtUi>,
    q_modes: Query<&VtModes>,
) {
    for (childof, mut timer, color, mut bg_color) in cursor.iter_mut() {
        // DECTCEM (`CSI ? 25 l`) hides the cursor. This must win over
        // the zero-duration "blink disabled" sentinel below — a user
        // who both disabled blinking and hid the cursor wants the
        // cursor hidden, not pinned visible.
        let dectcem = q_ui
            .get(childof.parent())
            .ok()
            .and_then(|ui| q_modes.get(ui.target()).ok())
            .map(|m| m.dectcem)
            .unwrap_or(true);
        if !dectcem {
            if bg_color.0 != Color::NONE {
                bg_color.0 = Color::NONE;
            }
            continue;
        }
        // Zero-duration timer means "blink disabled": keep the cursor
        // visible and skip ticking (a zero-period Timer would just_finish
        // every frame).
        if timer.duration().is_zero() {
            if bg_color.0 != **color {
                bg_color.0 = **color;
            }
            continue;
        }
        timer.tick(time.delta());
        if timer.just_finished() {
            if bg_color.0 == Color::NONE {
                bg_color.0 = **color
            } else {
                bg_color.0 = Color::NONE
            }
        }
    }
}
