//! The virtual terminal backend
use crate::prelude::*;

use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};

/// A terminal display. Will spawn a new [`Node`] sized to the parent
/// container and populate the hierarchy with [`TextSpan`] components according
/// to the target entity's properties.
///
/// Related components are prefixed with `Vt`, e.g. [`VtViewport`].
///
/// In order to display the terminal, you need to choose a front-end
/// component. They do not have to be on the same entity. For
/// [bevy::ui]-based rendering use [`VtUi`]. For [bevy::sprite]-based
/// rendering use [`VtUi2d`].
///
/// No shell is provided by default. You will need to bring your own.
///
/// ```rust
///# use q_term::prelude::*;
///# let mut app = App::new();
///# let mut world = app.world_mut();
///# let mut commands = world.commands();
/// let term_id = commands.spawn(Terminal).id();
/// commands.spawn(( Shell(term_id) ))
/// ```
#[derive(Component, Reflect)]
#[require(
    VtLineTarget,
    VtCursor,
    VtScrollPos,
    VtSize,
    VtViewport,
    VtTabStop,
    VtModes,
    Name::new("Terminal")
)]
pub struct Terminal;

/// DEC private modes the parser tracks per [`Terminal`]. Mutated by
/// SM (`CSI ? Pn h`) and RM (`CSI ? Pn l`); read by render systems
/// to drive cursor visibility, wrap behavior, etc.
///
/// Fields default to the values a freshly opened xterm would
/// report — i.e. cursor visible (DECTCEM) and auto-wrap on
/// (DECAWM). Add new modes as one-line field additions; the
/// dispatch site in `ansi.rs` is the single place that needs to
/// learn the new code.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VtModes {
    /// DECTCEM (`?25`). Cursor visible when `true`.
    pub dectcem: bool,
    /// DECAWM (`?7`). Auto-wrap at the right margin when `true`.
    /// Not yet honored by the renderer — reserved for the
    /// follow-up DECAWM PR so the dispatch table stays stable.
    pub decawm: bool,
}
impl Default for VtModes {
    fn default() -> Self {
        Self {
            dectcem: true,
            decawm: true,
        }
    }
}

/// Cursor for the [`Terminal`]. Points at a given char index into a
/// [`TerminalRow`] (nth from end). Note that this is relative to the
/// _viewport_.
///
/// Cursor positions are relative to the top-left of the viewport, with x
/// increasing to the right and y increasing downwards. This is the opposite
/// of the [`TerminalRow`] index.
///
/// Rendered by [`VtUiCursor`] when a [`VtUi`] frontend is attached.
///
/// ```notrust
///                        row idx | cursor line
/// (0,0).---------.             3 | 0
///      |         |             2 | 1
///      |         |             1 | 2
///      `---------`(10,3)       0 | 3
/// ```
#[derive(Component, Reflect, Clone, Copy, Debug, Default)]
pub struct VtCursor {
    pub row: usize,
    pub col: usize,
    pub pending_wrap: bool,
}
impl VtCursor {
    pub fn new(line: usize, char: usize) -> Self {
        Self {
            row: line,
            col: char,
            pending_wrap: false,
        }
    }
}

/// Configurable tab stop in character width
#[derive(Component, Reflect, Clone, Copy, Debug)]
pub struct VtTabStop(pub usize);
impl Default for VtTabStop {
    fn default() -> Self {
        Self(8)
    }
}

