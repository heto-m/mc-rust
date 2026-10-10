//! Deterministic client-side player movement matching vanilla Minecraft's local player tick.

#![allow(clippy::needless_range_loop)]

mod aabb;
mod body;
mod collide;
mod data;
mod effects;
mod fluid;
mod fly;
mod mover;
mod player;
mod tick;
mod trig;
mod types;
mod vehicle;
mod world;

pub use aabb::Aabb;
pub use body::{Body, BodyKind, BodyScratch, aim, fluid_height};
pub use data::{PhysicsData, StateInfo, flag};
pub use fly::RIPTIDE_SPIN_TICKS;
pub use player::PlayerPhysics;
pub use types::{
    Abilities, Attributes, Effects, ItemUse, MoveKind, MovementOutput, PlayerInput, Pose,
};
pub use vehicle::{Boat, BoatStatus, Mount, MountMode};
pub use world::CollisionWorld;

#[cfg(test)]
mod fly_tests;
#[cfg(test)]
mod tests;
