//! The renderpearl backend: Java's GPU calls recorded as wgpu work on mcr's device.

pub mod backend;
pub mod context;
pub mod enums;
pub mod ffi;
pub mod flip;
pub mod info;
pub mod pass;
pub mod pipeline;
pub mod resources;
pub mod shader;
pub mod slab;
pub mod slots;
pub mod spv;
#[cfg(test)]
mod tests;

pub use backend::{Image, StatsSnapshot, backend, capture_next_present, take_capture};
pub use context::{GpuError, install};