// TODO: Add a limit to the number of stored lines.
// This can't currently be a vecdeque due to trait constraints,
// so need to add a manual 'maximum' field and do checks on insert
// to ensure the vec doesn't overload its capacity.
/// This entity represents the underlying text buffer.
///
/// Relationship target for [`VtLine`] 1:n
#[derive(Component, Default, Deref, Debug, Reflect)]
#[relationship_target(relationship=VtLine)]
pub struct VtLineTarget(Vec<Entity>);
impl VtLineTarget {
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// A single, newline-delimited logical line. Lines are composed of vecs of
/// [`VtCell`]s. Lines do _not_ include the trailing newline.
///
/// This component requires [`VtRowTarget`]. All [`VtRow`]s related to this
/// entity can be accessed through that relationship. In this way, updates
/// to [`VtLine`] components will propogate to the visible lines.
///
/// ```notrust
///     (VtLineTarget) -> (VtLine; VtRowTarget) -> (VtRow)
///                 |                         \--> (VtRow)
///                 \---> (VtLine; VtRowTarget) -> (VtRow; VtViewportRow)
///                                                         V
///                                            (Terminal; VtLayout)
/// ```
#[derive(Component, Debug, Reflect, PartialEq, Clone)]
#[relationship(relationship_target=VtLineTarget)]
#[require(VtRowTarget)]
pub struct VtLine {
    cells: Vec<VtCell>,
    #[relationship]
    target: Entity,
}
impl VtLine {
    pub fn new(terminal_id: Entity) -> Self {
        Self {
            cells: vec![],
            target: terminal_id,
        }
    }
    pub fn from_str<S: ToString>(terminal_id: Entity, string: S) -> Self {
        Self {
            cells: string.to_string().chars().map(VtCell::new).collect(),
            target: terminal_id,
        }
    }
    pub fn from_str_with_style<S: ToString>(
        terminal_id: Entity,
        string: S,
        style: VtCellStyle,
    ) -> Self {
        Self {
            cells: string
                .to_string()
                .chars()
                .map(|c| VtCell::new(c).with_style(style))
                .collect(),
            target: terminal_id,
        }
    }
    pub fn as_string(&self) -> String {
        self.cells.iter().map(|cell| cell.value).collect()
    }
    pub fn from_cells(terminal_id: Entity, cells: Vec<VtCell>) -> Self {
        Self {
            cells,
            target: terminal_id,
        }
    }

    pub fn cells(&self) -> &[VtCell] {
        &self.cells
    }
}

/// Always coupled with [`VtLine`].
///
/// Relationship target for [`VtRow`] 1:n
#[derive(Component, Debug, Reflect, Default, Clone)]
#[relationship_target(relationship=VtRow)]
pub struct VtRowTarget(Vec<Entity>);
impl VtRowTarget {
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// A single cell within a [`VtLine`]. The basic building block of a virtual
/// terminal.
#[derive(Debug, Reflect, PartialEq, Clone, Copy)]
pub struct VtCell {
    pub value: char,
    pub style: VtCellStyle,
}
impl Default for VtCell {
    fn default() -> Self {
        Self {
            value: ' ',
            style: Default::default(),
        }
    }
}
impl VtCell {
    pub fn new(value: char) -> Self {
        Self {
            value,
            style: VtCellStyle::default(),
        }
    }
    pub fn with_style(mut self, style: VtCellStyle) -> Self {
        self.style = style;
        self
    }
}

/// Text styles for [`VtCell`]s.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct VtCellStyle {
    pub color: Color,
    pub background: Color,
}
impl Default for VtCellStyle {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            background: Color::BLACK,
        }
    }
}

/// Scroll position, in lines. 0 means you're at the bottom.
#[derive(Component, Debug, Reflect, PartialEq, Eq, Hash, Clone, Copy, Deref, Default)]
#[component(immutable)]
pub struct VtScrollPos(pub usize);

/// Accumulator for fractional line scroll deltas (e.g. trackpad pixel events).
///
/// Trackpads commonly emit many small `MouseScrollUnit::Pixel` events whose
/// converted line-delta is well under 1.0. Without accumulation those
/// deltas truncate to 0 when cast to `isize` and the viewport never moves.
/// `on_scroll` accumulates here and only emits a `TermScrollMsg` once the
/// magnitude crosses a whole line.
#[derive(Component, Debug, Reflect, Clone, Copy, Deref, Default)]
pub struct VtScrollAccumulator(pub f32);

