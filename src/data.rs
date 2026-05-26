use crate::prelude::*;

mod terminfo {
    use super::*;
    use bevy::ecs::query::QueryData;

    /// Public API and query helpers for the [`Terminal`] entity.
    #[derive(QueryData, Debug)]
    pub struct TermInfo {
        pub id: Entity,
        pub cursor: &'static VtCursor,
        pub line_target: &'static VtLineTarget,
        pub viewport: &'static VtViewport,
        pub size: &'static VtSize,
        pub scroll_pos: &'static VtScrollPos,
        pub tab_stop: &'static VtTabStop,
    }
    impl<'w, 's> TermInfoItem<'w, 's> {
        #[inline(always)]
        pub fn lines<'a>(
            &self,
            q_lines: &'a Query<(Entity, &VtLine)>,
        ) -> impl Iterator<Item = (Entity, &'a VtLine)> {
            q_lines.iter_many(self.line_target.iter())
        }

        #[inline(always)]
        pub fn viewport_rows<'a>(
            &self,
            q_viewport_rows: &'a Query<(Entity, &VtViewportRow)>,
        ) -> impl Iterator<Item = (Entity, &'a VtViewportRow)> {
            q_viewport_rows.iter_many(self.viewport.iter())
        }

        pub fn rows<'a>(
            &self,
            q_row_targets: &'a Query<&VtRowTarget, With<VtLine>>,
            q_rows: &'a Query<(Entity, &VtRow)>,
        ) -> impl Iterator<Item = (Entity, &'a VtRow)> {
            self.line_target.iter().flat_map(|line_id| {
                let target = r!(q_row_targets.get(line_id).ok());
                q_rows.iter_many(target.entities()).collect::<Vec<_>>()
            })
        }

        /// Write text into this terminal's buffer. Supports ANSI.
        pub fn write(&self, commands: &mut Commands, value: impl ToString) {
            commands.write_message(TermStdOut::write(self.id, value));
        }
        /// Write rich text spans into this terminal's buffer.
        pub fn write_spans(&self, commands: &mut Commands, spans: Vec<TermWrite>) {
            commands.write_message(TermStdOut::write_spans(self.id, spans));
        }

        /// Write text into this terminal's buffer via a [`MessageWriter`].
        /// Supports ANSI. Use this in hot-path systems; for lifecycle
        /// hooks, observers, and external callers without a
        /// [`MessageWriter`] system param, use [`Self::write`].
        pub fn write_via(&self, writer: &mut MessageWriter<TermStdOut>, value: impl ToString) {
            writer.write(TermStdOut::write(self.id, value));
        }

        /// Write rich text spans into this terminal's buffer via a
        /// [`MessageWriter`]. Use this in hot-path systems; for lifecycle
        /// hooks, observers, and external callers without a
        /// [`MessageWriter`] system param, use [`Self::write_spans`].
        pub fn write_spans_via(
            &self,
            writer: &mut MessageWriter<TermStdOut>,
            spans: Vec<TermWrite>,
        ) {
            writer.write(TermStdOut::write_spans(self.id, spans));
        }
    }
}
pub use terminfo::*;

mod pty {
    use super::*;

    /// Pseudo-terminal entity marker. This component is in charge
    /// of storing the input buffer and sending data from the [`PtyLeader`]
    /// (the terminal) to the [`PtyFollower`] (the shell).
    #[derive(Component, Default, Debug, Reflect)]
    #[require(
        PtyLeader(Entity::PLACEHOLDER),
        PtyFollower(Entity::PLACEHOLDER),
        LineDiscipline
    )]
    pub struct Pty {
        /// The input buffer.
        pub buffer: String,
    }

    /// How the [`Terminal`] sends information to the [`TerminalShell`].
    /// Canonical mode is the default. It sends lines on submit.
    /// Raw mode sends inputs unbuffered. This is useful for TUIs like vim or htop.
    #[derive(Component, Default, Reflect, Debug)]
    #[component(immutable)]
    pub enum LineDiscipline {
        #[default]
        Canonical,
        Raw,
    }

    #[derive(Component, Reflect, Debug)]
    pub struct PtyLeader(Entity);

    #[derive(Component, Reflect, Debug)]
    pub struct PtyFollower(Entity);
}
pub use pty::*;

mod ui {
    use bevy::{
        ecs::{lifecycle::HookContext, world::DeferredWorld},
        text::LineHeight,
    };

    use super::*;

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
            let color = world.get::<VtCursorColor>(ctx.entity).copied().unwrap_or_default();
            if let Some(mut bg) = world.get_mut::<BackgroundColor>(ctx.entity) {
                bg.0 = *color;
            }
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
}
pub use ui::*;

