//! Vanilla `Minecraft#handleKeybinds` for the attack and use keys: miss lockout, the use repeat,
//! per-tick mining and the attack strength ticker.

use mcr_buf::BlockPos;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, component_type_id};
use mcr_protocol::types::game::{GameType, Hand};

use crate::client::Client;
use crate::error::Result;
use crate::relay::LocalAction;
use crate::state::ClientState;
use crate::use_item::{UseResult, UseTarget};

pub const MISS_TICKS: i32 = 10;
const SCREEN_MISS_TICKS: i32 = 10_000;
const USE_REPEAT_TICKS: u8 = 4;
const DEFAULT_ATTACK_SPEED: f64 = 4.0;

#[derive(Debug, Clone, Default)]
pub(crate) struct Combat {
    miss_time: i32,
    right_click_delay: u8,
    pub(crate) hands_busy: bool,
    /// The server's using flag as last seen, for edge detection.
    pub(crate) server_using: bool,
    attack_ticker: u32,
    swap_ticker: u32,
    last_main_item: i32,
    pub(crate) placed: Option<(BlockPos, u32)>,
}

/// One tick of the attack and use keys: presses since the last tick and whether each is held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CombatKeys {
    pub attack_clicks: u8,
    pub use_clicks: u8,
    pub attack_down: bool,
    pub use_down: bool,
}

/// What [`Client::handle_keybinds`] did, for the hand animation and local sounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CombatOutcome {
    /// The main hand swung (attack, miss, stab, dig) or a use asked for a swing.
    pub swing: bool,
    /// A predicted block placement: where, and the placed state.
    pub placed: Option<(BlockPos, u32)>,
    /// The dig tick on which a hit sound is due.
    pub dig_hit: Option<BlockPos>,
    /// A piercing weapon stabbed; its swing sound is the client's to play.
    pub stab: bool,
    pub released: bool,
}

fn item_key(stack: &ItemStack) -> i32 {
    if stack.is_empty() { 0 } else { stack.item }
}

impl ClientState {
    /// Whether the stack's effective components (item defaults edited by its patch) include `name`.
    pub fn has_component(&self, stack: &ItemStack, name: &str) -> bool {
        if stack.is_empty() {
            return false;
        }
        let Some(id) = component_type_id(name) else {
            return false;
        };
        if stack.components.removed.contains(&id) {
            return false;
        }
        stack.components.added.iter().any(|c| c.type_id() == id)
            || u32::try_from(stack.item)
                .is_ok_and(|i| self.data.item_has_default_component(i, id as u32))
    }

    fn minimum_attack_charge(&self, stack: &ItemStack) -> f32 {
        let patched = stack.components.added.iter().find_map(|c| match c {
            DataComponent::MinimumAttackCharge(v) => Some(*v),
            _ => None,
        });
        match patched {
            Some(v) if v.is_finite() => v,
            Some(_) => 0.0,
            // Only spears carry it by default, all with 1.0; the tables keep presence, not values.
            None if self.has_component(stack, "minecraft:minimum_attack_charge") => 1.0,
            None => 0.0,
        }
    }

    /// Vanilla `getCurrentItemAttackStrengthDelay`: ticks for a full-strength attack.
    pub fn attack_strength_delay(&self) -> f32 {
        let speed = self
            .attribute("attack_speed", DEFAULT_ATTACK_SPEED)
            .clamp(0.1, 1024.0);
        (20.0 / speed) as f32
    }
}

impl Client {
    pub fn is_using_item(&self) -> bool {
        self.using.is_some()
    }

    /// Vanilla `getAttackStrengthScale`.
    pub fn attack_strength_scale(&self, partial: f32) -> f32 {
        let delay = self.state.attack_strength_delay();
        ((self.combat.attack_ticker as f32 + partial) / delay).clamp(0.0, 1.0)
    }

    /// Vanilla `getItemSwapScale`, which lowers the first-person main hand.
    pub fn item_swap_scale(&self, partial: f32) -> f32 {
        let delay = self.state.attack_strength_delay();
        ((self.combat.swap_ticker as f32 + partial) / delay).clamp(0.0, 1.0)
    }

    /// Ticks the miss lockout still blocks attacks.
    pub fn miss_time(&self) -> i32 {
        self.combat.miss_time
    }

    /// `Player#tick`: the tickers count up and restart when the main hand holds a different item.
    pub(crate) fn tick_attack_strength(&mut self) {
        let c = &mut self.combat;
        c.attack_ticker = c.attack_ticker.saturating_add(1);
        c.swap_ticker = c.swap_ticker.saturating_add(1);
        let held = item_key(self.state.held_stack());
        if held != self.combat.last_main_item {
            self.combat.last_main_item = held;
            self.reset_attack_strength();
        }
    }

    /// `resetAttackStrengthTicker`; a mirror hears it too.
    pub fn reset_attack_strength(&mut self) {
        self.combat.attack_ticker = 0;
        self.combat.swap_ticker = 0;
        self.local_action(LocalAction::ResetAttackStrength);
    }

    fn cannot_attack_with(&self, stack: &ItemStack) -> bool {
        let required = self.state.minimum_attack_charge(stack);
        let strength = self.combat.attack_ticker as f32 / self.state.attack_strength_delay();
        required > 0.0 && strength < required
    }

    fn block_is_air(&self, pos: BlockPos) -> bool {
        self.state
            .chunks
            .block(pos.x, pos.y, pos.z)
            .and_then(|b| self.state.data.block_state(u32::from(b.0)))
            .is_none_or(|i| i.is_air())
    }

