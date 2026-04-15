use bevy::{input::mouse::MouseScrollUnit, text::LineHeight};
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
    q: Query<(Entity, &ComputedNode, &VtCharWidth), Changed<ComputedNode>>,
    mut commands: Commands,
) {
    trace!("update_char_width");
    for (entity, node, cw) in q {
        commands
            .entity(entity)
            .insert(VtCharWidth::new(cw.target(), node.content_size().x));
    }
}

pub fn resize(
    q_ui: Query<
        (
            &VtUi,
            &ComputedNode,
            &TextFont,
            &LineHeight,
            &VtCharWidthTarget,
        ),
        Changed<ComputedNode>,
    >,
    q_width: Query<&VtCharWidth>,
    mut commands: Commands,
) {
    trace!("resize");
    for (vt_ui, node, font, line_height, cw_target) in q_ui.iter() {
        let cw = c!(q_width.get(cw_target.entity()));
        let size = node.size();
        let line_height = match line_height {
            LineHeight::Px(px) => *px,
            LineHeight::RelativeToFont(rel) => rel * font.font_size,
        };
        let cols = (size.x / cw.value()).floor() as usize;
        let rows = (size.y / line_height).floor() as usize;
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
    ui_id: Entity,
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
                    let new = (TextSpan::new(cell.value), style_bundle, ChildOf(ui_id));
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
            ChildOf(ui_id),
        )]
    }
}

/// Translates from [`VtViewportRow`] entities to the [`VtUi`]-based render.
/// Completely clears out the UI, then replaces the [`TextSpan`]s based on
/// the current viewport data.
pub fn refresh_ui(
    q: Query<
        (TermInfo, &VtUiTarget),
        Or<(Changed<VtSize>, Changed<VtViewport>, Changed<VtScrollPos>)>,
    >,
    q_lines: Query<&VtLine>,
    q_viewport: Query<(&VtViewportRow, Option<Ref<VtRow>>)>,
    mut commands: Commands,
) {
    trace!("refresh_ui (spawn textspans)");
    for (terminfo, ui_target) in q {
        let ui_id = ui_target.target();
        commands.entity(ui_id).despawn_children();
        let mut spans = vec![];
        let mut row_count = 0;
        for (_, maybe_row) in q_viewport.iter_many(terminfo.viewport.iter()) {
            row_count += 1;
            let mut new_spans = if let Some(row) = maybe_row {
                let line = c!(q_lines.get(row.line()));
                generate_textspan_ui(&terminfo, ui_id, Some((row.as_ref(), line)))
            } else {
                generate_textspan_ui(&terminfo, ui_id, None)
            };
            spans.append(&mut new_spans);
        }
        // pad
        while row_count < terminfo.size.rows {
            row_count += 1;
            spans.push((
                TextSpan::new(" ".repeat(terminfo.size.cols) + "\n"),
                TextSpanStyleBundle::default(),
                ChildOf(ui_id),
            ));
        }
        commands.spawn_batch(spans);
    }
}

// todo: use input focus instead
pub(crate) fn on_scroll(
    trigger: On<Pointer<Scroll>>,
    q: Query<(&LineHeight, &TextFont, &VtUi)>,
    mut commands: Commands,
) {
    let (line_height, text_font, ui) = r!(q.get(trigger.entity));
    let delta = match trigger.unit {
        MouseScrollUnit::Line => trigger.y,
        MouseScrollUnit::Pixel => {
            let line_height = match line_height {
                LineHeight::Px(line_height) => *line_height,
                LineHeight::RelativeToFont(rel) => rel * text_font.font_size,
            };
            trigger.y / line_height
        }
    };
    commands.write_message(TermMsg::scroll(ui.target(), delta as isize));
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
