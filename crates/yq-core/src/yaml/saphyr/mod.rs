//! Vendored copy of `saphyr-parser` 0.2.2 (MIT OR Apache-2.0, see LICENSE in
//! this directory), an event-based YAML 1.2 parser.
//!
//! It is vendored rather than depended on so that anchor names can be exposed
//! on events (upstream only exposes numeric anchor IDs) and so that a
//! comment-aware scanner can be added later without forking the crate again.
//! Local changes: `Parser::anchor_name`, removal of the `thiserror` and
//! `arraydeque` dependencies, and the buffered iterator input.

#![allow(clippy::all, clippy::pedantic, dead_code, unused_imports)]

mod char_traits;
#[macro_use]
mod debug;
pub mod input;
mod parser;
mod scanner;

pub use input::{str::StrInput, Input};
pub use parser::{Event, EventReceiver, Parser, SpannedEventReceiver, Tag};
pub use scanner::{Marker, ScalarStyle, ScanError, Span};
