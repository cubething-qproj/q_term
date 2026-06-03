//! Events which modify the virtual terminal display.
use crate::prelude::*;

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
