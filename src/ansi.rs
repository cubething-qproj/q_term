#![allow(clippy::upper_case_acronyms)]

use crate::prelude::*;
use anstyle_parse::{Parser, Utf8Parser};
use bevy::color::palettes::{basic, css};

pub type AnsiParser = Parser<Utf8Parser>;

/// Select C0/C1 control codes. See
/// https://en.wikipedia.org/wiki/C0_and_C1_control_codes
#[allow(dead_code)]
#[repr(u8)]
enum ControlCodes {
    /// Bell, ^G, \a
    BEL = 0x07,
    /// Backspace, ^H, \b
    BS = 0x08,
    /// Tab, ^I, \t
    HT = 0x09,
    /// Line feed, ^J, \n
    LF = 0x0a,
    /// Carriage return, ^M, \r
    CR = 0x0d,
    /// Escape, ^[, \x1b, \033
    ESC = 0x1b,
}

#[repr(u8)]
#[allow(dead_code)]
enum CsiAction {
    // cursor actions
    /// Cursor up, n rows
    CUU = 0x41,
    /// Cursor down, n rows
    CUD = 0x42,
    /// Cursor forward, n cols
    CUF = 0x43,
    /// Cursor back, n cols
    CUB = 0x44,
    /// Cursor to start of line, n lines down
    CNL = 0x45,
    /// Cursor to start of line, n lines up
    CPL = 0x46,
    /// Cursor to column n
    CHA = 0x47,
    /// Cursor to row n, col m
    CUP = 0x48,
    /// Same as CUP
    HVP = 0x66,
    // erase
    /// Erase parts of the screen.
    ED = 0x4a,
    /// Erase around cursor.
    EL = 0x4b,
    // color
    /// SGR style escapes
    SGR = 0x6d,
}

#[derive(Debug, Clone, PartialEq)]
enum MaybeRef<'a, T: Clone> {
    Owned(Option<Entity>, T),
    Borrowed(Entity, &'a T),
}
impl<'a, T: Clone> MaybeRef<'a, T> {
    fn value(&self) -> &T {
        match self {
            MaybeRef::Owned(_, t) => t,
            MaybeRef::Borrowed(_, t) => t,
        }
    }
    fn entity(&self) -> Option<Entity> {
        match self {
            MaybeRef::Owned(Some(e), _) => Some(*e),
            MaybeRef::Owned(None, _) => None,
            MaybeRef::Borrowed(e, _) => Some(*e),
        }
    }
    fn is_owned(&self) -> bool {
        matches!(self, Self::Owned(_, _))
    }
}

macro_rules! assert_cursor_in_view {
    ($self:ident$(, $retvalue:expr)?) => {
        assert!($self.cursor.row <= $self.rows);
        assert!($self.cursor.col <= $self.cols);
    };
}

#[derive(PartialEq, Copy, Clone)]
struct VisibleRowIndex {
    line_idx: usize,
    row_idx: usize,
}
impl std::fmt::Debug for VisibleRowIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("({:?}, {:?})", self.line_idx, self.row_idx))
    }
}

#[derive(PartialEq, Clone, Deref, DerefMut)]
struct GridLine<'a> {
    #[deref]
    line: MaybeRef<'a, VtLine>,
    rows: Vec<GridRow<'a>>,
}
impl<'a> GridLine<'a> {
    pub fn new(term_id: Entity, rows: Vec<GridRow<'a>>) -> Self {
        Self {
            line: MaybeRef::Owned(None, VtLine::new(term_id)),
            rows,
        }
    }
}
impl<'a> std::fmt::Debug for GridLine<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let borrowed = if self.line.is_owned() { "" } else { "&" };
        let val = self.line.value().as_string();
        f.write_fmt(format_args!("<{borrowed}\"{val:?}\", {:?}>", self.rows))
    }
}

#[derive(PartialEq, Clone, Deref, DerefMut)]
struct GridRow<'a> {
    row: MaybeRef<'a, VtRow>,
}
impl<'a> GridRow<'a> {
    pub fn new(offset: usize) -> Self {
        Self {
            row: MaybeRef::Owned(None, VtRow::new(Entity::PLACEHOLDER, offset)),
        }
    }
}
impl<'a> std::fmt::Debug for GridRow<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let borrowed = if self.row.is_owned() { "" } else { "&" };
        f.write_fmt(format_args!("{borrowed}(>>{:?})", self.row.value().offset))
    }
}

