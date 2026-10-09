//! Deterministic client-side player movement matching vanilla Minecraft's local player tick.

#![allow(clippy::needless_range_loop)]

mod aabb;
mod collide;
mod data;
mod effects;
mod fluid;
mod mover;
mod player;
mod tick;
mod trig;
mod types;
mod world;

pub use aabb::Aabb;
pub use data::{PhysicsData, StateInfo, flag};
pub use player::PlayerPhysics;
pub use types::{Abilities, Attributes, Effects, MoveKind, MovementOutput, PlayerInput, Pose};
pub use world::CollisionWorld;

#[cfg(test)]
mod tests;