mod terminal {
    use std::time::Duration;

    use super::*;

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
    #[derive(Component, Default, Reflect)]
    #[require(
        // Interal
        VtLineTarget,
        VtCursor,
        VtScrollPos,
        VtSize,
        VtViewport,
        VtTabStop,
        Name::new("Terminal"),
        )]
    pub struct Terminal;

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
}
pub use terminal::*;

mod events {
    use super::*;

    /// Reverse-channel bytes flowing **toward the application's stdin**
    /// (e.g. DSR / DA replies, encoded keystrokes, mouse reports). The
    /// parser emits these; an external consumer (`q_shell`, a loopback
    /// adapter, a test) decides where they go.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermStdIn {
        /// Target terminal entity.
        pub term: Entity,
        /// Raw bytes. Interpretation left to the consumer.
        pub writes: Vec<u8>,
    }
    impl TermStdIn {
        /// Construct a [`TermStdIn`] reply from a byte slice.
        pub fn new(term: Entity, writes: impl Into<Vec<u8>>) -> Self {
            Self {
                term,
                writes: writes.into(),
            }
        }
    }

    /// Bytes flowing **from the application** into the terminal's
    /// parser, parameterised by output channel (`1` = stdout,
    /// `2` = stderr, matching POSIX fd numbers).
    ///
    /// Use the [`TermStdOut`] and [`TermStdErr`] aliases at call
    /// sites; the generic exists so a single impl serves both channels
    /// while keeping them as distinct Bevy message types (separate
    /// `Messages<_>` resources, separate `MessageReader`s).
    ///
    /// `process_input` reads both channels and feeds them through the
    /// same parser. Future consumers (e.g. stderr-tinting renderers)
    /// can filter on channel by reading only the variant they care
    /// about.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermOutputChannel<const CHANNEL: u8> {
        /// Target terminal entity.
        pub term: Entity,
        /// Spans to write into the buffer.
        pub writes: Vec<TermWrite>,
    }

    /// Stdout writes from the application (POSIX fd 1).
    pub type TermStdOut = TermOutputChannel<1>;
    /// Stderr writes from the application (POSIX fd 2).
    pub type TermStdErr = TermOutputChannel<2>;

    impl<const CHANNEL: u8> TermOutputChannel<CHANNEL> {
        /// Construct a [`TermOutputChannel`] with arbitrary write spans.
        pub fn new(term: Entity, writes: Vec<TermWrite>) -> Self {
            Self { term, writes }
        }
        /// Writes text directly to the buffer. Supports ANSI. For a
        /// rich-text based API, see [`Self::write_spans`].
        pub fn write(term: Entity, value: impl ToString) -> Self {
            let line = value.to_string();
            Self {
                term,
                writes: vec![TermWrite::new(line)],
            }
        }
        /// Writes a simple line to the buffer. Supports ANSI. Will
        /// append a newline at the end. Will clear styles before and
        /// after writing. For rich text support, see
        /// [`Self::write_spans`].
        pub fn writeln(term: Entity, line: impl ToString) -> Self {
            let line = line.to_string();
            Self {
                term,
                writes: vec![TermWrite::new(line + "\n").reset_style(true)],
            }
        }
        /// Writes a rich line of text to the terminal. See
        /// [`TermWrite`] for more detail.
        pub fn write_spans(term: Entity, spans: Vec<TermWrite>) -> Self {
            Self {
                term,
                writes: spans,
            }
        }
    }

    /// Request to scroll a terminal viewport by `delta` lines.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermScrollMsg {
        /// Target terminal entity.
        pub term: Entity,
        /// Signed line delta. Positive scrolls toward older content.
        pub delta: isize,
    }
    impl TermScrollMsg {
        /// Construct a [`TermScrollMsg`].
        pub fn new(term: Entity, delta: isize) -> Self {
            Self { term, delta }
        }
    }

    /// Request to jump a terminal viewport to the bottom.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermJumpToBottomMsg {
        /// Target terminal entity.
        pub term: Entity,
    }
    impl TermJumpToBottomMsg {
        /// Construct a [`TermJumpToBottomMsg`].
        pub fn new(term: Entity) -> Self {
            Self { term }
        }
    }

    /// Request to reflow a terminal's buffer to the current viewport.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermReflowMsg {
        /// Target terminal entity.
        pub term: Entity,
    }
    impl TermReflowMsg {
        /// Construct a [`TermReflowMsg`].
        pub fn new(term: Entity) -> Self {
            Self { term }
        }
    }

    /// Notification that a terminal's buffer was mutated.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermBufferMutatedMsg {
        /// Target terminal entity.
        pub term: Entity,
    }
    impl TermBufferMutatedMsg {
        /// Construct a [`TermBufferMutatedMsg`].
        pub fn new(term: Entity) -> Self {
            Self { term }
        }
    }

    /// Notification that a terminal's cursor moved.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermCursorMovedMsg {
        /// Target terminal entity.
        pub term: Entity,
        /// Cursor position after the move.
        pub pos: VtCursor,
    }
    impl TermCursorMovedMsg {
        /// Construct a [`TermCursorMovedMsg`].
        pub fn new(term: Entity, pos: VtCursor) -> Self {
            Self { term, pos }
        }
    }

    /// Request to redraw the terminal's UI representation.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermRedrawRequestedMsg {
        /// Target terminal entity.
        pub term: Entity,
    }
    impl TermRedrawRequestedMsg {
        /// Construct a [`TermRedrawRequestedMsg`].
        pub fn new(term: Entity) -> Self {
            Self { term }
        }
    }

    /// Per-term byte cap for [`PendingTermInput`] queues. Default
    /// 1 MiB. Override by inserting before [`TerminalPlugin`] runs or
    /// by reassigning at runtime.
    #[derive(Resource, Debug, Clone, Copy, Reflect)]
    pub struct PendingTermInputCap {
        /// Maximum total bytes of `TermWrite::text` queued per term
        /// before [`PendingTermInput::push_writes`] starts evicting
        /// oldest spans (whole-span FIFO).
        pub bytes: usize,
    }
    impl Default for PendingTermInputCap {
        fn default() -> Self {
            Self { bytes: 1 << 20 }
        }
    }

    /// Pending [`TermStdOut`] writes queued on a term whose
    /// [`TermInfo`] could not be resolved when the message was
    /// processed.
    ///
    /// Producers attach this component instead of dropping the
    /// message; the `drain_pending` system re-emits a [`TermStdOut`]
    /// once the term's prerequisites resolve. Multiple queued
    /// writes against the same term accumulate in `writes` to
    /// preserve write order.
    ///
    /// The queue is bounded by [`PendingTermInputCap`] total
    /// `TermWrite::text` bytes. When a push would exceed the cap,
    /// oldest whole spans are evicted FIFO until the new content fits;
    /// the producer-side helper [`PendingTermInput::push_writes`]
    /// enforces this and emits a single `warn!` per call summarising
    /// any eviction.
    #[derive(Component, Debug, Clone, Default, Reflect)]
    pub struct PendingTermInput {
        /// Spans queued for the term. Re-emitted as the `writes`
        /// payload of a [`TermStdOut`] when drained.
        pub writes: Vec<TermWrite>,
    }
    impl PendingTermInput {
        /// Push spans onto the queue, evicting oldest whole spans
        /// (FIFO) when the queue would otherwise exceed `cap_bytes`
        /// in total `TermWrite::text` bytes. Returns the number of
        /// spans evicted.
        ///
        /// `cap_bytes` is passed in by the caller. Producer systems
        /// read it from `Res<`[`PendingTermInputCap`]`>`; tests pass
        /// arbitrary values to exercise eviction behaviour.
        ///
        /// A single incoming span larger than the cap is still
        /// admitted (dropping it would silently lose mid-stream
        /// data); the eviction loop will drain every other span
        /// from the queue in that case.
        ///
        /// Emits at most one `warn!` per call, summarising any
        /// eviction that occurred.
        pub fn push_writes(
            &mut self,
            new: impl IntoIterator<Item = TermWrite>,
            cap_bytes: usize,
        ) -> usize {
            let mut total: usize = self.writes.iter().map(|w| w.text.len()).sum();
            let mut evicted: usize = 0;
            for span in new {
                total += span.text.len();
                self.writes.push(span);
                while total > cap_bytes && self.writes.len() > 1 {
                    let popped = self.writes.remove(0);
                    total -= popped.text.len();
                    evicted += 1;
                }
            }
            if evicted > 0 {
                warn!(
                    evicted,
                    cap_bytes, "PendingTermInput exceeded cap; evicted oldest spans"
                );
            }
            evicted
        }
    }

    /// Pending [`TermScrollMsg`] delta queued on a term whose
    /// [`TermInfo`] could not be resolved when the message was
    /// processed.
    ///
    /// Producers attach this component instead of dropping the
    /// message; the `drain_pending` system re-emits a [`TermScrollMsg`]
    /// once the term's prerequisites resolve. Multiple queued
    /// scrolls against the same term accumulate in `delta`.
    #[derive(Component, Debug, Clone, Default, Reflect)]
    pub struct PendingTermScroll {
        /// Accumulated signed line delta. Re-emitted as the `delta`
        /// of a [`TermScrollMsg`] when drained.
        pub delta: isize,
    }
    impl PendingTermScroll {
        /// Accumulate a signed line delta with saturating semantics.
        /// Use this rather than `delta += new` to avoid overflow on
        /// pathological pending accumulations.
        pub fn add_delta(&mut self, new: isize) {
            self.delta = self.delta.saturating_add(new);
        }
    }

    /// Notification that focus on a terminal entity changed.
    ///
    /// Stub; no producer or consumer wired up yet.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermFocusChangedMsg {
        /// Target terminal entity.
        pub term: Entity,
        /// `true` when the terminal gained focus, `false` when it lost it.
        pub focused: bool,
    }
    impl TermFocusChangedMsg {
        /// Construct a [`TermFocusChangedMsg`].
        pub fn new(term: Entity, focused: bool) -> Self {
            Self { term, focused }
        }
    }
}
pub use events::*;