/// A transient grid. Used to modify the terminal entities and reconstructed
/// on each pass.
#[derive(Debug)]
pub struct Grid<'a> {
    lines: Vec<GridLine<'a>>,
    viewport_entities: Vec<Entity>,
    cols: usize,
    rows: usize,
    scroll_pos: usize,
    cursor: VtCursor,
    term_id: Entity,
}
impl<'a> Grid<'a> {
    pub fn new<'w, 's>(
        terminfo: &TermInfoItem<'w, 's>,
        q_lines: &'a Query<(Entity, &VtLine, &VtRowTarget)>,
        q_rows: &'a Query<(Entity, &VtRow)>,
    ) -> Self {
        let lines = q_lines
            .iter_many(terminfo.line_target.entities())
            .map(|(line_id, line, row_target)| {
                let rows = q_rows
                    .iter_many(row_target.entities())
                    .map(|(row_id, row)| GridRow {
                        row: MaybeRef::Borrowed(row_id, row),
                    })
                    .collect::<Vec<_>>();
                GridLine {
                    line: MaybeRef::Borrowed(line_id, line),
                    rows,
                }
            })
            .collect::<Vec<_>>();

        Self {
            lines,
            viewport_entities: terminfo.viewport.to_vec(),
            cols: terminfo.size.cols,
            rows: terminfo.size.rows,
            scroll_pos: terminfo.scroll_pos.0,
            cursor: *terminfo.cursor,
            term_id: terminfo.id,
        }
    }

    fn visible_lines_as_string(&self, to_write: Option<char>) -> String {
        self.visible_rows()
            .iter()
            .enumerate()
            .flat_map(|(i, vrow)| {
                let line = self.lines.get(vrow.line_idx).unwrap();
                let row = line.rows.get(vrow.row_idx).unwrap();
                let mut string = line
                    .value()
                    .as_string()
                    .chars()
                    .skip(row.row.value().offset)
                    .take(self.cols)
                    .collect::<String>();
                if let Some(last) = string.chars().last()
                    && last != '\n'
                    && string.len() == self.cols
                {
                    string.push_str("[WRAP]\n");
                }
                string = string.replace('\n', "\\n");
                if i == self.cursor.row {
                    let newval = if let Some(to_write) = to_write {
                        let to_write = if to_write == '\n' {
                            "\\n".to_string()
                        } else {
                            to_write.to_string()
                        };
                        format!("[{to_write}]")
                    } else {
                        "_".to_string()
                    };
                    if string.get(self.cursor.col..self.cursor.col).is_some() {
                        if self.cursor.col == string.len() {
                            string.push_str(&newval);
                        } else {
                            string.remove(self.cursor.col);
                            string.insert_str(self.cursor.col, &newval);
                        }
                    } else {
                        string.push_str(&newval);
                    }
                }
                string = format!("{string:<width$} | {vrow:?}\n", width = self.cols);
                string.chars().collect::<Vec<char>>()
            })
            .collect::<String>()
    }

    pub fn scroll_viewport(&mut self, dir: isize) {
        self.scroll_pos = self.scroll_pos.saturating_add_signed(dir);
    }

    pub fn increment_char(&mut self, wrap: bool) {
        self.cursor.col = (self.cursor.col + 1).min(self.cols);
        if self.cursor.col >= self.cols {
            if wrap {
                if self.cursor.pending_wrap {
                    self.cursor.col = 0;
                    self.increment_line();
                } else {
                    self.cursor.pending_wrap = true;
                }
            } else {
                self.cursor.col = self.cols.saturating_sub(1);
            }
        }
        assert_cursor_in_view!(self);
    }
    pub fn decrement_char(&mut self, wrap: bool) {
        if self.cursor.col == 0 && wrap {
            self.decrement_line();
            self.cursor.col = self.cols.saturating_sub(1);
        } else {
            self.cursor.col = self.cursor.col.saturating_sub(1);
        }
        assert_cursor_in_view!(self);
    }
    pub fn increment_line(&mut self) {
        self.cursor.row = (self.cursor.row + 1).clamp(0, self.rows - 1);
        assert_cursor_in_view!(self);
    }
    pub fn decrement_line(&mut self) {
        self.cursor.row = self.cursor.row.saturating_sub(1);
        assert_cursor_in_view!(self);
    }

