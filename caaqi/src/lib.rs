//! caaqi: an executor for computation built on caaqi-core-scoped.
//!
//! For now it holds cells whose readers are tracked, and a linear executor
//! that reruns those readers when the cells change.

pub mod cell;
pub mod linear_executor;
pub mod synced_list;
pub mod time;
pub mod tracked_cell;