mod command {
    use super::*;
    use std::fmt::Debug;

    /// Generic command bus message addressed to a terminal entity.
    ///
    /// Reserves the contract that downstream consumers hook into. Production
    /// and consumption happen in [`TerminalSystems::Input`]; the writer->reader
    /// pair lives in the same schedule so messages are observed within a single
    /// frame.
    ///
    /// `q_term` ships only the data shape here. Convenience surfaces
    /// such as `println` deliberately live on the `shell` side via
    /// extension traits or wrapper types — the formatting policy and
    /// any redirection into [`TermStdOut`] is shell scope, not
    /// terminal scope.
    ///
    /// `q_term` does NOT register any concrete `CommandMsg<T>`;
    /// consumers call [`register_command_msg`] for the `T`s they
    /// need.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct CommandMsg<T>
    where
        T: Reflect + TypePath + Clone + Debug + Send + Sync + 'static,
    {
        /// Terminal entity the command is addressed to.
        pub term: Entity,
        /// Command payload. Concrete `T` is owned by the consumer.
        pub command: T,
    }

    impl<T> CommandMsg<T>
    where
        T: Reflect + TypePath + Clone + Debug + Send + Sync + 'static,
    {
        /// Construct a [`CommandMsg`] addressed to `term`.
        pub fn new(term: Entity, command: T) -> Self {
            Self { term, command }
        }
    }

