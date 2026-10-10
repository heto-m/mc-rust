//! Renderer-independent game client: connection driver, game state, 20 TPS tick and actions.

mod actions;
mod batch;
mod client;
pub mod commands;
mod config;
mod entity;
mod error;
mod events;
mod handler;
mod inventory;
mod player;
mod registry;
pub mod script;
mod social;
mod state;

pub use actions::Digging;
pub use batch::ChunkBatchSizeCalculator;
pub use client::{Client, Phase};
pub use commands::{CommandTree, Completion, SuggestSource};
pub use config::{ClientConfig, OnlineAuth, ResourcePackPolicy};
pub use entity::{AttributeState, Entities, Entity, MobEffect};
pub use error::{ClientError, Result};
pub use events::{ChatKind, ClientEvent, DisconnectReason, sanitize_for_log};
pub use inventory::{Container, Inventory};
pub use mcr_physics::PlayerInput;
pub use player::LocalPlayer;
pub use registry::{Dimension, Registries, Registry, RegistryItem};
pub use social::{
    BossBar, ChatMessage, ChatState, Objective, PlayerInfo, PlayerList, Score, Scoreboard, TabList,
    Team, Titles,
};
pub use state::{
    ClientState, CommandReply, Outbox, ServerInfo, SpawnPoint, WorldBorder, WorldInfo,
};
