//! Regression test for the trackpad scroll bug fixed in
//! `fix(scroll): accumulate fractional trackpad pixel deltas`.
//!
//! Trackpads emit many `MouseScrollUnit::Pixel` `Pointer<Scroll>`
//! events per gesture, each with `|y|` well under one line of height.
//! The original `on_scroll` converted to lines (`y / line_height`)
//! and cast to `isize`, which truncated every event to `0` and
//! produced `TermScrollMsg { delta: 0 }` -- no movement, ever.
//!
//! Fix (`src/systems.rs`): a [`VtScrollAccumulator`] component on
//! the `VtUi` carries the fractional remainder across events; only
//! emit a [`TermScrollMsg`] when a whole line has accumulated. A
//! [`VtScrollSensitivity`] resource scales line vs. pixel deltas.
//!
//! These tests synthesise `Pointer<Scroll>` events directly via
//! `Commands::trigger` (no real window or picking backend needed --
//! [`NormalizedRenderTarget::None`] is sufficient for the
//! [`Location`] field) and assert that:
//!
//! 1. Many small `Pixel` deltas eventually move `VtScrollPos`,
//!    with the accumulator carrying a sub-line remainder between
//!    events.
//! 2. A single `Line` delta moves `VtScrollPos` by exactly that
//!    many lines, with no remainder.

use bevy::camera::NormalizedRenderTarget;
use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, Scroll};
use bevy::picking::pointer::{Location, PointerId};
use bevy::text::LineHeight;

use crate::prelude::*;

/// Cols × rows of the synthetic terminal. `rows` is kept well below
/// the number of lines we write so `max_scroll > 0` in `apply_scroll`
/// and the viewport can actually move.
const TERM_COLS: usize = 80;
const TERM_ROWS: usize = 10;
/// Number of lines written into the terminal during `Startup`. Must
/// exceed `TERM_ROWS` so the scroll clamp range is non-empty.
const SEED_LINES: usize = 20;
/// `LineHeight::Px` value forced onto the `VtUi`. `on_scroll`
/// divides pixel deltas by this to get a line-fraction.
const LINE_HEIGHT_PX: f32 = 24.0;

#[derive(Resource)]
struct Term(Entity);

#[derive(Resource)]
struct Ui(Entity);

/// Synthesise a `Pointer<Scroll>` and queue it through
/// `Commands::trigger`. The `Pointer<E>` type is `#[derive(
/// EntityEvent)]`, so the observer registered on `entity` (via
/// `VtUi::on_add`) fires when commands flush. `NormalizedRenderTarget
/// ::None` lets us construct a [`Location`] without a real window.
fn fire_scroll(commands: &mut Commands, entity: Entity, unit: MouseScrollUnit, y: f32) {
    commands.trigger(Pointer::<Scroll>::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::None {
                width: 0,
                height: 0,
            },
            position: Vec2::ZERO,
        },
        Scroll {
            unit,
            x: 0.0,
            y,
            hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
            phase: bevy::input::touch::TouchPhase::Moved,
        },
        entity,
    ));
}

/// Spawn a terminal pre-loaded with [`SEED_LINES`] lines of text and
/// a `VtUi` whose `LineHeight` is pinned to [`LINE_HEIGHT_PX`]. The
/// `VtUi::on_add` hook registers the [`on_scroll`](crate::on_scroll)
/// observer for us, so triggers fired against the returned `VtUi`
/// entity drive the real production path.
fn setup_app() -> App {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let TestTerm { term: term_id, fg } = spawn_test_term(
            &mut commands,
            VtSize {
                cols: TERM_COLS,
                rows: TERM_ROWS,
            },
        );
        // VtSize must be present synchronously so `process_input`
        // applies the seeded writes on the first frame instead of
        // queueing them in `PendingTermInput`.
        for i in 0..SEED_LINES {
            commands.write_message(writeln(term_id, fg, format!("line {i}")));
        }
        commands.insert_resource(Term(term_id));

        // `VtUi::on_add` spawns a `VtCharWidth` child and observes
        // `on_scroll`. We override `LineHeight` so the
        // pixel-to-lines conversion in `on_scroll` uses a known
        // divisor.
        let ui_id = commands
            .spawn((VtUi::new(term_id), LineHeight::Px(LINE_HEIGHT_PX)))
            .id();
        commands.insert_resource(Ui(ui_id));
    });
    app
}

