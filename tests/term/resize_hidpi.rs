//! Regression test for the trackpad/HiDPI scroll bug fixed in
//! `fix(resize): treat ComputedNode::size as physical pixels on HiDPI`.
//!
//! `bevy_ui::ComputedNode::size()` is in **physical** pixels (Bevy
//! 0.18), but [`LineHeight`] and the [`VtCharWidth`] measurement we
//! compare it against are in **logical** pixels. On a HiDPI display
//! that mismatch made `resize` report ~2× the rows that actually
//! fit, e.g. a 600px-tall window with a 24px line height reported 50
//! rows of viewport. Downstream, `apply_scroll`'s clamp pinned
//! `max_scroll = num_rows.saturating_sub(size.rows)` to 0 and the
//! viewport never moved.
//!
//! Fix (`src/systems.rs`): multiply `ComputedNode::size()` by
//! `ComputedNode::inverse_scale_factor()` in both `resize` and
//! `update_char_width` so all downstream math is in logical pixels.
//!
//! These tests drive `resize` directly via `run_system_once` (same
//! pattern as `vtsize_cw_divide_by_zero.rs`) with a synthetic
//! `ComputedNode` whose `inverse_scale_factor` is varied to simulate
//! different DPI scales.

use bevy::ecs::system::RunSystemOnce;
use bevy::math::Vec2;
use bevy::text::LineHeight;
use bevy::ui::ComputedNode;

use crate::prelude::*;

/// Char-width used in both subtests; chosen so the column arithmetic
/// has obvious answers (800/8 = 100, 400/8 = 50).
const CW_PX: f32 = 8.0;
/// Line height used in both subtests; chosen so the row arithmetic
/// has obvious answers (600/12 = 50, 300/12 = 25).
const LINE_HEIGHT_PX: f32 = 12.0;
/// Physical width of the synthetic UI node.
const PHYSICAL_W: f32 = 800.0;
/// Physical height of the synthetic UI node.
const PHYSICAL_H: f32 = 600.0;

/// Spawn a `Terminal` + `VtUi` whose `ComputedNode` reports
/// `(PHYSICAL_W, PHYSICAL_H)` at the given inverse scale factor,
/// drive a non-zero `VtCharWidth` onto the auto-spawned cw child,
/// run `resize` once, and return the resulting [`VtSize`] written to
/// the terminal (or `None` if `resize` bailed).
fn run_resize_with_scale(inverse_scale_factor: f32) -> Option<(usize, usize)> {
    let mut app = get_test_app();

    let term_id = app.world_mut().spawn(Terminal).id();

    // Overriding `LineHeight` and `TextFont` here defeats the
    // `#[require(...)]` defaults so `resize`'s
    // `LineHeight::RelativeToFont` branch resolves to a known value.
    let vtui_id = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Px(PHYSICAL_W),
                height: Val::Px(PHYSICAL_H),
                ..Default::default()
            },
            VtUi::new(term_id),
            ComputedNode {
                size: Vec2::new(PHYSICAL_W, PHYSICAL_H),
                inverse_scale_factor,
                ..Default::default()
            },
            LineHeight::Px(LINE_HEIGHT_PX),
        ))
        .id();

    // The `VtUi` on-add hook spawns a `VtCharWidth` child via
    // deferred commands; flush so the relationship exists before we
    // try to re-insert a non-zero width on the child.
    app.world_mut().flush();

    let cw_entity = app
        .world()
        .get::<VtCharWidthTarget>(vtui_id)
        .expect("VtUi on_add should have spawned a VtCharWidth child")
        .target();
    // `VtCharWidth` is `#[component(immutable)]`; replace via
    // `insert` rather than `&mut`.
    app.world_mut()
        .entity_mut(cw_entity)
        .insert(VtCharWidth::new(vtui_id, CW_PX));

    app.world_mut()
        .run_system_once(resize)
        .expect("resize ran");

    app.world()
        .get::<VtSize>(term_id)
        .map(|s| (s.cols, s.rows))
}

/// 1× DPI: physical size equals logical size. Verifies the fix did
/// not invert the scaling direction.
#[test]
fn resize_at_1x_dpi_uses_physical_size_unchanged() {
    let (cols, rows) = run_resize_with_scale(1.0).expect("VtSize should have been inserted");
    let expected_cols = (PHYSICAL_W / CW_PX) as usize;
    let expected_rows = (PHYSICAL_H / LINE_HEIGHT_PX) as usize;
    assert_eq!(
        cols, expected_cols,
        "1× DPI cols: expected {expected_cols}, got {cols}",
    );
    assert_eq!(
        rows, expected_rows,
        "1× DPI rows: expected {expected_rows}, got {rows}",
    );
}

/// 2× DPI: a `ComputedNode::size()` of (800, 600) physical pixels
/// corresponds to (400, 300) logical pixels, so the viewport must
/// report 50 cols × 25 rows -- **not** 100 × 50 (the pre-fix bug,
/// which made `max_scroll` saturate to 0 in `apply_scroll`).
#[test]
fn resize_at_2x_dpi_converts_physical_to_logical_pixels() {
    // inverse_scale_factor = 0.5 corresponds to a 2× display.
    let (cols, rows) = run_resize_with_scale(0.5).expect("VtSize should have been inserted");
    let expected_cols = ((PHYSICAL_W * 0.5) / CW_PX) as usize; // 50
    let expected_rows = ((PHYSICAL_H * 0.5) / LINE_HEIGHT_PX) as usize; // 25
    assert_eq!(
        cols, expected_cols,
        "2× DPI cols: expected {expected_cols} (logical), got {cols} -- pre-fix would have been \
         {}",
        (PHYSICAL_W / CW_PX) as usize,
    );
    assert_eq!(
        rows, expected_rows,
        "2× DPI rows: expected {expected_rows} (logical), got {rows} -- pre-fix would have been \
         {}",
        (PHYSICAL_H / LINE_HEIGHT_PX) as usize,
    );
}
