use mcr_buf::{BlockPos, Identifier};
use mcr_protocol::play::clientbound::world::SoundSource;
use mcr_protocol::types::Component;
use mcr_protocol::types::{Particle, SoundEvent};
use mcr_world::ChunkPos;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum DisconnectReason {
    Server(Component),
    Network(String),
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatKind {
    System,
    Player,
    Disguised,
    ActionBar,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Joined {
        entity_id: i32,
    },
    Respawned,
    Reconfiguring,
    Transfer {
        host: String,
        port: u16,
    },
    Disconnected(DisconnectReason),

    RecipeBookUpdated,
    AdvancementsUpdated,
    MapUpdated(i32),
    MerchantOffersUpdated,
    StatsUpdated,
    SignEditorOpened {
        pos: BlockPos,
        side: mcr_protocol::types::game::SignSide,
    },
    BookOpened {
        hand: mcr_protocol::types::game::Hand,
    },

    ChunkLoaded(ChunkPos),
    ChunkUnloaded(ChunkPos),
    LightUpdated(ChunkPos),
    BlockChanged {
        pos: BlockPos,
        state: u32,
    },
    BlockEntityChanged(BlockPos),
    BlockEvent {
        pos: BlockPos,
        b0: u8,
        b1: u8,
        block: i32,
    },
    BlockDestruction {
        entity_id: i32,
        pos: BlockPos,
        progress: u8,
    },
    WeatherChanged,
    TimeChanged,

    EntityAdded(i32),
    EntityRemoved(i32),
    EntityMoved(i32),
    EntityDataChanged(i32),
    EntityEquipmentChanged(i32),
    EntityEvent {
        entity_id: i32,
        event: i8,
    },
    EntityAnimation {
        entity_id: i32,
        action: u8,
    },
    EntityDamaged {
        entity_id: i32,
        source_type: i32,
    },
    ItemTaken {
        item_id: i32,
        collector_id: i32,
        amount: i32,
    },

    Chat {
        kind: ChatKind,
        sender: Option<Uuid>,
        message: Component,
    },
    Sound {
        sound: mcr_buf::IdOr<SoundEvent>,
        source: SoundSource,
        pos: Option<[f64; 3]>,
        entity_id: Option<i32>,
        volume: f32,
        pitch: f32,
    },
    StopSound {
        source: Option<SoundSource>,
        name: Option<Identifier>,
    },
    Particle {
        particle: Particle,
        pos: [f64; 3],
        count: i32,
        delta: [f32; 3],
        speed: [f32; 3],
        always_show: bool,
    },
    LevelEvent {
        event: i32,
        pos: BlockPos,
        data: i32,
        global: bool,
    },
    Explosion {
        center: [f64; 3],
        radius: f32,
    },
    Title {
        title: Option<Component>,
        subtitle: Option<Component>,
    },
    TitlesCleared,
    PlayerListChanged,
    TabListChanged,
    BossBarChanged(Uuid),
    ScoreboardChanged,

    PlayerTeleported {
        id: i32,
    },
    HealthChanged,
    ExperienceChanged,
    GameModeChanged,
    AbilitiesChanged,
    Died {
        message: Option<Component>,
    },
    HeldSlotChanged(u8),
    InventoryChanged,
    ContainerOpened {
        id: i32,
        menu_type: i32,
        title: Component,
    },
    ContainerUpdated(i32),
    ContainerClosed(i32),
    BlockAckReceived(i32),
}

/// Makes server-controlled text safe for a terminal: control characters (C0, DEL, C1, including ESC) become `\u{..}` escapes.
pub fn sanitize_for_log(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

impl DisconnectReason {
    pub fn to_log_string(&self) -> String {
        match self {
            Self::Server(c) => sanitize_for_log(&c.to_plain_string()),
            Self::Network(m) => sanitize_for_log(m),
            Self::Local => "local".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_csi_osc_and_newlines() {
        let s = sanitize_for_log("a\u{1b}[2J\u{1b}]0;pwned\u{7}\nb\u{9b}c\u{7f}");
        assert!(!s.chars().any(char::is_control));
        assert_eq!(s, "a\\u{1b}[2J\\u{1b}]0;pwned\\u{7}\\u{a}b\\u{9b}c\\u{7f}");
    }
}