    /// Register `CommandMsg<T>` as a Bevy message on `app`.
    ///
    /// Canonical registration point for generic command messages.
    /// `q_term::TerminalPlugin` does not register any specific `T`
    /// because the set of payload types is owned by downstream
    /// consumers (`shell`, `quell`, application code). Each consumer
    /// calls this helper once per `T` it cares about.
    pub fn register_term_cmd<T>(app: &mut App)
    where
        T: Reflect + TypePath + Clone + Debug + Send + Sync + 'static,
    {
        app.add_message::<CommandMsg<T>>();
    }
}
pub use command::*;

pub mod helpers {
    use super::*;

    /// This struct hold all the necessary data to spawn a terminal text span in
    /// a convenient format. In order to facilitate text wrapping and ANSI
    /// support, [`VtLine`] data must contain the entire logical line, while the
    /// text spans must be spawned separately. This struct is designed to help
    /// with the API by making virtual text spans easier to author.
    #[derive(Debug, PartialEq, Reflect, Clone)]
    pub struct TermWrite {
        pub text: String,
        pub style: Option<VtCellStyle>,
        pub reset_style: bool,
    }
    impl TermWrite {
        pub fn new(text: impl ToString) -> Self {
            Self {
                text: text.to_string(),
                style: None,
                reset_style: false,
            }
        }
        pub fn with_color(self, color: impl Into<Color>) -> Self {
            Self {
                style: Some(VtCellStyle {
                    color: color.into(),
                    ..self.style.unwrap_or_default()
                }),
                ..self
            }
        }
        pub fn with_background(self, color: impl Into<Color>) -> Self {
            Self {
                style: Some(VtCellStyle {
                    background: color.into(),
                    ..self.style.unwrap_or_default()
                }),
                ..self
            }
        }
        pub fn reset_style(self, reset_style: bool) -> Self {
            Self {
                reset_style,
                ..self
            }
        }
    }
}
pub use helpers::*;
