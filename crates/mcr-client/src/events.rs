use mcr_buf::{BlockPos, Identifier};
use mcr_protocol::play::clientbound::world::{ExplosionParticle, SoundSource};
use mcr_protocol::types::Component;
use mcr_protocol::types::{Particle, SoundEvent};
use mcr_world::ChunkPos;
use uuid::Uuid;

pub const MAX_EXPLOSION_PARTICLE_KINDS: usize = 64;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageEffects {
    Hurt,
    Thorns,
    Drowning,
    Burning,
    Poking,
    Freezing,
}

impl DamageEffects {
    pub fn from_name(name: &str) -> Self {
        match name {
            "thorns" => Self::Thorns,
            "drowning" => Self::Drowning,
            "burning" => Self::Burning,
            "poking" => Self::Poking,
            "freezing" => Self::Freezing,
            _ => Self::Hurt,
        }
    }

    /// Known-pack entries arrive without their data, so vanilla's own types are resolved by name.
    pub fn vanilla_default(path: &str) -> Self {
        match path {
            "campfire"
            | "fireball"
            | "hot_floor"
            | "in_fire"
            | "lava"
            | "on_fire"
            | "sulfur_cube_hot"
            | "unattributed_fireball" => Self::Burning,
            "drown" => Self::Drowning,
            "freeze" => Self::Freezing,
            "sweet_berry_bush" => Self::Poking,
            "thorns" => Self::Thorns,
            _ => Self::Hurt,
        }
    }

    pub fn player_hurt_sound(self) -> &'static str {
        match self {
            Self::Hurt | Self::Thorns => "entity.player.hurt",
            Self::Drowning => "entity.player.hurt_drown",
            Self::Burning => "entity.player.hurt_on_fire",
            Self::Poking => "entity.player.hurt_sweet_berry_bush",
            Self::Freezing => "entity.player.hurt_freeze",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEditor {
    CommandBlock,
    StructureBlock,
    Jigsaw,
    TestBlock,
    TestInstanceBlock,
}

impl BlockEditor {
    pub fn from_block(name: &str) -> Option<Self> {
        Some(match name.strip_prefix("minecraft:").unwrap_or(name) {
            "command_block" | "chain_command_block" | "repeating_command_block" => {
                Self::CommandBlock
            }
            "structure_block" => Self::StructureBlock,
            "jigsaw" => Self::Jigsaw,
            "test_block" => Self::TestBlock,
            "test_instance_block" => Self::TestInstanceBlock,
            _ => return None,
        })
    }
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
    Toast(crate::toast::ToastEvent),
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
    /// Right-clicking an operator block as a creative operator opens its editor on the client.
    BlockEditorOpened {
        pos: BlockPos,
        kind: BlockEditor,
    },
    MinecartEditorOpened {
        entity_id: i32,
    },
    TestInstanceStatus {
        status: Component,
        size: Option<[i32; 3]>,
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
    /// Game event 4: the End poem and credits.
    ShowCredits,
    /// The `animate` packet: 0 wake up, 1 critical hit, 2 magic critical hit.
    EntityAnimation {
        entity_id: i32,
        action: u8,
    },
    EntitySwing {
        entity_id: i32,
        offhand: bool,
    },
    EntityDamaged {
        entity_id: i32,
        source_type: i32,
        effects: DamageEffects,
    },
    EntityHurt {
        entity_id: i32,
    },
    ItemTaken {
        item_id: i32,
        collector_id: i32,
        amount: i32,
        pos: [f64; 3],
        experience_orb: bool,
        silent: bool,
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
        /// Raised by the client for its own action rather than sent by the server.
        predicted: bool,
    },
    Explosion {
        center: [f64; 3],
        radius: f32,
        particle: Particle,
        /// `None` when the server asked for a silent explosion.
        sound: Option<mcr_buf::IdOr<SoundEvent>>,
        block_count: i32,
        block_particles: Vec<ExplosionParticle>,
    },
    /// A sound the client plays for itself, e.g. from a game event.
    LocalSound {
        sound: &'static str,
        pos: [f64; 3],
        volume: f32,
        pitch: f32,
        source: SoundSource,
    },
    /// Configuration's `reset_chat`: the chat starts empty after reconfiguring.
    ChatCleared,
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
    PlayerRotated,
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
    fn vanilla_damage_types_pick_their_player_sounds() {
        let sound = |t| DamageEffects::vanilla_default(t).player_hurt_sound();
        assert_eq!(sound("on_fire"), "entity.player.hurt_on_fire");
        assert_eq!(sound("drown"), "entity.player.hurt_drown");
        assert_eq!(sound("fall"), "entity.player.hurt");
    }

    #[test]
    fn escapes_csi_osc_and_newlines() {
        let s = sanitize_for_log("a\u{1b}[2J\u{1b}]0;pwned\u{7}\nb\u{9b}c\u{7f}");
        assert!(!s.chars().any(char::is_control));
        assert_eq!(s, "a\\u{1b}[2J\\u{1b}]0;pwned\\u{7}\\u{a}b\\u{9b}c\\u{7f}");
    }

    #[test]
    fn operator_blocks_have_editors() {
        for block in [
            "minecraft:command_block",
            "chain_command_block",
            "repeating_command_block",
        ] {
            assert_eq!(
                BlockEditor::from_block(block),
                Some(BlockEditor::CommandBlock)
            );
        }
        assert_eq!(BlockEditor::from_block("jigsaw"), Some(BlockEditor::Jigsaw));
        assert_eq!(
            BlockEditor::from_block("test_instance_block"),
            Some(BlockEditor::TestInstanceBlock)
        );
        assert_eq!(BlockEditor::from_block("chest"), None);
    }
}
