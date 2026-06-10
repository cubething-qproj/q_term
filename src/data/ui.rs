//! Rendering via [bevy::ui]
use std::time::Duration;

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    text::LineHeight,
};

use crate::{prelude::*, systems::term::on_scroll};

/// [bevy::ui]-based implementation of the [`Terminal`] frontend.
///
/// Related to [`VtUiTarget`] (1:1)
#[derive(Component, Reflect, Debug, PartialEq, Clone, Copy)]
#[require(
        Node {
            overflow: Overflow::clip(),
            ..Default::default()
        },
        Pickable,
        TextColor,
        TextFont,
        LineHeight,
    )]
#[component(on_add=Self::on_add)]
#[relationship(relationship_target = VtUiTarget)]
pub struct VtUi(Entity);
impl VtUi {
    pub fn new(term_id: Entity) -> Self {
        Self(term_id)
    }
    pub fn target(&self) -> Entity {
        self.0
    }
    /// Spawn [`VtUiTarget`], [`VtCharWidth`] etc
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let term_id = world.get::<VtUi>(ctx.entity).unwrap().0;
        let mut commands = world.commands();
        commands.entity(term_id).insert(VtUiTarget::new(ctx.entity));

        let id = commands.spawn(VtCharWidth::new(ctx.entity, 0.)).id();
        commands
            .entity(ctx.entity)
            .add_one_related::<VtCharWidth>(id)
            .observe(on_scroll);

        // Spawn order matters: bevy_ui renders later siblings on
        // top of earlier ones. The grid must come first so the cursor
        // overlays glyphs rather than hiding behind them.
        //
        // VtUiGrid carries a back-reference to the VtUi entity via its
        // relationship; the matching VtUiGridTarget is inserted on
        // ctx.entity automatically.
        commands.spawn((VtUiGrid(ctx.entity), ChildOf(ctx.entity)));
        commands.entity(ctx.entity).with_child(VtUiCursor);
    }
}

/// Text-rooted child of [`VtUi`] that owns the grid of [`TextSpan`]s
/// rebuilt by `refresh_ui`. Sibling of [`VtUiCursor`] so cursor
/// rendering is unaffected by redraws.
///
/// Related 1:1 to [`VtUiGridTarget`].
#[derive(Component, Debug, Reflect, PartialEq, Clone, Copy)]
#[require(
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            ..Default::default()
        },
        TextLayout::new_with_no_wrap(),
        Text,
    )]
#[relationship(relationship_target = VtUiGridTarget)]
pub struct VtUiGrid(pub Entity);
impl VtUiGrid {
    pub fn target(&self) -> Entity {
        self.0
    }
}

/// Relationship target for [`VtUiGrid`] (1:1). Inserted on the
/// [`VtUi`] entity when its grid sub-node is spawned.
#[derive(Component, Debug, Reflect, PartialEq, Eq, Hash, Clone, Copy, Deref)]
#[relationship_target(relationship = VtUiGrid, linked_spawn)]
pub struct VtUiGridTarget(Entity);
impl VtUiGridTarget {
    pub fn target(&self) -> Entity {
        self.0
    }
}

/// Relationship target for [`VtUi`] (1:1). Will be attached to the given [`Terminal`] entity when
/// spawning [`VtUi`].
#[derive(Component, Debug, Reflect, PartialEq, Clone, Copy)]
#[relationship_target(relationship=VtUi)]
pub struct VtUiTarget(Entity);
impl VtUiTarget {
    pub fn new(target: Entity) -> Self {
        Self(target)
    }
    pub fn target(&self) -> Entity {
        self.0
    }
}

/// Width of a character cell in pixels, determined by measuring the width of a space.
/// Related to [`VtCharWidthTarget`] (1:1)
#[derive(Component, Debug, Reflect, PartialEq, Clone, Copy)]
#[component(immutable)]
#[require(Node::default(), Pickable::IGNORE, Visibility::Hidden, Text::new(" "))]
#[relationship(relationship_target=VtCharWidthTarget)]
pub struct VtCharWidth {
    #[relationship]
    target: Entity,
    value: f32,
}
impl VtCharWidth {
    pub fn new(target: Entity, value: f32) -> Self {
        Self { target, value }
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    pub fn target(&self) -> Entity {
        self.target
    }
}

/// Relationship target for [`VtCharWidth`]. Relationship is 1:1. Appears only with
/// [`VtUi`] or [`VtUi2d`].
#[derive(Component, Debug, Reflect, PartialEq, Eq, Hash, Clone, Copy, Deref)]
#[relationship_target(relationship=VtCharWidth, linked_spawn)]
pub struct VtCharWidthTarget(Entity);
impl VtCharWidthTarget {
    pub fn target(&self) -> Entity {
        self.0
    }
}

/// A component for the visual representation of the cursor.
/// Must be child of [VtUi]
#[derive(Reflect, Component, PartialEq, Eq, Debug)]
#[require(VtCursorStyle, VtCursorColor, VtStrobeTimer, Node, BackgroundColor)]
#[component(on_insert = Self::on_insert)]
pub struct VtUiCursor;
impl VtUiCursor {
    /// Seed [`BackgroundColor`] from the required [`VtCursorColor`] so
    /// the cursor is visible immediately rather than waiting up to one
    /// strobe period for [`flash_cursor`](crate::flash_cursor) to flip
    /// it on.
    fn on_insert(mut world: DeferredWorld, ctx: HookContext) {
        let color = world
            .get::<VtCursorColor>(ctx.entity)
            .copied()
            .unwrap_or_default();
        if let Some(mut bg) = world.get_mut::<BackgroundColor>(ctx.entity) {
            bg.0 = *color;
        }
    }
}

/// The duration between cursor visibility state changes.
#[derive(Reflect, Component, PartialEq, Debug, Deref, DerefMut)]
pub struct VtStrobeTimer(Timer);
impl VtStrobeTimer {
    pub fn new(duration: Duration) -> Self {
        Self(Timer::new(duration, TimerMode::Repeating))
    }
}
impl Default for VtStrobeTimer {
    fn default() -> Self {
        Self(Timer::new(Duration::from_millis(530), TimerMode::Repeating))
    }
}

/// The cursor's style. Defaults to Block.
#[derive(Reflect, Component, PartialEq, Eq, Default, Debug)]
pub enum VtCursorStyle {
    #[default]
    Block,
    Beam,
    Underline,
}

/// The color of the cursor display. Defaults to white at 50% opacity.
#[derive(Reflect, Component, PartialEq, Debug, Clone, Copy, Deref, DerefMut)]
pub struct VtCursorColor(Color);
impl Default for VtCursorColor {
    fn default() -> Self {
        Self(Color::srgba(1., 1., 1., 0.5))
    }
}
