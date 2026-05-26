//! Cursor-overlay smoke tests for the `VtUi` frontend.
//!
//! Covers the bare invariants of the cursor wiring:
//!
//! 1. `VtUi::on_add` spawns a [`VtUiGrid`] and a [`VtUiCursor`] as
//!    children of the `VtUi` entity, in that order (grid first so the
//!    cursor renders on top of the grid's text spans).
//! 2. The [`VtUiCursor`] on-insert hook seeds [`BackgroundColor`] from
//!    the required [`VtCursorColor`] so the cursor is visible on the
//!    first frame rather than waiting up to one strobe period for
//!    [`flash_cursor`] to flip it on.
//! 3. A zero-duration [`VtStrobeTimer`] disables the blink:
//!    [`flash_cursor`] keeps the cursor visible and does not tick the
//!    timer (a zero-period repeating timer would otherwise
//!    `just_finish` every frame).
//! 4. [`refresh_ui`] rebuilds the grid's text spans without
//!    despawning the sibling cursor entity.

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use crate::prelude::*;

/// Spawn a `Terminal` + `VtUi`, flush deferred commands, and return
/// `(term_id, vtui_id)`.
fn spawn_term(app: &mut App) -> (Entity, Entity) {
    let term_id = app.world_mut().spawn(Terminal).id();
    let vtui_id = app.world_mut().spawn(VtUi::new(term_id)).id();
    app.world_mut().flush();
    (term_id, vtui_id)
}

/// Resolve the auto-spawned [`VtUiCursor`] and [`VtUiGrid`] children
/// of a [`VtUi`] entity.
fn find_cursor_and_grid(app: &App, vtui_id: Entity) -> (Entity, Entity) {
    let mut cursor = None;
    let mut grid = None;
    let children = app
        .world()
        .get::<Children>(vtui_id)
        .expect("VtUi should have children after flush");
    for child in children.iter() {
        if app.world().get::<VtUiCursor>(child).is_some() {
            cursor = Some(child);
        }
        if app.world().get::<VtUiGrid>(child).is_some() {
            grid = Some(child);
        }
    }
    (
        cursor.expect("VtUi on_add should have spawned a VtUiCursor child"),
        grid.expect("VtUi on_add should have spawned a VtUiGrid child"),
    )
}

/// `VtUi::on_add` spawns the grid before the cursor so bevy_ui's
/// later-sibling-on-top rule keeps the cursor overlay above the
/// rebuilt text spans.
#[test]
fn vtui_spawns_grid_then_cursor_as_children() {
    let mut app = get_test_app();
    let (_, vtui_id) = spawn_term(&mut app);

    let children: Vec<Entity> = app
        .world()
        .get::<Children>(vtui_id)
        .expect("VtUi should have children")
        .iter()
        .collect();

    let grid_idx = children
        .iter()
        .position(|&e| app.world().get::<VtUiGrid>(e).is_some())
        .expect("VtUi child set should include a VtUiGrid");
    let cursor_idx = children
        .iter()
        .position(|&e| app.world().get::<VtUiCursor>(e).is_some())
        .expect("VtUi child set should include a VtUiCursor");

    assert!(
        grid_idx < cursor_idx,
        "VtUiGrid (idx {grid_idx}) must precede VtUiCursor (idx {cursor_idx}) so the cursor \
         renders on top of the text spans; pre-fix the order was reversed and the cursor was \
         occluded by glyphs",
    );

    // The grid relationship back-reference must resolve to the VtUi.
    let grid_id = children[grid_idx];
    let back = app
        .world()
        .get::<VtUiGrid>(grid_id)
        .expect("VtUiGrid present")
        .target();
    assert_eq!(back, vtui_id, "VtUiGrid back-reference should target VtUi");
}

/// The `VtUiCursor` on-insert hook must seed `BackgroundColor` from
/// the required `VtCursorColor` so the cursor is visible on frame 0
/// instead of black-on-black for up to one strobe period.
#[test]
fn cursor_background_seeded_from_vtcursorcolor_on_insert() {
    let mut app = get_test_app();
    let (_, vtui_id) = spawn_term(&mut app);
    let (cursor_id, _) = find_cursor_and_grid(&app, vtui_id);

    let color = *app
        .world()
        .get::<VtCursorColor>(cursor_id)
        .expect("VtUiCursor requires VtCursorColor");
    let bg = app
        .world()
        .get::<BackgroundColor>(cursor_id)
        .expect("VtUiCursor requires BackgroundColor")
        .0;

    assert_eq!(
        bg, *color,
        "on_insert hook should seed BackgroundColor from VtCursorColor; pre-fix it stayed \
         Color::NONE until flash_cursor flipped it ~530ms later",
    );
}

/// A zero-duration [`VtStrobeTimer`] is the "blink disabled" sentinel:
/// `flash_cursor` must force the cursor visible and skip ticking,
/// otherwise a zero-period repeating `Timer` would `just_finish`
/// every frame and the cursor would strobe at the framerate.
#[test]
fn flash_cursor_zero_duration_keeps_cursor_visible() {
    let mut app = get_test_app();
    let (_, vtui_id) = spawn_term(&mut app);
    let (cursor_id, _) = find_cursor_and_grid(&app, vtui_id);

    let color = *app.world().get::<VtCursorColor>(cursor_id).unwrap();

    // Override the default 530ms timer with the "blink disabled"
    // sentinel and force the bg invisible to prove `flash_cursor`
    // restores it.
    app.world_mut()
        .entity_mut(cursor_id)
        .insert(VtStrobeTimer::new(Duration::ZERO))
        .insert(BackgroundColor(Color::NONE));

    app.world_mut()
        .run_system_once(flash_cursor)
        .expect("flash_cursor ran");

    let bg = app.world().get::<BackgroundColor>(cursor_id).unwrap().0;
    assert_eq!(
        bg, *color,
        "zero-duration strobe must force BackgroundColor = VtCursorColor, not blink",
    );
}

/// `refresh_ui` clears and rebuilds the grid's text spans; it must
/// not touch the sibling cursor entity. Pre-fix it called
/// `despawn_children` on the `VtUi` itself, which would have killed
/// the cursor every redraw.
#[test]
fn refresh_ui_preserves_cursor_entity() {
    let mut app = get_test_app();
    let (term_id, vtui_id) = spawn_term(&mut app);
    let (cursor_id, grid_id) = find_cursor_and_grid(&app, vtui_id);

    // Give the terminal a non-zero size so `refresh_ui` enters its
    // padding loop and actually spawns grid children to despawn next
    // time round.
    app.world_mut()
        .entity_mut(term_id)
        .insert(VtSize { cols: 4, rows: 2 });

    // Two passes: the first populates the grid, the second exercises
    // the despawn_children path on a non-empty grid.
    for _ in 0..2 {
        app.world_mut()
            .write_message(TermRedrawRequestedMsg { term: term_id });
        app.world_mut()
            .run_system_once(refresh_ui)
            .expect("refresh_ui ran");
        app.world_mut().flush();
    }

    assert!(
        app.world().get_entity(cursor_id).is_ok(),
        "refresh_ui must not despawn the cursor entity",
    );
    assert!(
        app.world().get::<VtUiCursor>(cursor_id).is_some(),
        "cursor entity must still carry VtUiCursor marker",
    );
    assert!(
        app.world().get_entity(grid_id).is_ok(),
        "refresh_ui must not despawn the grid entity itself",
    );
}
