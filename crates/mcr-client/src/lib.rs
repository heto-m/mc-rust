//! Renderer-independent game client: connection driver, game state, 20 TPS tick and actions.

mod actions;
pub mod attach;
#[cfg(test)]
mod audit_tests;
mod batch;
mod chat_in;
mod client;
pub mod commands;
mod conduct;
mod config;
mod dialogs;
mod digging;
#[cfg(test)]
mod digging_tests;
mod edits;
mod entity;
mod entity_events;
mod error;
mod events;
mod gamestate;
#[cfg(test)]
mod gamestate_tests;
mod glide;
mod handler;
#[cfg(test)]
mod handler_tests;
pub mod hashing;
#[cfg(test)]
mod hashing_tests;
mod interaction;
mod inventory;
#[cfg(test)]
mod lifecycle_tests;
pub mod locator;
mod menu;
#[cfg(test)]
mod menu_layout_tests;
#[cfg(test)]
mod menu_tests;
mod packs;
mod pickup;
mod player;
mod probe;
mod registry;
mod relay;
mod riding;
pub mod script;
pub mod secure_chat;
mod slot_display;
mod social;
mod state;
mod toast;
mod use_item;
#[cfg(test)]
mod use_item_tests;

pub use actions::Digging;
pub use batch::ChunkBatchSizeCalculator;
pub use client::{Client, Phase};
pub use commands::{CommandTree, Completion, SuggestSource, complete_chat};
pub use conduct::{
    ConductEnd, ConductLink, ConductReply, ConductRequest, conduct_channel, conduct_hash,
};
pub use config::{ClientConfig, OnlineAuth, ResourcePackPolicy, TransferTarget, WireChoice};
pub use dialogs::{
    DIALOG_REGISTRY, DialogEnd, DialogLink, DialogReply, DialogRequest, dialog_channel,
    resolve_dialog,
};
pub use entity::{AttributeState, Entities, Entity, FlightHistory, MobEffect, SimShape};
pub use error::{ClientError, Result};
pub use events::{
    BlockEditor, ChatKind, ChatTag, ClientEvent, DamageEffects, DisconnectReason, sanitize_for_log,
};
pub use gamestate::{
    Advancements, GameplayState, GhostRecipe, MapData, Merchant, RecipeBook, SignEditor, Waypoints,
};
pub use inventory::{
    Container, Inventory, OUTSIDE_SLOT, PLAYER_MENU_ID, ServerUpdate, inventory_index_to_menu_slot,
};
pub use mcr_physics::PlayerInput;
pub use menu::MountInfo;
pub use packs::{PackEnd, PackLink, PackReply, PackRequest, pack_channel};
pub use pickup::Pickup;
pub use player::LocalPlayer;
pub use probe::{ServerVersion, probe};
pub use registry::{BuiltinData, Dimension, Registries, Registry, RegistryItem, data_pack_path};
pub use relay::{ClientHooks, LocalAction, Relay, Relayed};
pub use riding::{Ride, Sim};
pub use secure_chat::TextRenderer;
pub use slot_display::{holder_items, resolve as resolve_slot_display};
pub use social::{
    BossBar, ChatMessage, ChatState, MAX_SIDEBAR_ROWS, MAX_TAB_ENTRIES, Objective, PlayerInfo,
    PlayerList, Score, ScoreStyle, Scoreboard, SidebarRow, TabList, Team, Titles,
    boss_bars_ordered, format_score, tab_name, team_named_color,
};
pub use state::{
    ClientState, CommandReply, Outbox, ServerInfo, SpawnPoint, TransientBlock, WorldBorder,
    WorldInfo, is_trusted_url,
};
pub use toast::{ToastEvent, first_stack};
pub use use_item::{UseKind, UseResult, UseTarget, UsingItem};
