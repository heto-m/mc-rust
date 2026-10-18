//! Right-click flow: vanilla `Minecraft#startUseItem` order, item use state and local placement prediction.

use mcr_buf::BlockPos;
use mcr_physics::{Aabb, ItemUse};
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::component::DataComponent;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::{GameType, Hand};
use mcr_protocol::types::{ItemStack, Vec3d};
use mcr_world::BlockState;

use crate::client::Client;
use crate::error::Result;
use crate::interaction::{entity_interact_swings, item_use_on_swings, item_use_swings};
use crate::inventory::OFFHAND_MENU_SLOT;
use crate::relay::LocalAction;

const LIVING_FLAGS: u8 = 8;
const MAX_USE_TICKS: i32 = 72_000;
/// `BundleItem` and `BrushItem#getUseDuration`.
const BUNDLE_BRUSH_USE_TICKS: i32 = 200;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UseTarget {
    Air,
    Block {
        pos: BlockPos,
        face: Facing,
        cursor: [f32; 3],
        inside: bool,
    },
    Entity {
        id: i32,
        location: [f64; 3],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseResult {
    Pass,
    /// Consumed without a hand swing (eating, drawing a bow, a failed placement).
    Consume,
    /// Consumed with the swing animation.
    Success,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    NotABlock,
    Failed,
    Placed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    Consume,
    Hold,
    Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UsingItem {
    pub hand: Hand,
    pub kind: UseKind,
    pub remaining: i32,
    pub started_tick: u64,
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn offset(face: Facing) -> [i32; 3] {
    match face {
        Facing::Down => [0, -1, 0],
        Facing::Up => [0, 1, 0],
        Facing::North => [0, 0, -1],
        Facing::South => [0, 0, 1],
        Facing::West => [-1, 0, 0],
        Facing::East => [1, 0, 0],
    }
}

/// Blocks whose `useWithoutItem` consumes a bare-handed click in vanilla.
fn block_consumes_use(name: &str) -> bool {
    let n = short(name);
    matches!(
        n,
        "chest"
            | "trapped_chest"
            | "ender_chest"
            | "barrel"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "crafting_table"
            | "enchanting_table"
            | "brewing_stand"
            | "beacon"
            | "hopper"
            | "dropper"
            | "dispenser"
            | "lectern"
            | "loom"
            | "cartography_table"
            | "smithing_table"
            | "stonecutter"
            | "grindstone"
            | "jukebox"
            | "note_block"
            | "bell"
            | "lever"
            | "repeater"
            | "comparator"
            | "daylight_detector"
            | "respawn_anchor"
            | "crafter"
            | "decorated_pot"
            | "chiseled_bookshelf"
            | "command_block"
            | "chain_command_block"
            | "repeating_command_block"
            | "structure_block"
            | "jigsaw"
            | "test_block"
            | "test_instance_block"
            | "composter"
            | "cake"
            | "vault"
            | "trial_spawner"
            | "sweet_berry_bush"
            | "cave_vines"
            | "cave_vines_plant"
            | "glow_lichen_placeholder"
    ) || n.ends_with("_shulker_box")
        || n == "shulker_box"
        || n.ends_with("_anvil")
        || n == "anvil"
        || n.ends_with("_bed")
        || n.ends_with("_button")
        || n.ends_with("_sign")
        || n.ends_with("_hanging_sign")
        || n.ends_with("_trapdoor") && !n.starts_with("iron")
        || n.ends_with("_door") && !n.starts_with("iron")
        || n.ends_with("_fence_gate")
        || n.ends_with("_candle_cake")
}

fn is_instant_item(name: &str) -> bool {
    let n = short(name);
    n.ends_with("_bucket") && n != "milk_bucket"
        || n == "bucket"
        || n.ends_with("_boat")
        || n.ends_with("_raft")
        || matches!(
            n,
            "snowball"
                | "egg"
                | "blue_egg"
                | "brown_egg"
                | "ender_pearl"
                | "experience_bottle"
                | "splash_potion"
                | "lingering_potion"
                | "wind_charge"
                | "fishing_rod"
                | "firework_rocket"
                | "ender_eye"
                | "writable_book"
                | "written_book"
                | "knowledge_book"
                | "filled_map"
                | "totem_of_undying_placeholder"
        )
}

impl Client {
    fn hand_stack(&self, hand: Hand) -> ItemStack {
        self.state.hand_stack(hand)
    }

    /// What `Item#use` would do locally for the stack.
    pub fn use_kind(&self, stack: &ItemStack) -> Option<(UseKind, i32)> {
        self.state.use_kind(stack)
    }

    fn block_name_at(&self, pos: BlockPos) -> Option<&'static str> {
        let state = self.state.chunks.block(pos.x, pos.y, pos.z)?;
        let info = self.state.data.block_state(state.0)?;
        self.state.data.block_name(info.block)
    }

    /// Vanilla right-click: block use, then item use, main hand before off hand.
    pub fn start_use(&mut self, target: UseTarget) -> Result<UseResult> {
        if self.digging.is_some() || self.state.player.game_mode == GameType::Spectator {
            return Ok(UseResult::Pass);
        }
        for hand in [Hand::Main, Hand::Off] {
            if let Some(result) = self.use_hand(hand, target)? {
                if result == UseResult::Success {
                    self.local_action(LocalAction::Swing {
                        hand,
                        attack: false,
                    });
                }
                return Ok(result);
            }
        }
        Ok(UseResult::Pass)
    }

    /// One hand's turn of [`Self::start_use`]; `None` passes to the next hand.
    fn use_hand(&mut self, hand: Hand, target: UseTarget) -> Result<Option<UseResult>> {
        let held = self.hand_stack(hand);
        let held_name = u32::try_from(held.item)
            .ok()
            .filter(|_| !held.is_empty())
            .and_then(|i| self.state.data.item_name(i));
        let mut block_target = false;
        match target {
            UseTarget::Entity { id, location } => {
                self.send_interact(id, hand, location)?;
                let entity = self
                    .state
                    .entities
                    .get(id)
                    .and_then(|e| u32::try_from(e.kind).ok())
                    .and_then(|k| self.state.data.entity_type_name(k));
                if entity.is_some_and(|e| short(e) == "command_block_minecart")
                    && self.state.player.can_use_game_master_blocks()
                {
                    self.push_event(crate::events::ClientEvent::MinecartEditorOpened {
                        entity_id: id,
                    });
                    return Ok(Some(UseResult::Success));
                }
                if entity.is_some_and(|e| entity_interact_swings(e, held_name)) {
                    return Ok(Some(UseResult::Success));
                }
            }
            UseTarget::Block {
                pos,
                face,
                cursor,
                inside,
            } => {
                block_target = true;
                let sneaking = self.state.player.physics.keys_sneak();
                let hands_busy = !self.hand_stack(Hand::Main).is_empty()
                    || !self.hand_stack(Hand::Off).is_empty();
                let suppress = sneaking && hands_busy;
                self.use_item_on(hand, pos, face, cursor, inside)?;
                let block = self.block_name_at(pos);
                if !suppress
                    && hand == Hand::Main
                    && self.state.player.can_use_game_master_blocks()
                    && let Some(kind) = block.and_then(crate::events::BlockEditor::from_block)
                {
                    self.push_event(crate::events::ClientEvent::BlockEditorOpened { pos, kind });
                    return Ok(Some(UseResult::Success));
                }
                if !suppress && hand == Hand::Main && block.is_some_and(block_consumes_use) {
                    return Ok(Some(UseResult::Success));
                }
                match self.try_place(hand, &held, pos, face) {
                    Placement::Placed => return Ok(Some(UseResult::Success)),
                    Placement::Failed => return Ok(Some(UseResult::Consume)),
                    Placement::NotABlock => {}
                }
                if self.state.is_item(&held, "brush") {
                    self.begin_using(hand, UseKind::Hold, BUNDLE_BRUSH_USE_TICKS, &held);
                    return Ok(Some(UseResult::Consume));
                }
                let on_cooldown = self.state.is_on_cooldown(&held);
                if let (Some(item), Some(block)) = (held_name, block)
                    && !on_cooldown
                    && item_use_on_swings(item, block)
                {
                    return Ok(Some(UseResult::Success));
                }
            }
            UseTarget::Air => {}
        }
        if !held.is_empty() {
            let kind = self.use_kind(&held);
            self.use_item(hand)?;
            if self.state.is_item(&held, "writable_book") {
                self.state.gameplay.book_editor = Some(hand);
                self.push_event(crate::events::ClientEvent::BookOpened { hand });
                return Ok(Some(UseResult::Success));
            }
            match kind {
                Some((UseKind::Instant, _)) => {
                    let gliding = self.state.player.physics.fall_flying;
                    if held_name.is_some_and(|n| item_use_swings(n, block_target, gliding)) {
                        return Ok(Some(UseResult::Success));
                    }
                }
                Some((kind, ticks)) => {
                    self.begin_using(hand, kind, ticks, &held);
                    if self.state.is_bundle(&held) {
                        return Ok(Some(UseResult::Success));
                    }
                    return Ok(Some(UseResult::Consume));
                }
                None => {}
            }
        }
        Ok(None)
    }

    fn send_interact(&mut self, id: i32, hand: Hand, location: [f64; 3]) -> Result<()> {
        let secondary = self.state.player.physics.keys_sneak();
        self.send_play(&S::Interact(sb::Interact {
            entity_id: id,
            hand,
            location: Vec3d {
                x: location[0],
                y: location[1],
                z: location[2],
            },
            using_secondary_action: secondary,
        }))
    }

    fn try_place(
        &mut self,
        hand: Hand,
        held: &ItemStack,
        pos: BlockPos,
        face: Facing,
    ) -> Placement {
        if held.is_empty() || self.state.is_on_cooldown(held) {
            return Placement::NotABlock;
        }
        let Some(block) = u32::try_from(held.item)
            .ok()
            .and_then(|i| self.state.data.item_info(i))
            .and_then(|i| i.block)
        else {
            return Placement::NotABlock;
        };
        let data = self.state.data;
        let Some(clicked) = self.state.chunks.block(pos.x, pos.y, pos.z) else {
            return Placement::NotABlock;
        };
        let replaceable = data
            .block_state(clicked.0)
            .is_some_and(|i| i.is_replaceable());
        let at = if replaceable {
            pos
        } else {
            let o = offset(face);
            BlockPos::new(pos.x + o[0], pos.y + o[1], pos.z + o[2])
        };
        let Some(existing) = self.state.chunks.block(at.x, at.y, at.z) else {
            return Placement::NotABlock;
        };
        let free = data
            .block_state(existing.0)
            .is_some_and(|i| i.is_air() || i.is_replaceable());
        if !free {
            return Placement::Failed;
        }
        let Some(info) = data.block_info(block) else {
            return Placement::Placed;
        };
        let state = info.default_state;
        if data.state_properties(state).count() != 0 {
            self.combat.placed = Some((at, state));
            return Placement::Placed;
        }
        let bb = self.state.player.physics.bounding_box();
        let blocked = data.collision_shape(state).iter().any(|b| {
            let shape = Aabb::new(
                [
                    f64::from(at.x) + f64::from(b.min[0]),
                    f64::from(at.y) + f64::from(b.min[1]),
                    f64::from(at.z) + f64::from(b.min[2]),
                ],
                [
                    f64::from(at.x) + f64::from(b.max[0]),
                    f64::from(at.y) + f64::from(b.max[1]),
                    f64::from(at.z) + f64::from(b.max[2]),
                ],
            );
            shape.intersects(&bb)
        });
        if blocked {
            return Placement::Failed;
        }
        let sequence = self.sequence;
        self.state.predict_block(at, BlockState(state), sequence);
        self.combat.placed = Some((at, state));
        if !self.state.player.instabuild {
            let slot = match hand {
                Hand::Main => {
                    crate::inventory::HOTBAR_START + usize::from(self.state.player.held_slot)
                }
                Hand::Off => OFFHAND_MENU_SLOT,
            };
            if let Some(s) = self.state.inventory.player.slots.get_mut(slot) {
                s.count -= 1;
                if s.count <= 0 {
                    *s = ItemStack::default();
                }
            }
        }
        Placement::Placed
    }

    fn begin_using(&mut self, hand: Hand, kind: UseKind, ticks: i32, stack: &ItemStack) {
        if kind == UseKind::Instant || self.using.is_some() {
            return;
        }
        self.using = Some(UsingItem {
            hand,
            kind,
            remaining: ticks.clamp(0, MAX_USE_TICKS),
            started_tick: self.ticks,
        });
        let effects = stack
            .components
            .added
            .iter()
            .find_map(|c| match c {
                DataComponent::UseEffects(u) => Some(ItemUse {
                    can_sprint: u.can_sprint,
                    speed_multiplier: if u.speed_multiplier.is_finite() {
                        u.speed_multiplier.clamp(0.0, 1.0)
                    } else {
                        0.2
                    },
                }),
                _ => None,
            })
            .unwrap_or_default();
        self.state.player.physics.using_item = Some(effects);
    }

    pub fn using_item(&self) -> Option<&UsingItem> {
        self.using.as_ref()
    }

    pub(crate) fn stop_using_local(&mut self) {
        self.using = None;
        self.state.player.physics.using_item = None;
    }

    /// Releases the use key: ends the local state and tells the server (bows, tridents, shields).
    pub fn release_use(&mut self) -> Result<()> {
        let Some(using) = self.using else {
            return Ok(());
        };
        let stack = self.hand_stack(using.hand);
        let held = self.ticks.saturating_sub(using.started_tick);
        if held >= 10 && self.state.is_item(&stack, "trident") {
            let strength = self.riptide_strength(&stack);
            if strength > 0.0 && self.in_water_or_rain() && !self.state.player.physics.passenger {
                self.start_riptide(strength);
            }
        }
        self.stop_using_local();
        self.release_use_item()
    }

    /// Per-tick use countdown. Like `LocalPlayer#onSyncedDataUpdated`, only a change of the server's
    /// using flag starts or stops the local use, so a stale flag never restarts a released one.
    pub(crate) fn update_using_item(&mut self) {
        let (server, hand) = match self.state.player.data.get(&LIVING_FLAGS) {
            Some(EntityDataValue::Byte(b)) => {
                (b & 1 != 0, if b & 2 != 0 { Hand::Off } else { Hand::Main })
            }
            _ => (false, Hand::Main),
        };
        let was = std::mem::replace(&mut self.combat.server_using, server);
        let Some(u) = self.using else {
            if server && !was {
                let stack = self.hand_stack(hand);
                if !stack.is_empty() {
                    let (kind, ticks) = match self.use_kind(&stack) {
                        Some((UseKind::Instant, _)) | None => (UseKind::Hold, MAX_USE_TICKS),
                        Some(k) => k,
                    };
                    self.begin_using(hand, kind, ticks, &stack);
                }
            }
            return;
        };
        if was && !server {
            self.stop_using_local();
            return;
        }
        let held = self.hand_stack(u.hand);
        if held.is_empty() {
            self.stop_using_local();
            return;
        }
        let remaining = u.remaining - 1;
        if remaining <= 0 && u.kind == UseKind::Consume {
            self.stop_using_local();
        } else if let Some(using) = self.using.as_mut() {
            using.remaining = remaining.max(0);
        }
    }
}

pub(crate) fn seconds_to_ticks(seconds: f32) -> i32 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds.min(MAX_USE_TICKS as f32 / 20.0) * 20.0).ceil() as i32
}

impl crate::state::ClientState {
    pub fn hand_stack(&self, hand: Hand) -> ItemStack {
        match hand {
            Hand::Main => self.held_stack().clone(),
            Hand::Off => self
                .inventory
                .player
                .slots
                .get(OFFHAND_MENU_SLOT)
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn is_bundle(&self, stack: &ItemStack) -> bool {
        u32::try_from(stack.item)
            .ok()
            .and_then(|i| self.data.item_name(i))
            .is_some_and(|n| short(n).ends_with("bundle"))
    }

    pub(crate) fn is_item(&self, stack: &ItemStack, name: &str) -> bool {
        u32::try_from(stack.item)
            .ok()
            .and_then(|i| self.data.item_name(i))
            .is_some_and(|n| short(n) == name)
    }

    fn has_projectile(&self, names: &[&str]) -> bool {
        self.player.instabuild
            || self.inventory.player.slots.iter().any(|s| {
                !s.is_empty()
                    && u32::try_from(s.item)
                        .ok()
                        .and_then(|i| self.data.item_name(i))
                        .is_some_and(|n| names.contains(&short(n)))
            })
    }

    pub fn use_kind(&self, stack: &ItemStack) -> Option<(UseKind, i32)> {
        if stack.is_empty() || self.is_on_cooldown(stack) {
            return None;
        }
        let item = u32::try_from(stack.item).ok()?;
        let data = self.data;
        let name = short(data.item_name(item)?);
        let consume = stack
            .components
            .added
            .iter()
            .find_map(|c| match c {
                DataComponent::Consumable(c) => Some(c.consume_seconds),
                _ => None,
            })
            .or_else(|| data.item_consume_seconds(item));
        if let Some(seconds) = consume {
            let food = stack
                .components
                .added
                .iter()
                .find_map(|c| match c {
                    DataComponent::Food(f) => Some(f.can_always_eat),
                    _ => None,
                })
                .or_else(|| data.item_food(item));
            if let Some(always) = food
                && !(always || self.player.instabuild || self.player.food < 20)
            {
                return None;
            }
            return Some((UseKind::Consume, seconds_to_ticks(seconds)));
        }
        match name {
            "bow" => self
                .has_projectile(&["arrow", "tipped_arrow", "spectral_arrow"])
                .then_some((UseKind::Hold, MAX_USE_TICKS)),
            "crossbow" => {
                let charged =
                    stack.components.added.iter().any(
                        |c| matches!(c, DataComponent::ChargedProjectiles(p) if !p.0.is_empty()),
                    );
                if charged {
                    Some((UseKind::Instant, 0))
                } else {
                    self.has_projectile(&[
                        "arrow",
                        "tipped_arrow",
                        "spectral_arrow",
                        "firework_rocket",
                    ])
                    .then_some((UseKind::Hold, MAX_USE_TICKS))
                }
            }
            "trident" | "shield" | "spyglass" | "goat_horn" => Some((UseKind::Hold, MAX_USE_TICKS)),
            _ if name.ends_with("bundle") => Some((UseKind::Hold, BUNDLE_BRUSH_USE_TICKS)),
            _ if self.has_component(stack, "minecraft:blocks_attacks")
                || self.has_component(stack, "minecraft:kinetic_weapon") =>
            {
                Some((UseKind::Hold, MAX_USE_TICKS))
            }
            _ if is_instant_item(name) => Some((UseKind::Instant, 0)),
            _ if data.item_equip_slot(item).is_some_and(|s| s != "mainhand") => {
                Some((UseKind::Instant, 0))
            }
            _ => None,
        }
    }
}
