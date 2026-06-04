//! Input and output channels (to/from [`Process`])

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
    pub pty: Entity,
    pub input: TermInput,
}

/// Process signal messages. Interpreted by [`Job`] entities.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Sig {
    /// Produced by: (^C), kill builtin.
    /// Polite request to stop. Can be caught.
    Int,
    /// Produced by: (^\), kill builtin.
    /// Polite request to stop. Can be caught. Produces a core dump (in our case, stack trace).
    Quit,
    /// Produced by: (^Z), kill builtin.
    /// Signifies that a job is being placed in the background.
    Tstp,
    /// Produced by: kill builtin.
    /// Polite request from another program to stop. Can be caught.
    Term,
    /// Produced by: kill builtin.
    /// Immediate kill for the process. The kernel (q_term) despawns the
    /// [Process] immediately. Cannot be caught.
    Kill,
    /// Produced by: pty close, kill builtin.
    /// Signifies that any listeners have 'hung up' and are no longer available.
    /// Typically used as a reload mechanism or to exit a repl.
    Hup,
    /// [SIGSTOP]
    Stop,
    /// [SIGCONT]
    Cont,
    /// [SIGTTIN]
    Ttin,
    /// [SIGTTOUT]
    Ttou,
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
    pub signal: Sig,
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
pub struct ProgOutputChannel<const CHANNEL: u8> {
    /// Target terminal entity.
    pub term: Entity,
    /// Spans to write into the buffer.
    pub writes: Vec<TermWrite>,
}

/// Stdout writes. (POSIX fd 1).
pub type StdOut = ProgOutputChannel<1>;
/// Stderr writes to the [`Terminal`]. (POSIX fd 2).
pub type StdErr = ProgOutputChannel<2>;

impl<const CHANNEL: u8> ProgOutputChannel<CHANNEL> {
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
