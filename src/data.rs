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

        #[inline(always)]
        pub fn mutate(&self, commands: &mut Commands, msg: TermMsgKind) {
            commands.write_message(TermMsg::new(self.id, msg));
        }

        pub fn write(&self, commands: &mut Commands, value: impl ToString) {
            commands.write_message(TermMsg::write(self.id, value));
        }
        pub fn write_spans(&self, commands: &mut Commands, spans: Vec<TermWrite>) {
            commands.write_message(TermMsg::write_spans(self.id, spans));
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
        TextLayout::new_with_no_wrap(),
        Pickable,
        TextColor,
        TextFont,
        LineHeight,
        Text,
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
}
pub use ui::*;

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
    #[derive(Component, Default, Reflect)]
    #[require(
        // Interal
        VtLineTarget,
        VtCursor,
        VtScrollPos,
        VtSize,
        VtViewport,
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
    /// ```notrust
    ///                        row idx | cursor line
    /// (0,0).---------.             3 | 0
    ///      |         |             2 | 1
    ///      |         |             1 | 2
    ///      `---------`(10,3)       0 | 3
    /// ```
    #[derive(Component, Default, Reflect, Clone, Copy, Debug)]
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
    ///
    /// Related to [`VtLineTarget`] 1:n
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
            world.commands().write_message(TermMsg::reflow(ctx.entity));
        }
    }
}
pub use terminal::*;

mod events {
    use super::*;

    /// A mutation to send to the terminal window.
    #[derive(Debug, Message, Reflect, Clone)]
    pub struct TermMsg {
        pub target: Entity,
        pub kind: TermMsgKind,
        pub(crate) retry_count: usize,
    }
    #[derive(Debug, Reflect, Clone)]
    pub enum TermMsgKind {
        /// Reflow the terminal. Modifes LineRefs.
        Reflow,
        /// Scroll in the given direction. A scroll position of 0 means you are
        /// at the last line.
        Scroll(isize),
        /// Jump to the last line. Sets scroll value to 0.
        JumpToBottom,
        /// Write charaters to the screen.
        Write(Vec<TermWrite>),
    }
    impl TermMsg {
        pub fn new(term_id: Entity, kind: TermMsgKind) -> Self {
            Self {
                target: term_id,
                kind,
                retry_count: 0,
            }
        }
        pub fn scroll(term_id: Entity, direction: isize) -> Self {
            Self::new(term_id, TermMsgKind::Scroll(direction))
        }
        pub fn jump_to_bottom(term_id: Entity) -> Self {
            Self::new(term_id, TermMsgKind::JumpToBottom)
        }
        pub fn reflow(term_id: Entity) -> Self {
            Self::new(term_id, TermMsgKind::Reflow)
        }
        /// Writes text directly to the buffer. Supports ANSI. For a rich-text
        /// based API, see [Self::write_spans]
        pub fn write(term_id: Entity, spans: impl ToString) -> Self {
            let line = spans.to_string();
            Self::new(term_id, TermMsgKind::Write(vec![TermWrite::new(line)]))
        }
        /// Writes a simple line to the buffer. Supports ANSI. Will append a
        /// newline at the end. Will clear styles before and after writing. For
        /// rich text support, see [Self::write_spans]
        pub fn writeln(term_id: Entity, line: impl ToString) -> Self {
            let line = line.to_string();
            Self::new(
                term_id,
                TermMsgKind::Write(vec![TermWrite::new(line + "\n").reset_style(true)]),
            )
        }
        /// Writes a rich line of text to the terminal. See [`TermWrite`] for
        /// more detail.
        pub fn write_spans(term_id: Entity, spans: Vec<TermWrite>) -> Self {
            Self::new(term_id, TermMsgKind::Write(spans))
        }
    }
}
pub use events::*;

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
