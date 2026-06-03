//! A terminal emulator for Bevy.
//!
//! Contains a terminal emulator, process simulation, and a basic shell.
//!
//! ## Features
//! - Bevy-ui based terminal rendering
//! - Rich ANSI parsing
//! - Process management
//! - A minimal shell interface
//! - Shell job management
//!
//! ## Non-features
//! - Multiplexing
//! - Pipes and redirection
//! - Filesystem I/O
//! - Raw mode - TUIs are not yet supported

mod ansi;
mod data;
mod messages;
mod plugin;
mod systems;
pub mod prelude {
    pub use super::ansi::*;
    pub use super::data::*;
    pub use super::messages::*;
    pub use super::plugin::*;
    pub use super::systems::*;
    pub use bevy::prelude::*;
    pub use tiny_bail::prelude::*;
}
