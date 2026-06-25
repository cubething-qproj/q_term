//! SM / RM dispatch skeleton + DECTCEM (`?25`).
//!
//! These tests intentionally target the *skeleton*, not a full mode
//! table. The contract we're locking in:
//!
//! 1. Every [`Terminal`] gains a [`VtModes`] component with sane
//!    defaults (`dectcem = true`).
//! 2. `CSI ? 25 h` / `CSI ? 25 l` flip `VtModes.dectcem` via the
//!    private-mode parameter parser.
//! 3. Multi-param SM/RM sequences (`CSI ? 25 ; 999 l`) apply the
//!    modes they recognize and ignore the rest without panicking.
//! 4. Non-private SM/RM (`CSI 4 h`, no `?` intermediate) does *not*
//!    touch DEC private modes — the skeleton must not conflate the
//!    two parameter spaces.
//! 5. Unknown private modes (`CSI ? 999 h`) are a no-op.
//! 6. When `dectcem == false`, [`flash_cursor`] forces the cursor's
//!    [`BackgroundColor`] to [`Color::NONE`] regardless of strobe
//!    state. When `dectcem == true`, normal blink semantics resume.

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use crate::prelude::*;

// ---------------------------------------------------------------------------
// Parser-level helpers (mirror `tests/term/ansi.rs::ansi_test`)
// ---------------------------------------------------------------------------

/// Spawn a terminal, write `input`, then on the first step run
/// `check` against the resolved [`VtModes`] component. Pass/fail is
/// signalled via the standard `AppExit` channel.
fn modes_test(
    input: &'static str,
    check: impl Fn(&VtModes, &mut Commands) -> bool + Send + Sync + 'static,
) {
    let mut app = get_test_app();
    app.add_systems(Startup, move |mut commands: Commands| {
        let TestTerm { term, fg } = spawn_test_term(&mut commands, VtSize { cols: 20, rows: 4 });
        commands.write_message(write(term, fg, input));
    });
    app.add_step(
        0,
        move |q_modes: Query<&VtModes, With<Terminal>>, mut commands: Commands| {
            let modes = r!(q_modes.single());
            if check(modes, &mut commands) {
                commands.write_message(AppExit::Success);
            } else {
                commands.write_message(AppExit::error());
            }
        },
    );
    assert!(app.run().is_success());
}

// ---------------------------------------------------------------------------
// Skeleton: defaults + dispatch plumbing
// ---------------------------------------------------------------------------

/// Every [`Terminal`] should auto-spawn a [`VtModes`] component with
/// `dectcem = true`. Without this, downstream render code has no
/// signal to consult and DECTCEM-off becomes unobservable.
#[test]
fn vtmodes_defaults_attached_to_terminal() {
    modes_test("", |modes, commands| {
        r!(commands.assert(modes.dectcem, "VtModes::dectcem should default to true"))
    });
}

/// `CSI ? 25 l` is the canonical "hide cursor" DECRST. It must flip
/// `VtModes.dectcem` to false.
#[test]
fn dectcem_reset_clears_flag() {
    modes_test("\x1b[?25l", |modes, commands| {
        r!(commands.assert(
            !modes.dectcem,
            "CSI ?25 l (DECTCEM reset) should clear VtModes.dectcem",
        ))
    });
}

/// `CSI ? 25 h` after a reset must restore `dectcem = true`. Locks in
/// the SM/RM symmetry — pre-skeleton there was no `h` arm at all.
#[test]
fn dectcem_set_restores_flag() {
    modes_test("\x1b[?25l\x1b[?25h", |modes, commands| {
        r!(commands.assert(
            modes.dectcem,
            "CSI ?25 h after ?25 l should restore VtModes.dectcem",
        ))
    });
}

/// Multi-param SM/RM: `CSI ? 999 ; 25 l` must reset DECTCEM even
/// when the known mode is *not* in the first slot. Guards against
/// the `params[0]`-only / early-return-on-unknown bug class — those
/// impls would happily pass `\x1b[?25;999l` while failing here.
#[test]
fn dectcem_multi_param_ignores_unknown() {
    modes_test("\x1b[?999;25l", |modes, commands| {
        r!(commands.assert(
            !modes.dectcem,
            "multi-param RM must scan every param, not just params[0]",
        ))
    });
}

