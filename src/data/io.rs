//! Input and output channels (to/from [`Process`])

use std::collections::VecDeque;

use crate::prelude::*;

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
    pub term: Entity,
    pub input: TermInput,
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

/// Bytes flowing from a program into the [`Terminal`].
/// NOTE: There is no equivalent for StdErr.
#[derive(Message, Debug, Clone, Reflect)]
pub struct TermStdOut {
    /// Source [`Terminal`]
    pub term: Entity,
    /// Source program / etc
    pub from: Entity,
    /// Text spans to push to the terminal
    pub message: Vec<TermWrite>,
}

/// Pending writes queued on a term whose
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
#[derive(Component, Debug, Clone, Reflect)]
pub struct PendingStdOut {
    pub msgs: VecDeque<TermStdOut>,
}
impl Default for PendingStdOut {
    fn default() -> Self {
        Self {
            msgs: VecDeque::with_capacity(1024),
        }
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
