//! A terminal emulator for Bevy.
//!
//! Contains a terminal emulator. Process simulation and shell
//! interfaces have moved out of this crate -- see `q_proc` for
//! process management and the shell crate for shell semantics.
//!
//! ## Features
//! - Bevy-ui based terminal rendering
//! - Rich ANSI parsing
//!
//! ## Non-features
//! - Multiplexing
//! - Pipes and redirection
//! - Filesystem I/O
//! - Raw mode - TUIs are not yet supported

mod ansi;
mod data;
pub mod msgs;
mod plugins;
pub mod systems;
pub mod prelude {
    pub use super::ansi::*;
    pub use super::data::prelude::*;
    pub use super::plugins::*;
    pub use bevy::prelude::*;
    pub use tiny_bail::prelude::*;
}