/// Non-private SM (`CSI 25 h`, no `?` intermediate) targets the
/// ANSI mode space, not DEC private modes. The skeleton must keep
/// the two spaces disjoint — `?25` (DECTCEM) and bare `25` are
/// different modes that happen to share a number.
#[test]
fn ansi_mode_does_not_touch_private_modes() {
    // First reset DECTCEM, then issue the same number without `?`.
    // The specific ANSI mode number is irrelevant — the assertion
    // is purely about parameter-space separation, not about any
    // particular ANSI mode's semantics.
    modes_test("\x1b[?25l\x1b[25h", |modes, commands| {
        r!(commands.assert(
            !modes.dectcem,
            "bare `CSI 25 h` (no `?`) must not flip the private-mode DECTCEM flag",
        ))
    });
}

/// Unknown private modes are a no-op: parser must not panic and must
/// not touch unrelated flags.
#[test]
fn unknown_private_mode_is_noop() {
    modes_test("\x1b[?999h\x1b[?999l", |modes, commands| {
        r!(commands.assert(
            modes.dectcem,
            "unknown private modes (`?999`) must be silently ignored",
        ))
    });
}

// ---------------------------------------------------------------------------
// DECTCEM render wiring
// ---------------------------------------------------------------------------

/// Spawn a `Terminal` + `VtUi`, flush deferred commands, return ids.
/// Mirrors `tests/term/cursor.rs::spawn_term`.
fn spawn_term(app: &mut App) -> (Entity, Entity) {
    let term_id = app.world_mut().spawn(Terminal).id();
    let vtui_id = app.world_mut().spawn(VtUi::new(term_id)).id();
    app.world_mut().flush();
    (term_id, vtui_id)
}

/// Resolve the auto-spawned [`VtUiCursor`] child of a [`VtUi`].
fn find_cursor(app: &App, vtui_id: Entity) -> Entity {
    let children = app
        .world()
        .get::<Children>(vtui_id)
        .expect("VtUi should have children after flush");
    for child in children.iter() {
        if app.world().get::<VtUiCursor>(child).is_some() {
            return child;
        }
    }
    panic!("VtUi on_add should have spawned a VtUiCursor child");
}

/// With `VtModes.dectcem == false`, [`flash_cursor`] must force the
/// cursor's [`BackgroundColor`] to [`Color::NONE`], overriding both
/// the seeded visible color and the strobe timer.
#[test]
fn dectcem_off_hides_cursor_via_flash_cursor() {
    let mut app = get_test_app();
    let (term_id, vtui_id) = spawn_term(&mut app);
    let cursor_id = find_cursor(&app, vtui_id);

    // Disable DECTCEM directly on the component (skeleton already
    // wired the component onto Terminal via #[require]). Mutate in
    // place via `get_mut` so we don't silently require `VtModes: Copy`.
    {
        let mut modes = app
            .world_mut()
            .get_mut::<VtModes>(term_id)
            .expect("Terminal should have VtModes");
        modes.dectcem = false;
    }

    // Sanity: the on_insert hook seeded BackgroundColor to the
    // visible cursor color; flash_cursor must override it.
    let color = *app.world().get::<VtCursorColor>(cursor_id).unwrap();
    app.world_mut()
        .entity_mut(cursor_id)
        .insert(BackgroundColor(*color));

    app.world_mut()
        .run_system_once(flash_cursor)
        .expect("flash_cursor ran");

    let bg = app.world().get::<BackgroundColor>(cursor_id).unwrap().0;
    assert_eq!(
        bg,
        Color::NONE,
        "dectcem=false must force cursor BackgroundColor to Color::NONE",
    );
}

/// DECTCEM-off must win against the zero-duration "blink disabled"
/// sentinel in [`flash_cursor`]. The sentinel forces bg = visible
/// color via an early return; a naive impl that layers DECTCEM
/// *after* that return would leave the cursor visible when the user
/// has both disabled blinking and hidden the cursor.
#[test]
fn dectcem_off_overrides_zero_duration_strobe() {
    let mut app = get_test_app();
    let (term_id, vtui_id) = spawn_term(&mut app);
    let cursor_id = find_cursor(&app, vtui_id);

    {
        let mut modes = app
            .world_mut()
            .get_mut::<VtModes>(term_id)
            .expect("Terminal should have VtModes");
        modes.dectcem = false;
    }

    let color = *app.world().get::<VtCursorColor>(cursor_id).unwrap();
    app.world_mut()
        .entity_mut(cursor_id)
        .insert(VtStrobeTimer::new(Duration::ZERO))
        .insert(BackgroundColor(*color));

    app.world_mut()
        .run_system_once(flash_cursor)
        .expect("flash_cursor ran");

    let bg = app.world().get::<BackgroundColor>(cursor_id).unwrap().0;
    assert_eq!(
        bg,
        Color::NONE,
        "dectcem=false must override the zero-duration blink-disabled sentinel",
    );
}