/// Many sub-line `Pixel` deltas accumulate and eventually scroll.
///
/// With `LINE_HEIGHT_PX = 24`, the default
/// `VtScrollSensitivity::pixel = 3.0`, and `y = -2.0` per event:
///
/// ```text
/// line_delta = (-2.0 / 24.0) * 3.0 = -0.25  per event
/// ```
///
/// After 10 events the accumulator has crossed two whole-line
/// boundaries (at events 4 and 8), so two `TermScrollMsg { delta:
/// -1 }` messages have been emitted and `VtScrollPos` should be 2
/// (scrolling *up* increases `scroll_pos` via
/// `saturating_sub_signed(-1)`). The leftover remainder is `-0.5`.
///
/// Pre-fix, every `(y / line_height) as isize` rounded the per-event
/// delta to 0, so no message was ever emitted and `VtScrollPos`
/// stayed at 0.
#[test]
fn pixel_scrolls_accumulate_to_whole_line_moves() {
    let mut app = setup_app();

    // Step 0: wait for `process_input` to spawn all seeded rows so
    // that `max_scroll = num_rows - size.rows > 0`. Once the row
    // count is large enough, transition to step 1 which fires the
    // scroll events one-per-frame (mirroring how a real trackpad
    // delivers pixel deltas, and avoiding any intra-frame observer-
    // command-flush ambiguity that batching all 10 in one tick
    // would invite).
    app.add_step(
        0,
        |term: Res<Term>,
         q_term: Query<TermInfo>,
         q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
         q_rows: Query<(Entity, &VtRow)>,
         mut next: ResMut<NextState<Step>>| {
            let terminfo = r!(q_term.get(term.0));
            let row_count = terminfo.rows(&q_rowtargets, &q_rows).count();
            if row_count < TERM_ROWS + 2 {
                return;
            }
            next.set(Step(1));
        },
    );

    // Step 1: fire one `Pixel { y: -2.0 }` scroll per frame, ten
    // times, then transition to step 2. The one-per-frame cadence
    // matches real trackpad input and guarantees the accumulator
    // component inserted by the first event is visible to the
    // second event (cross-frame command flush, not intra-frame).
    app.add_step(
        1,
        |mut fired: Local<u32>,
         ui: Res<Ui>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            if *fired >= 10 {
                next.set(Step(2));
                return;
            }
            fire_scroll(&mut commands, ui.0, MouseScrollUnit::Pixel, -2.0);
            *fired += 1;
        },
    );

    // Step 2: wait a few frames for the last `TermScrollMsg`
    // emissions to drain through `apply_scroll`, then assert the
    // resulting position and accumulator remainder.
    app.add_step(
        2,
        |mut waited: Local<u32>,
         term: Res<Term>,
         ui: Res<Ui>,
         q_pos: Query<&VtScrollPos>,
         q_acc: Query<&VtScrollAccumulator>,
         mut commands: Commands| {
            *waited += 1;
            if *waited < 4 {
                return;
            }
            let pos = r!(q_pos.get(term.0));
            r!(commands.assert(
                pos.0 == 2,
                format!(
                    "expected VtScrollPos = 2 after 10 sub-line pixel scrolls (line_delta = \
                     -0.25 each, two whole-line boundaries crossed), got {}",
                    pos.0,
                ),
            ));

            let acc = r!(q_acc.get(ui.0));
            // Cumulative line_delta = -2.5; 2 whole lines emitted;
            // remainder should be -0.5 (within float tolerance).
            r!(commands.assert(
                (acc.0 - (-0.5)).abs() < 1e-3,
                format!("expected accumulator remainder ≈ -0.5, got {}", acc.0),
            ));
            r!(commands.assert(
                acc.0 > -1.0 && acc.0 < 1.0,
                format!("accumulator should always be in (-1.0, 1.0), got {}", acc.0),
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}

/// A single whole-line `Line`-unit scroll moves `VtScrollPos` by
/// exactly that many lines, with no remainder. Guards the line-unit
/// branch of `on_scroll` (the pre-fix code path that worked for
/// classic mouse wheels but not for trackpads).
#[test]
fn line_unit_scroll_moves_exact_lines() {
    let mut app = setup_app();

    app.add_step(
        0,
        |term: Res<Term>,
         ui: Res<Ui>,
         q_term: Query<TermInfo>,
         q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
         q_rows: Query<(Entity, &VtRow)>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let terminfo = r!(q_term.get(term.0));
            if terminfo.rows(&q_rowtargets, &q_rows).count() < TERM_ROWS + 2 {
                return;
            }
            fire_scroll(&mut commands, ui.0, MouseScrollUnit::Line, -3.0);
            next.set(Step(1));
        },
    );

    app.add_step(
        1,
        |term: Res<Term>,
         ui: Res<Ui>,
         q_pos: Query<&VtScrollPos>,
         q_acc: Query<&VtScrollAccumulator>,
         mut commands: Commands| {
            let pos = r!(q_pos.get(term.0));
            if pos.0 == 0 {
                return;
            }
            r!(commands.assert(
                pos.0 == 3,
                format!(
                    "expected VtScrollPos = 3 after one Line(-3) scroll, got {}",
                    pos.0
                ),
            ));
            // Line(-3) at sensitivity.line = 1.0 → total = -3.0,
            // whole = -3, remainder = 0.
            if let Ok(acc) = q_acc.get(ui.0) {
                r!(commands.assert(
                    acc.0.abs() < 1e-3,
                    format!(
                        "expected accumulator remainder ≈ 0.0 after exact line scroll, got {}",
                        acc.0
                    ),
                ));
            }
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