    fn attack_swing(&mut self) -> Result<()> {
        self.local_action(LocalAction::Swing {
            hand: Hand::Main,
            attack: true,
        });
        self.send_play(&S::Punch(sb::Punch))
    }

    /// Vanilla `MultiPlayerGameMode#piercingAttack`: the server runs the stab, the client only swings.
    fn piercing_attack(&mut self) -> Result<()> {
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::Stab,
            pos: BlockPos::new(0, 0, 0),
            face: Facing::Down,
            sequence: 0,
        }))?;
        self.combat.attack_ticker = 0;
        self.local_action(LocalAction::ResetAttackStrength);
        self.local_action(LocalAction::Swing {
            hand: Hand::Main,
            attack: true,
        });
        Ok(())
    }

    /// Vanilla `MultiPlayerGameMode#attack`.
    fn attack_entity(&mut self, id: i32) -> Result<()> {
        self.attack(id)?;
        self.reset_attack_strength();
        if self.state.player.instabuild {
            self.destroy_delay = 5;
        }
        Ok(())
    }

    /// Vanilla `Minecraft#startAttack`; true when the click broke its block at once.
    fn start_attack(&mut self, hit: UseTarget, out: &mut CombatOutcome) -> Result<bool> {
        if self.combat.miss_time > 0 || self.combat.hands_busy {
            return Ok(false);
        }
        if self.state.player.game_mode == GameType::Spectator {
            return Ok(false);
        }
        let held = self.state.held_stack().clone();
        if self.cannot_attack_with(&held) {
            return Ok(false);
        }
        if self.state.has_component(&held, "minecraft:piercing_weapon") {
            self.piercing_attack()?;
            out.swing = true;
            out.stab = true;
            return Ok(true);
        }
        let mut broke = false;
        let block = match hit {
            UseTarget::Block { pos, face, .. } if !self.block_is_air(pos) => Some((pos, face)),
            _ => None,
        };
        match (hit, block) {
            (UseTarget::Entity { id, .. }, _) => self.attack_entity(id)?,
            (_, Some((pos, face))) => {
                self.start_destroy(pos, face)?;
                broke = self.block_is_air(pos);
            }
            _ => {
                if !self.state.player.instabuild {
                    self.combat.miss_time = MISS_TICKS;
                }
                self.reset_attack_strength();
            }
        }
        self.attack_swing()?;
        out.swing = true;
        Ok(broke)
    }

    /// Vanilla `Minecraft#continueAttack`: holding attack on a block mines it a tick further.
    fn continue_attack(
        &mut self,
        down: bool,
        hit: UseTarget,
        out: &mut CombatOutcome,
    ) -> Result<()> {
        if !down {
            self.combat.miss_time = 0;
        }
        if self.combat.miss_time > 0 || self.is_using_item() {
            return Ok(());
        }
        let held = self.state.held_stack().clone();
        if self.state.has_component(&held, "minecraft:piercing_weapon") {
            return Ok(());
        }
        match hit {
            UseTarget::Block { pos, face, .. } if down => {
                if self.block_is_air(pos) {
                    return Ok(());
                }
                if self.continue_destroy(pos, face)? {
                    self.attack_swing()?;
                    out.swing = true;
                    if self
                        .digging
                        .as_ref()
                        .is_some_and(|d| d.pos == pos && d.ticks % 4 == 0)
                    {
                        out.dig_hit = Some(pos);
                    }
                }
            }
            _ => self.stop_destroy()?,
        }
        Ok(())
    }

    /// Vanilla `Minecraft#startUseItem`: never while mining, and it arms the 4-tick repeat.
    fn start_use_item(&mut self, hit: UseTarget, out: &mut CombatOutcome) -> Result<()> {
        if self.digging.is_some() {
            return Ok(());
        }
        self.combat.right_click_delay = USE_REPEAT_TICKS;
        if self.combat.hands_busy {
            return Ok(());
        }
        self.combat.placed = None;
        let result = self.start_use(hit)?;
        if result == UseResult::Success {
            out.swing = true;
            out.placed = out.placed.or(self.combat.placed.take());
        }
        Ok(())
    }

    /// One tick of the attack and use keys in vanilla's order. While an item is in use the clicks
    /// are dropped and releasing the use key ends the use; `screen_open` stops everything.
    pub fn handle_keybinds(
        &mut self,
        keys: CombatKeys,
        hit: UseTarget,
        screen_open: bool,
    ) -> Result<CombatOutcome> {
        let mut out = CombatOutcome::default();
        self.combat.right_click_delay = self.combat.right_click_delay.saturating_sub(1);
        if screen_open {
            self.combat.miss_time = SCREEN_MISS_TICKS;
            self.stop_destroy()?;
            return Ok(out);
        }
        let mut instant = false;
        if self.is_using_item() {
            if !keys.use_down {
                self.release_use()?;
                out.released = true;
            }
        } else {
            for _ in 0..keys.attack_clicks {
                instant |= self.start_attack(hit, &mut out)?;
            }
            for _ in 0..keys.use_clicks {
                self.start_use_item(hit, &mut out)?;
            }
        }
        if keys.use_down && self.combat.right_click_delay == 0 && !self.is_using_item() {
            self.start_use_item(hit, &mut out)?;
        }
        self.continue_attack(!instant && keys.attack_down, hit, &mut out)?;
        self.combat.miss_time = (self.combat.miss_time - 1).max(0);
        Ok(out)
    }
}
