//! Events which modify the virtual terminal display.
use crate::prelude::*;

/// Ordered operation on a terminal viewport.
#[derive(Message, Debug, Clone, Reflect, PartialEq, Eq)]
pub enum TermViewportMsg {
    /// Scroll by a signed line delta. Positive scrolls toward older content.
    Scroll { term: Entity, delta: isize },
    /// Jump to the bottom of the terminal buffer.
    JumpBottom { term: Entity },
}
impl TermViewportMsg {
    /// Construct a scroll operation.
    pub fn scroll(term: Entity, delta: isize) -> Self {
        Self::Scroll { term, delta }
    }

    /// Construct a jump-to-bottom operation.
    pub fn jump_bottom(term: Entity) -> Self {
        Self::JumpBottom { term }
    }

    /// Target terminal entity.
    pub fn term(&self) -> Entity {
        match *self {
            Self::Scroll { term, .. } | Self::JumpBottom { term } => term,
        }
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
