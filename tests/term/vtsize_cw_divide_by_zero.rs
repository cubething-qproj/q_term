//! Regression test for the capacity-overflow panic that occurred when
//! `VtUi` was spawned with a `ComputedNode` before character-width
//! measurement had populated `VtCharWidth`.
//!
//! Root cause: `resize` divides `ComputedNode::size().x` by
//! `VtCharWidth::value()`, casts the result to `usize`. `f32 as usize`
//! saturates for non-finite values, so `1.0 / 0.0 = +inf` becomes
//! `usize::MAX`. That `VtSize { cols: usize::MAX, rows: usize::MAX }`
//! flowed into downstream systems that called `" ".repeat(cols)`
//! / `String::with_capacity(cols)`, producing
//! `thread '...' panicked: capacity overflow` in `raw_vec`.
//!
//! The presenting symptom in `q_term/active/examples/minimal` was a
//! "freeze on launch": the panic occurred on a `Compute Task Pool`
//! worker, the main thread kept the window alive, and the runtime
//! aborted instead of unwinding.
//!
//! Fix (`src/systems.rs`): early-bail via `tiny_bail::c!` when
//! `cw.value()` or `line_height` is non-positive, so no `VtSize` is
//! written until measurement produces real metrics.

use bevy::ecs::system::RunSystemOnce;
use bevy::ui::ComputedNode;

use crate::prelude::*;

/// `resize` must not insert an absurd `VtSize` when `VtCharWidth` is
/// still at its initial zero value while `ComputedNode` already has a
/// real size. Without the fix, `(size.x / 0.0).floor() as usize`
/// saturates to `usize::MAX` and a `VtSize { cols: usize::MAX, rows:
/// usize::MAX }` is written, which causes downstream consumers
/// (`refresh_ui`, etc.) to panic with `capacity overflow` on
/// `" ".repeat(cols)`.
#[test]
fn resize_bails_when_char_width_is_zero() {
    let mut app = get_test_app();

    // Spawn the entities `resize` queries: a `VtUi` carrying a
    // `ComputedNode` with a real size, and (via the `VtUi` spawn
    // observer) the `VtCharWidth` entity with its initial value of
    // `0.0`. Insert a non-zero `ComputedNode` manually so the divide
    // sees `size.x > 0.0`.
    let term_id = app.world_mut().spawn(Terminal).id();
    let _vtui_id = app
        .world_mut()
        .spawn((
            Node {
                width: Val::Px(800.0),
                height: Val::Px(600.0),
                ..Default::default()
            },
            VtUi::new(term_id),
            ComputedNode {
                size: bevy::math::Vec2::new(800.0, 600.0),
                ..Default::default()
            },
        ))
        .id();

    // Drive `resize` directly. Pre-fix: succeeds but writes
    // `VtSize { cols: usize::MAX, rows: usize::MAX }`. Post-fix:
    // bails via `c!`, no `VtSize` insert.
    app.world_mut()
        .run_system_once(resize)
        .expect("resize ran");

    // The `Terminal` should not have an absurd `VtSize`. Either no
    // `VtSize` was written (post-fix bail) or it was written with sane
    // values; either way `cols` must fit within a reasonable bound,
    // not be near `usize::MAX`.
    let size = app.world().get::<VtSize>(term_id);
    if let Some(size) = size {
        assert!(
            size.cols < 1_000_000,
            "VtSize cols overflowed: got {}, expected sane value or no insert at all",
            size.cols,
        );
        assert!(
            size.rows < 1_000_000,
            "VtSize rows overflowed: got {}, expected sane value or no insert at all",
            size.rows,
        );
    }
}
