use crate::prelude::*;

/// User-facing terminal API.
mod terminfo {
    use super::*;
    use bevy::ecs::query::QueryData;

    /// Public API and query helpers for the [`Terminal`] entity.
    #[derive(QueryData, Debug)]
    pub struct TermInfo {
        pub id: Entity,
        pub cursor: &'static VtCursor,
        pub modes: &'static VtModes,
        pub line_target: &'static VtLineTarget,
        pub viewport: &'static VtViewport,
        pub size: &'static VtSize,
        pub scroll_pos: &'static VtScrollPos,
        pub tab_stop: &'static VtTabStop,
        pub shell_target: Option<&'static ShellTarget>,
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

/// Basic data types required for a shell implementation.
mod shell {
    use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};

    use super::*;
    // Kernel equivalent: pty follower.
    // Obviated by single-use.
    /// Marker struct for shell entities. The systems associated with this
    /// struct must be implemented outside this crate.
    #[derive(Component, Reflect, Debug)]
    #[relationship(relationship_target = ShellTarget)]
    #[component(on_add = Self::on_add)]
    pub struct Shell(pub Entity);
    impl Shell {
        fn on_add(mut world: DeferredWorld, ctx: HookContext) {
            let mut cmds = world.commands();
            cmds.entity(ctx.entity).insert(ForegroundJob(ctx.entity));
        }
    }

    /// Attached to the [`Terminal`] when spawning a [`Shell`].
    #[derive(Component, Reflect, Debug)]
    #[relationship_target(relationship = Shell)]
    pub struct ShellTarget(Entity);
    impl ShellTarget {
        pub fn target(&self) -> Entity {
            self.0
        }
    }

    /// Marker for a process owned by a [`Shell`].
    /// As long as this component is attached to
    #[derive(Component, Reflect, Debug)]
    #[relationship(relationship_target = ShellJobTarget)]
    pub struct ShellJob(pub Entity);

    /// The [`Shell`] which owns this [`Job`].
    #[derive(Component, Reflect, Debug)]
    #[relationship_target(relationship = ShellJob)]
    pub struct ShellJobTarget(Entity);

    /// The focused program. Could be the shell or any other process.
    /// This is the mechanism behind blocking shell input.
    /// All messages sent via [`StdIn`]
    /// **Important:** this should _only_ be set by the shell.
    #[derive(Component, Reflect, Debug)]
    #[relationship(relationship_target = ForegroundJobTarget)]
    pub struct ForegroundJob(pub Entity);

    /// Attached to the [`Shell`] which owns this [`ForegroundJob`]
    #[derive(Component, Reflect, Debug)]
    #[relationship_target(relationship = ForegroundJob)]
    pub struct ForegroundJobTarget(Entity);
}
pub use shell::*;

/// Input and output channels (to/from 'processes' - terminal, shell, jobs)
mod io {
    use super::*;

    /// How the [`Terminal`] sends information to the [`Shell`].
    /// Canonical mode is the default. It sends lines on submit.
    /// Raw mode sends inputs unbuffered. This is useful for TUIs like vim or htop.
    #[derive(Component, Reflect, Debug)]
    pub enum LineDiscipline {
        Canonical { buffer: String },
        Raw,
    }
    impl Default for LineDiscipline {
        fn default() -> Self {
            Self::Canonical {
                buffer: String::new(),
            }
        }
    }

    /// Keystrokes et al. sent to the [`Terminal`] which must be interpreted by
    /// the [`LineDiscipline`]
    #[derive(Debug, Clone, Reflect)]
    #[non_exhaustive]
    pub enum TermInput {
        /// Pipe some text to the line discipline.
        Text(String),
        /// Clear the canonical mode buffer.
        /// Typically submitted via newline (\n)
        Submit,
        /// Backspace
        Erase,
        /// End of file.
        /// Typically submitted via (^D)
        Eof,
        // etc
    }

    /// Sends a message to the [`Terminal`], to be interpreted by the
    /// [`LineDiscipline`]
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermInputMsg {
        pub pty: Entity,
        pub input: TermInput,
    }

    /// Process signal messages. Interpreted by [`Job`] entities.
    #[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[non_exhaustive]
    pub enum SignalKind {
        /// SIGINT
        /// Produced by: (^C), kill builtin
        Int,
        /// SIGQUIT
        /// Produced by: (^\), kill builtin
        Quit,
        /// SIGTSTP
        /// Produced by: (^Z), kill builtin
        Tstp,
        /// SIGTERM
        /// Produced by: kill builtin
        Term,
        /// SIGKILL
        /// Produced by: kill builtin
        Kill,
        /// SIGHUP
        /// Produced by: pty close, kill builtin
        Hup,
        /// SIGCONT
        /// Produced by: shell (fg/bg resume)
        Cont,
        // Usr1, Usr2, Pipe, Chld, Winch as needed
    }

    /// Message to send a signal to a [`Job`].
    /// These are also known as interrupts.
    #[derive(Message, Clone, Copy, Debug, Reflect)]
    pub struct SignalMsg {
        /// Source [`Pty`]
        pub pty: Entity,
        /// Message sink - the targeted job
        pub target: Entity,
        /// Signal kind
        pub signal: SignalKind,
    }

    // TODO: This API needs to be completely re-thought.
    // Where do we translate from span-based to ANSI?
    // Probably want to keep the span-based writes _ONLY_ at
    // the consumer level _if that._

    /// Bytes flowing toward a running process. An external consumer such as
    /// a shell determines where it goes.
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermStdIn {
        /// Source [`Terminal`]
        pub term: Entity,
        /// Message sink -- the targeted job
        pub target: Entity,
        /// Raw bytes. Interpretation left to the consumer.
        pub message: String,
    }
    impl TermStdIn {
        /// Construct a [`TermStdIn`] reply from a byte slice.
        pub fn new(term: Entity, target: Entity, msg: impl ToString) -> Self {
            Self {
                term,
                target,
                message: msg.to_string(),
            }
        }
    }

    /// Flows from a [`Job`] or [`Shell`] into a [`Terminal`]'s
    /// ANSI parser. Parameterised by output channel (`1` = stdout,
    /// `2` = stderr, matching POSIX fd numbers).
    ///
    /// Use the [`TermStdOut`] and [`TermStdErr`] aliases at call
    /// sites. The generic exists so a single impl serves both channels
    /// while keeping them as distinct Bevy message types (separate
    /// [`Message`] resources, separate [`MessageReader`]s).
    #[derive(Message, Debug, Clone, Reflect)]
    pub struct TermOutputChannel<const CHANNEL: u8> {
        /// Target terminal entity.
        pub term: Entity,
        /// Spans to write into the buffer.
        pub writes: Vec<TermWrite>,
    }

    /// Stdout writes to the [`Terminal`]. (POSIX fd 1).
    pub type TermStdOut = TermOutputChannel<1>;
    /// Stderr writes to the [`Terminal`]. (POSIX fd 2).
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
}
pub use io::*;

/// Rendering via [bevy::ui]
mod ui {
    use std::time::Duration;

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
}
pub use ui::*;

/// The virtual terminal backend
mod terminal {
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
}
pub use terminal::*;

/// Events which modify the virtual terminal display.
mod events {
    use super::*;

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
}
pub use events::*;

/// Misc
mod helpers {
    use super::*;

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