/// Scroll sensitivity multipliers applied in `on_scroll` before
/// accumulation into [`VtScrollAccumulator`].
///
/// Defaults: `line = 1.0`, `pixel = 3.0`. Trackpads in particular tend to
/// emit many sub-line pixel events; a multiplier > 1 keeps the gesture
/// from feeling sluggish. Override by inserting this resource before
/// [`TerminalPlugin`] runs, or by reassigning at runtime.
#[derive(Resource, Debug, Clone, Copy, Reflect)]
pub struct VtScrollSensitivity {
    /// Multiplier for `MouseScrollUnit::Line` events.
    pub line: f32,
    /// Multiplier for `MouseScrollUnit::Pixel` events (post line-height
    /// normalization).
    pub pixel: f32,
}
impl Default for VtScrollSensitivity {
    fn default() -> Self {
        Self {
            line: 1.0,
            pixel: 3.0,
        }
    }
}

/// Visible layout for the terminal.
/// The terminal's layout grid. Contains references to entities which should
/// contain [`VtRow`] and [`VtViewportRow`] components.
///
/// Relationship target for [`VtViewportRow`] (1:n).
#[derive(Component, Default, Deref, Debug, Reflect)]
#[relationship_target(relationship=VtViewportRow)]
// #[component(on_add=Self::on_add)]
pub struct VtViewport(Vec<Entity>);
impl VtViewport {
    pub fn entities(&self) -> &[Entity] {
        &self.0
    }
}

/// Visible row. Possible sibling of [`VtRow`].
///
/// This entity does not require [`VtRow`] as there may be empty terminal
/// lines that still need to be rendered, for example when the buffer is
/// cleared.
///
/// Related to [`VtViewport`] 1:n.
#[derive(Component, Debug, Reflect, PartialEq, Eq, Hash, Clone, Copy)]
#[relationship(relationship_target = VtViewport)]
#[component(immutable)]
pub struct VtViewportRow(Entity);
impl VtViewportRow {
    pub fn new(term_id: Entity) -> Self {
        Self(term_id)
    }
}

/// A reference to a logical line with a character offset into its full text
/// value.
///
/// This entity may optionally have a [`VtViewportRow`] component attached
/// to it.
///
/// ```notrust
///                              (Terminal; VtViewport)
///                                             ^
///     (VtLine; VtRowTarget) -> (VtRow; VtViewportRow)
///                      \-----> (VtRow)
/// ```
///
/// Related to [`VtRowTarget`] (1:n)
#[derive(Component, Debug, Reflect, PartialEq, Eq, Hash, Clone, Copy)]
#[relationship(relationship_target=VtRowTarget)]
#[component(immutable)]
pub struct VtRow {
    /// Character offset into the line at which to begin this span.
    /// The range is always = [`VtSize::cols`].
    pub offset: usize,
    /// Relationship target. Points to `VtRowTarget`.
    #[relationship]
    line: Entity,
}
impl VtRow {
    pub fn new(line: Entity, offset: usize) -> Self {
        Self { offset, line }
    }

    pub fn line(&self) -> Entity {
        self.line
    }
}

#[derive(Component, Default, Debug, Reflect)]
#[component(immutable, on_insert=Self::on_insert)]
pub struct VtSize {
    pub rows: usize,
    pub cols: usize,
}
impl VtSize {
    fn on_insert(mut world: DeferredWorld, ctx: HookContext) {
        world
            .commands()
            .write_message(TermReflowMsg::new(ctx.entity));
    }
}

/// 1-1 relationship describing the foreground terminal process. This process
/// will be the target of all outflowing [`TermStdIn`] messages, and
/// only the [`TermStdOut`] messages from this entity will be rendered
/// to the [`Terminal`].
#[derive(Component, Debug, Reflect)]
#[relationship(relationship_target=VtForegroundProcessTarget)]
pub struct VtForegroundProcess {
    #[relationship]
    terminal: Entity,
}
impl VtForegroundProcess {
    pub fn new(terminal: Entity) -> Self {
        Self { terminal }
    }
}

/// Terminal relationship target for [`VtForegroundProcess`]
#[derive(Component, Debug, Reflect)]
#[relationship_target(relationship=VtForegroundProcess)]
pub struct VtForegroundProcessTarget {
    #[relationship_target]
    process: Entity,
}
impl VtForegroundProcessTarget {
    pub fn process(&self) -> Entity {
        self.process
    }
}