    /// Returns (line_idx, row_idx)
    /// row_idx is relative to the line
    fn visible_rows(&self) -> Vec<VisibleRowIndex> {
        assert_cursor_in_view!(self, vec![]);
        let lines = self
            .lines
            .iter()
            .enumerate()
            .flat_map(|(line_idx, line)| {
                line.rows
                    .iter()
                    .enumerate()
                    .map(|(row_idx, _)| VisibleRowIndex { line_idx, row_idx })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut lines = lines
            .into_iter()
            .rev()
            .skip(self.scroll_pos)
            .take(self.rows)
            .collect::<Vec<_>>();
        lines.reverse();
        lines
    }

    pub fn write(&mut self, c: char, style: VtCellStyle) {
        if self.cursor.pending_wrap {
            self.cursor.col = 0;
            self.increment_line();
            self.cursor.pending_wrap = false;
            self.lines
                .push(GridLine::new(self.term_id, vec![GridRow::new(0)]));
        }
        let mut visible_rows = self.visible_rows();
        while visible_rows.len() <= self.cursor.row {
            trace!("add new line (len = {:?})", self.lines.len());
            self.lines
                .push(GridLine::new(self.term_id, vec![GridRow::new(0)]));
            // guaranteed to exist as we just added it
            let last_line = self.lines.last().unwrap();
            visible_rows.push(VisibleRowIndex {
                line_idx: self.lines.len() - 1,
                row_idx: last_line.rows.len() - 1,
            });
        }

        #[cfg(debug_assertions)]
        {
            let line = "-".repeat(self.cols);
            let output = self.visible_lines_as_string(Some(c));
            trace!(
                "write {c:?}@<{},{},[{}]>({}x{})\n{line}\n{output}\n{line}\n",
                self.cursor.col, self.cursor.row, self.scroll_pos, self.cols, self.rows,
            );
        }
        let VisibleRowIndex { line_idx, row_idx } = visible_rows.get(self.cursor.row).unwrap();
        trace!(?line_idx, ?row_idx, "got visible row idx:");
        let gridline = self.lines.get_mut(*line_idx).unwrap();
        let gridrow = gridline.rows.get_mut(*row_idx).unwrap();
        let pos = gridrow.value().offset + self.cursor.col;
        let mut cells = gridline.line.value().cells().to_vec();
        let prev_len = cells.len();
        if pos < cells.len() {
            // overwrite existing cell
            cells[pos] = VtCell::new(c).with_style(style);
        } else {
            // extend: pad with empty cells, then append
            while cells.len() < pos {
                cells.push(VtCell::default());
            }
            cells.push(VtCell::new(c).with_style(style));
        }
        gridline.line = MaybeRef::Owned(
            gridline.line.entity(),
            VtLine::from_cells(self.term_id, cells),
        );
        // bump following row offsets by the number of cells inserted
        let growth = gridline.line.value().cells().len() - prev_len;
        if growth > 0 {
            gridline
                .rows
                .iter_mut()
                .enumerate()
                .for_each(|(idx, gridrow)| {
                    if idx > *row_idx {
                        let line_id = gridrow.row.value().line();
                        let offset = gridrow.row.value().offset + growth;
                        gridrow.row =
                            MaybeRef::Owned(gridrow.row.entity(), VtRow::new(line_id, offset))
                    }
                });
        }
    }

    /// Synchronize the [`Grid`] with the [`Terminal`] component in the
    /// [`World`]. Will update the corresponding [`VtRow`], [`VtLine`],
    /// [`VtCursor`], [`VtScrollPos`], and other components.
    pub fn sync(self, commands: &mut Commands) {
        // clear out visible row cache in order to refresh it here
        commands
            .entity(self.term_id)
            .remove_related::<VtViewportRow>(&self.viewport_entities);

        // cache viewport info
        commands
            .entity(self.term_id)
            .insert((self.cursor, VtScrollPos(self.scroll_pos)));

        // update grid entities
        let visible_rows = self.visible_rows();
        for (line_idx, gridline) in self.lines.into_iter().enumerate() {
            let line_id = match gridline.line {
                MaybeRef::Owned(Some(entity), line) => commands.entity(entity).insert(line).id(),
                MaybeRef::Owned(None, line) => commands.spawn(line).id(),
                _ => continue,
            };
            gridline
                .rows
                .into_iter()
                .enumerate()
                .for_each(|(row_idx, gridrow)| {
                    let rowid = match gridrow.row {
                        MaybeRef::Owned(Some(entity), row) => commands
                            .entity(entity)
                            .insert(VtRow::new(line_id, row.offset))
                            .id(),
                        MaybeRef::Owned(None, row) => {
                            commands.spawn(VtRow::new(line_id, row.offset)).id()
                        }
                        _ => return,
                    };
                    if visible_rows.contains(&VisibleRowIndex { line_idx, row_idx }) {
                        commands
                            .entity(rowid)
                            .insert(VtViewportRow::new(self.term_id));
                    }
                });
        }
    }
}

/// Parses the passed [`VirtualTextSpanSpawner`], possibly expanding it into multiple
/// and modifying various [`TerminalLine`]s.
pub(crate) struct AnsiPerformer<'a, 'g> {
    grid: &'a mut Grid<'g>,
    style: VtCellStyle,
    default_style: VtCellStyle,
}
impl<'a, 'g> AnsiPerformer<'a, 'g> {
    pub fn new(grid: &'a mut Grid<'g>) -> Self {
        Self {
            grid,
            style: VtCellStyle::default(),
            default_style: VtCellStyle::default(),
        }
    }
    pub fn reset_style(&mut self, style: VtCellStyle) {
        self.default_style = style;
        self.style = style;
    }
}
impl<'a, 'g> anstyle_parse::Perform for AnsiPerformer<'a, 'g> {
    fn print(&mut self, c: char) {
        trace!("print");
        self.grid.write(c, self.style);
        self.grid.increment_char(true);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            byte if byte == ControlCodes::BEL as u8 => {
                // TODO: sound a bell :)
                info!(?self.grid, "BEL");
            }
            byte if byte == ControlCodes::LF as u8 => {
                trace!("LF");
                self.grid
                    .lines
                    .push(GridLine::new(self.grid.term_id, vec![GridRow::new(0)]));
                self.grid.increment_line();
                self.grid.cursor.col = 0;
            }
            byte if byte == ControlCodes::CR as u8 => {
                self.grid.cursor.col = 0;
            }
            _ => {
                info_once!(
                    "Given an unsupported escape C0 or C1 escape sequence. See the docs supported types."
                );
                // unsupported
            }
        }
    }

    fn hook(
        &mut self,
        _params: &anstyle_parse::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: u8,
    ) {
        info!(?_action, "hook");
    }

    fn put(&mut self, _byte: u8) {
        info!(?_byte, "put");
    }

    fn unhook(&mut self) {
        info!("(unhook)")
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(
        &mut self,
        params: &anstyle_parse::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: u8,
    ) {
        info!(?action, "csi_dispatch");
        let mut param_iter = params.iter();
        match action {
            action if action == CsiAction::CUU as u8 => {
                let [next, ..] = param_iter.next().unwrap_or(&[0u16]) else {
                    return;
                };
                let next = (*next).max(1);
                for _ in 0..next {
                    self.grid.decrement_line();
                }
            }
            action if action == CsiAction::CUD as u8 => {
                let [next, ..] = param_iter.next().unwrap_or(&[0u16]) else {
                    return;
                };
                let next = (*next).max(1);
                for _ in 0..next {
                    self.grid.increment_line();
                }
            }
            action if action == CsiAction::CUF as u8 => {
                let [next, ..] = param_iter.next().unwrap_or(&[0u16]) else {
                    return;
                };
                let next = (*next).max(1);
                for _ in 0..next {
                    self.grid.increment_char(false);
                }
            }
            action if action == CsiAction::CUB as u8 => {
                let [next, ..] = param_iter.next().unwrap_or(&[0u16]) else {
                    return;
                };
                let next = (*next).max(1);
                for _ in 0..next {
                    self.grid.decrement_char(false);
                }
            }
            action if action == CsiAction::CNL as u8 => {
                let [next, ..] = param_iter.next().unwrap_or(&[0u16]) else {
                    return;
                };
                let next = (*next).max(1);
                self.grid.cursor.col = 0;
                for _ in 0..next {
                    self.grid.increment_line();
                }
            }
            action if action == CsiAction::CPL as u8 => {
                let [next, ..] = param_iter.next().unwrap_or(&[0u16]) else {
                    return;
                };
                let next = (*next).max(1);
                self.grid.cursor.col = 0;
                for _ in 0..next {
                    self.grid.decrement_line();
                }
            }
            action if action == CsiAction::CHA as u8 => {
                // CHA is 1-indexed; 0 treated as 1 per ANSI spec
                let col = param_iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1)
                    .max(1) as usize;
                self.grid.cursor.col = (col - 1).min(self.grid.cols.saturating_sub(1));
            }
            action if action == CsiAction::CUP as u8 || action == CsiAction::HVP as u8 => {
                // CUP params are semicolon-separated (two groups) and 1-indexed
                let row = param_iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1)
                    .max(1) as usize;
                let col = param_iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1)
                    .max(1) as usize;
                self.grid.cursor.row = (row - 1).min(self.grid.rows.saturating_sub(1));
                self.grid.cursor.col = (col - 1).min(self.grid.cols.saturating_sub(1));
            }
            // action if action == CsiAction::ED as u8 => {
            //     self.actions.push(AnsiAction::Erase { mode: iter.next().unwrap_or(0) as usize });
            // }
            // action if action == CsiAction::EL as u8 => {
            //     self.actions.push(AnsiAction::Erase { mode: iter.next().unwrap_or(0) as usize });
            // }
            action if action == CsiAction::SGR as u8 => {
                // SGR may contain multiple attributes in a single CSI
                // sequence (e.g. \x1b[38;2;R;G;B;48;2;R;G;Bm), so we loop.
                while let Some(param) = param_iter.next() {
                    match param {
                        [0] => self.style = self.default_style,
                        [8] => info_once!("Conceal color mode not yet implemented"),
                        // 24-bit color via colon subparams: \x1b[38:2:R:G:Bm
                        // TODO?: x == 58 for underline styling
                        [x, 2, r, g, b] if *x == 38 || *x == 48 => {
                            let color = Color::srgb_u8(*r as u8, *g as u8, *b as u8);
                            if *x == 38 {
                                self.style.color = color;
                            } else {
                                self.style.background = color;
                            }
                        }
                        // 256 color via colon subparams: \x1b[38:5:Nm
                        [x, 5, _n] if *x == 38 || *x == 48 => {
                            info_once!("256 color mode not currently supported.")
                        }
                        // 24-bit or 256 color via semicolons: \x1b[38;2;R;G;Bm
                        [x] if *x == 38 || *x == 48 => {
                            let mode = param_iter.next().and_then(|p| p.first().copied());
                            match mode {
                                Some(2) => {
                                    let r = param_iter
                                        .next()
                                        .and_then(|p| p.first().copied())
                                        .unwrap_or(0);
                                    let g = param_iter
                                        .next()
                                        .and_then(|p| p.first().copied())
                                        .unwrap_or(0);
                                    let b = param_iter
                                        .next()
                                        .and_then(|p| p.first().copied())
                                        .unwrap_or(0);
                                    let color = Color::srgb_u8(r as u8, g as u8, b as u8);
                                    if *x == 38 {
                                        self.style.color = color;
                                    } else {
                                        self.style.background = color;
                                    }
                                }
                                Some(5) => {
                                    let _ = param_iter.next(); // consume color index
                                    info_once!("256 color mode not currently supported.")
                                }
                                _ => {}
                            }
                        }
                        // palette mode
                        [x] if (*x >= 30 && *x <= 37)
                            || (*x >= 39 && *x <= 47)
                            || *x == 49
                            || (*x >= 90 && *x <= 97)
                            || (*x >= 100 && *x <= 107) =>
                        {
                            let is_standard = x / 10 == 3 || x / 10 == 4;
                            let is_bg = x / 10 == 4 || x / 10 == 10;
                            if *x == 39 || *x == 49 {
                                if is_bg {
                                    self.style.background = self.default_style.background;
                                } else {
                                    self.style.color = self.default_style.color;
                                }
                                continue;
                            }
                            // Standard (30–37, 40–47) → xterm dark variants
                            // Bright (90–97, 100–107) → xterm bright variants
                            // todo : TerminalPalette component or resource - per window or global?
                            let color = match (x % 10, is_standard) {
                                (0, true) => basic::BLACK,
                                (0, false) => basic::GRAY,
                                (1, true) => css::DARK_RED,
                                (1, false) => basic::RED,
                                (2, true) => basic::GREEN,
                                (2, false) => basic::LIME,
                                (3, true) => basic::OLIVE,
                                (3, false) => basic::YELLOW,
                                (4, true) => basic::NAVY,
                                (4, false) => basic::BLUE,
                                (5, true) => basic::PURPLE,
                                (5, false) => basic::FUCHSIA,
                                (6, true) => basic::TEAL,
                                (6, false) => css::AQUA,
                                (7, true) => basic::SILVER,
                                (7, false) => basic::WHITE,
                                _ => unreachable!(),
                            };
                            if is_bg {
                                self.style.background = color.into();
                            } else {
                                self.style.color = color.into();
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}
