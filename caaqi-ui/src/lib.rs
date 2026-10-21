//! UI built on caaqi.
//!
//! [`run`] opens a window and draws into it with [vello](::vello).

pub mod cell;
pub mod linear_executor;
pub mod synced_array;
pub mod tracked_cell;
pub mod window;

pub use window::{RunError, run};
// Re-exported so users get the versions this crate is built with.
pub use vello;
pub use winit;
