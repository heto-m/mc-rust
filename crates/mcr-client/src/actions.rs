use std::time::{SystemTime, UNIX_EPOCH};

use mcr_buf::{BlockPos, LengthPrefixed};
use mcr_protocol::common as c;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::play::serverbound::{ContainerInput, Facing};
use mcr_protocol::types::game::Hand;
use mcr_protocol::types::{ItemStack, UntrustedItemStack, Vec3d, Vec3f};

use crate::client::Client;
use crate::error::Result;
use crate::inventory::PLAYER_MENU_ID;
use crate::relay::LocalAction;
use crate::state::ClientState;

#[derive(Debug, Clone)]
pub struct Digging {
    pub pos: BlockPos,
    pub face: Facing,
    pub started_tick: u64,
    pub item: ItemStack,
    pub progress: f32,
    pub ticks: u32,
}

impl ClientState {
    /// `LocalPlayer.drop`: the held stack shrinks before the server answers. `None` for spectators,
    /// who never send the drop; otherwise whether anything left the hand.
    pub(crate) fn predict_drop(&mut self, whole_stack: bool) -> Option<bool> {
        use mcr_protocol::types::game::GameType;
        if self.player.game_mode == GameType::Spectator {
            return None;
        }
        let held = crate::inventory::HOTBAR_START + usize::from(self.player.held_slot);
        Some(match self.inventory.player.slots.get_mut(held) {
            Some(stack) if !stack.is_empty() => {
                if whole_stack || stack.count <= 1 {
                    *stack = ItemStack::default();
                } else {
                    stack.count -= 1;
                }
                true
            }
            _ => false,
        })
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The server skips the breaker when it broadcasts this event, so the client raises it for its own breaks.
const LEVEL_EVENT_DESTROY_BLOCK: i32 = 2001;

impl Client {
    pub(crate) fn next_sequence(&mut self) -> i32 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    /// Vanilla's `sendChat`: signed when a chat session is running, always with the last-seen update.
    pub fn send_chat(&mut self, message: &str) -> Result<()> {
        let packet = self.state.secure.chat(message, now_millis());
        self.send_play(&S::Chat(packet))
    }

    /// Vanilla's `sendCommand`; `command` has no leading slash. Commands with `minecraft:message`
    /// arguments go signed, everything else as a plain command.
    pub fn send_command(&mut self, command: &str) -> Result<()> {
        let command = command.trim_start_matches('/');
        let arguments = self.state.commands.signable_arguments(command);
        if arguments.is_empty() {
            return self.send_play(&S::ChatCommand(sb::ChatCommand {
                command: command.to_owned(),
            }));
        }
        let packet = self
            .state
            .secure
            .signed_command(command, &arguments, now_millis());
        self.send_play(&S::ChatCommandSigned(packet))
    }

    /// World Options' difficulty (0 peaceful to 3 hard); servers apply it only for gamemasters.
    pub fn change_difficulty(&mut self, id: u8) -> Result<()> {
        use mcr_protocol::types::game::Difficulty as D;
        let difficulty = [D::Peaceful, D::Easy, D::Normal, D::Hard][usize::from(id.min(3))];
        self.send_play(&S::ChangeDifficulty(sb::ChangeDifficulty { difficulty }))
    }

    /// The main hand's attack swing.
    pub fn swing(&mut self) -> Result<()> {
        self.local_action(LocalAction::Swing {
            hand: Hand::Main,
            attack: true,
        });
        self.send_play(&S::Punch(sb::Punch))
    }

    /// An attack or a miss reset the attack strength; only a mirror needs to hear it.
    pub fn reset_attack_strength(&mut self) {
        self.local_action(LocalAction::ResetAttackStrength);
    }

    /// Mining progress of the current dig, counted by the caller; only a mirror needs to hear it.
    pub fn dig_progress(&mut self, progress: f32) {
        if let Some(pos) = self.digging.as_ref().map(|d| d.pos) {
            self.local_action(LocalAction::Destroy {
                pos,
                progress: progress.clamp(0.0, 1.0),
            });
        }
    }

    pub fn use_item(&mut self, hand: Hand) -> Result<()> {
        let sequence = self.next_sequence();
        let (yaw, pitch) = self.view;
        self.send_play(&S::UseItem(sb::UseItem {
            hand,
            sequence,
            y_rot: yaw,
            x_rot: pitch,
        }))
    }

    pub fn use_item_on(
        &mut self,
        hand: Hand,
        pos: BlockPos,
        face: Facing,
        cursor: [f32; 3],
        inside: bool,
    ) -> Result<()> {
        let sequence = self.next_sequence();
        self.send_play(&S::UseItemOn(sb::UseItemOn {
            hand,
            hit: sb::BlockHit {
                pos,
                face,
                cursor: Vec3f {
                    x: cursor[0],
                    y: cursor[1],
                    z: cursor[2],
                },
                inside,
                world_border_hit: false,
            },
            sequence,
        }))
    }

    /// Places the held block against `against` on `face`; the new block appears at the neighbour.
    pub fn place_block(&mut self, against: BlockPos, face: Facing) -> Result<()> {
        self.use_item_on(Hand::Main, against, face, [0.5, 0.5, 0.5], false)
    }

    /// Vanilla `MultiPlayerGameMode#startDestroyBlock`; false when the action is restricted.
    pub fn start_destroy(&mut self, pos: BlockPos, face: Facing) -> Result<bool> {
        if !self.can_act_on_block(pos) {
            return Ok(false);
        }
        if self.state.player.instabuild {
            self.local_action(LocalAction::Destroy { pos, progress: 1.0 });
            let sequence = self.next_sequence();
            self.destroy_block_local(pos, sequence);
            self.destroy_delay = 5;
            self.send_play(&S::PlayerAction(sb::PlayerAction {
                action: sb::PlayerActionKind::StartDestroyBlock,
                pos,
                face,
                sequence,
            }))?;
            return Ok(true);
        }
        if self
            .digging
            .as_ref()
            .is_some_and(|d| d.pos == pos && d.item == *self.state.held_stack())
        {
            return Ok(true);
        }
        if let Some(old) = self.digging.take() {
            self.local_action(LocalAction::Destroy {
                pos: old.pos,
                progress: -1.0,
            });
            self.send_play(&S::PlayerAction(sb::PlayerAction {
                action: sb::PlayerActionKind::AbortDestroyBlock,
                pos: old.pos,
                face,
                sequence: 0,
            }))?;
        }
        let sequence = self.next_sequence();
        let progress = self.state.destroy_progress(pos);
        let instant = progress.is_some_and(|p| p >= 1.0);
        self.local_action(LocalAction::Destroy {
            pos,
            progress: if instant { 1.0 } else { 0.0 },
        });
        if instant {
            self.destroy_block_local(pos, sequence);
        } else {
            self.digging = Some(Digging {
                pos,
                face,
                started_tick: self.ticks,
                item: self.state.held_stack().clone(),
                progress: 0.0,
                ticks: 0,
            });
        }
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::StartDestroyBlock,
            pos,
            face,
            sequence,
        }))?;
        Ok(true)
    }

    /// Vanilla `continueDestroyBlock`, called every tick while the attack key is held on a block.
    pub fn continue_destroy(&mut self, pos: BlockPos, face: Facing) -> Result<bool> {
        if self.destroy_delay > 0 {
            self.destroy_delay -= 1;
            return Ok(true);
        }
        if self.state.player.instabuild && self.can_act_on_block(pos) {
            self.local_action(LocalAction::Destroy { pos, progress: 1.0 });
            self.destroy_delay = 5;
            let sequence = self.next_sequence();
            self.destroy_block_local(pos, sequence);
            self.send_play(&S::PlayerAction(sb::PlayerAction {
                action: sb::PlayerActionKind::StartDestroyBlock,
                pos,
                face,
                sequence,
            }))?;
            return Ok(true);
        }
        let same = self
            .digging
            .as_ref()
            .is_some_and(|d| d.pos == pos && d.item == *self.state.held_stack());
        if !same {
            return self.start_destroy(pos, face);
        }
        let Some(step) = self.state.destroy_progress(pos) else {
            self.digging = None;
            return Ok(false);
        };
        let Some(d) = self.digging.as_mut() else {
            return Ok(false);
        };
        d.progress += step;
        d.ticks += 1;
        let (progress, turned) = (d.progress, d.face != face);
        if turned {
            d.face = face;
        }
        self.local_action(LocalAction::Destroy {
            pos,
            progress: progress.clamp(0.0, 1.0),
        });
        if progress >= 1.0 {
            self.digging = None;
            let sequence = self.next_sequence();
            self.destroy_block_local(pos, sequence);
            self.destroy_delay = 5;
            self.send_play(&S::PlayerAction(sb::PlayerAction {
                action: sb::PlayerActionKind::StopDestroyBlock,
                pos,
                face,
                sequence,
            }))?;
        } else if turned {
            self.send_play(&S::PlayerAction(sb::PlayerAction {
                action: sb::PlayerActionKind::ChangeDestroyDirection,
                pos,
                face,
                sequence: 0,
            }))?;
        }
        Ok(true)
    }

    /// Attack key released: vanilla `stopDestroyBlock`.
    pub fn stop_destroy(&mut self) -> Result<()> {
        let Some(d) = self.digging.take() else {
            return Ok(());
        };
        self.local_action(LocalAction::Destroy {
            pos: d.pos,
            progress: -1.0,
        });
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::AbortDestroyBlock,
            pos: d.pos,
            face: Facing::Down,
            sequence: 0,
        }))
    }

    fn can_act_on_block(&self, pos: BlockPos) -> bool {
        use mcr_protocol::types::game::GameType;
        let p = &self.state.player;
        if matches!(p.game_mode, GameType::Adventure | GameType::Spectator) {
            return false;
        }
        let b = &self.state.world.border;
        let half = b.size / 2.0;
        let (x, z) = (f64::from(pos.x) + 0.5, f64::from(pos.z) + 0.5);
        (x - b.center_x).abs() <= half && (z - b.center_z).abs() <= half
    }

    fn destroy_block_local(&mut self, pos: BlockPos, sequence: i32) {
        let Some(old) = self.state.chunks.block(pos.x, pos.y, pos.z) else {
            return;
        };
        let held = self.state.held_stack();
        if self.state.player.instabuild
            && u32::try_from(held.item)
                .ok()
                .and_then(|i| self.state.data.item_tool(i))
                .is_some_and(|t| !t.can_destroy_blocks_in_creative)
        {
            return;
        }
        if self
            .state
            .data
            .block_state(u32::from(old.0))
            .is_none_or(|i| i.is_air())
        {
            return;
        }
        let after = self.state.state_after_break(old);
        self.state.predict_block(pos, after, sequence);
        self.push_event(crate::events::ClientEvent::LevelEvent {
            event: LEVEL_EVENT_DESTROY_BLOCK,
            pos,
            data: i32::from(old.0),
            global: false,
            predicted: true,
        });
    }

    /// Starts breaking; stays in the digging state unless the block breaks instantly.
    pub fn start_digging(&mut self, pos: BlockPos, face: Facing) -> Result<()> {
        self.start_destroy(pos, face).map(|_| ())
    }

    /// Creative break: the server removes the block on the start action, no digging state is kept.
    pub fn break_instantly(&mut self, pos: BlockPos, face: Facing) -> Result<()> {
        self.start_destroy(pos, face).map(|_| ())
    }

    /// Sends the finishing action for the current dig and predicts the broken block.
    pub fn finish_digging(&mut self) -> Result<()> {
        let Some(d) = self.digging.take() else {
            return Ok(());
        };
        self.local_action(LocalAction::Destroy {
            pos: d.pos,
            progress: 1.0,
        });
        let sequence = self.next_sequence();
        self.destroy_block_local(d.pos, sequence);
        self.destroy_delay = 5;
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::StopDestroyBlock,
            pos: d.pos,
            face: d.face,
            sequence,
        }))
    }

    pub fn abort_digging(&mut self) -> Result<()> {
        self.stop_destroy()
    }

    pub fn digging(&self) -> Option<&Digging> {
        self.digging.as_ref()
    }

    /// Ticks of holding the break after the start action until the block gives way; 1 when instant.
    pub fn estimate_dig_ticks(&self, pos: BlockPos) -> Option<u32> {
        if self.state.player.instabuild {
            return Some(1);
        }
        let p = self.state.destroy_progress(pos)?;
        if p <= 0.0 {
            return None;
        }
        Some(((1.0 / p).ceil() as u32).clamp(1, 1 << 20))
    }

    fn simple_action(&mut self, action: sb::PlayerActionKind) -> Result<()> {
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action,
            pos: BlockPos::new(0, 0, 0),
            face: Facing::Down,
            sequence: 0,
        }))
    }

    /// Vanilla `MultiPlayerGameMode.dropItem`: removes the predicted stack from the hand at once; true if anything left it.
    pub fn drop_item(&mut self, whole_stack: bool) -> Result<bool> {
        let Some(dropped) = self.state.predict_drop(whole_stack) else {
            return Ok(false);
        };
        if dropped {
            self.local_action(LocalAction::Drop { whole_stack });
        }
        self.simple_action(if whole_stack {
            sb::PlayerActionKind::DropAllItems
        } else {
            sb::PlayerActionKind::DropItem
        })?;
        Ok(dropped)
    }

    pub fn swap_offhand(&mut self) -> Result<()> {
        self.simple_action(sb::PlayerActionKind::SwapItemWithOffhand)
    }

    pub fn release_use_item(&mut self) -> Result<()> {
        self.simple_action(sb::PlayerActionKind::ReleaseUseItem)
    }

    pub fn pick_block(&mut self, pos: BlockPos, include_data: bool) -> Result<()> {
        self.send_play(&S::PickItemFromBlock(sb::PickItemFromBlock {
            pos,
            include_data,
        }))
    }

    pub fn attack(&mut self, entity_id: i32) -> Result<()> {
        self.send_play(&S::Attack(sb::Attack { entity_id }))
    }

    pub fn interact(&mut self, entity_id: i32, hand: Hand) -> Result<()> {
        self.send_play(&S::Interact(sb::Interact {
            entity_id,
            hand,
            location: Vec3d::default(),
            using_secondary_action: false,
        }))
    }

    pub fn select_hotbar(&mut self, slot: u8) -> Result<()> {
        if slot > 8 {
            return Ok(());
        }
        self.state.player.held_slot = slot;
        self.stop_using_local();
        self.send_play(&S::SetCarriedItem(sb::SetCarriedItem {
            slot: i16::from(slot),
        }))
    }

    pub fn open_inventory(&mut self) -> Result<()> {
        let entity_id = self.state.player.entity_id;
        self.send_play(&S::PlayerCommand(sb::PlayerCommand {
            entity_id,
            action: sb::PlayerCommandKind::OpenInventory,
            data: 0,
        }))
    }

    pub fn click_container(&mut self, slot: i16, button: i8, input: ContainerInput) -> Result<()> {
        let (container_id, state_id) = match &self.state.inventory.open {
            Some(c) => (c.id, c.state_id),
            None => (PLAYER_MENU_ID, self.state.inventory.player.state_id),
        };
        let creative = self.state.player.instabuild;
        let cx = crate::hashing::HashCx::new(self.state.data, &self.state.registries);
        let prediction =
            self.state
                .inventory
                .predict_click(&cx, creative, container_id, slot, button, input);
        self.send_play(&S::ContainerClick(sb::ContainerClick {
            container_id,
            state_id,
            slot_num: slot,
            button_num: button,
            input,
            changed_slots: LengthPrefixed(prediction.changed_slots),
            carried_item: prediction.carried,
        }))
    }

    pub fn close_container(&mut self) -> Result<()> {
        let id = self
            .state
            .inventory
            .open
            .take()
            .map_or(PLAYER_MENU_ID, |c| c.id);
        self.send_play(&S::ContainerClose(sb::ContainerClose { container_id: id }))
    }

    pub fn respawn(&mut self) -> Result<()> {
        self.send_play(&S::ClientCommand(sb::ClientCommand {
            action: sb::ClientCommandAction::PerformRespawn,
        }))
    }

    pub fn set_creative_slot(&mut self, slot: i16, item: ItemStack) -> Result<()> {
        if let Some(target) = usize::try_from(slot)
            .ok()
            .and_then(|i| self.state.inventory.player.slots.get_mut(i))
        {
            *target = item.clone();
        }
        self.send_creative_slot(slot, item)
    }

    fn send_creative_slot(&mut self, slot: i16, item: ItemStack) -> Result<()> {
        self.send_play(&S::SetCreativeModeSlot(sb::SetCreativeModeSlot {
            slot_num: slot,
            item: UntrustedItemStack(item),
        }))
    }

    /// Replaces the cursor stack locally; the server never learns about creative cursors.
    pub fn set_cursor(&mut self, stack: ItemStack) {
        self.state.inventory.cursor = stack;
    }

    pub fn creative_clear_inventory(&mut self) -> Result<()> {
        for slot in 1..=45 {
            self.set_creative_slot(slot, ItemStack::default())?;
        }
        Ok(())
    }

    /// Throws `item` out of a creative inventory the way vanilla's `handleCreativeModeItemDrop` does.
    pub fn creative_drop(&mut self, item: ItemStack) -> Result<()> {
        if item.is_empty() {
            return Ok(());
        }
        self.send_creative_slot(-1, item)
    }

    /// Runs a click on the player menu locally and tells the server about every slot it changed
    /// with creative slot packets, as vanilla's creative screens do instead of container clicks.
    pub fn creative_menu_click(
        &mut self,
        slot: i16,
        button: i8,
        input: ContainerInput,
    ) -> Result<()> {
        let before = self.state.inventory.player.slots.clone();
        let cx = crate::hashing::HashCx::new(self.state.data, &self.state.registries);
        self.state
            .inventory
            .predict_click(&cx, true, PLAYER_MENU_ID, slot, button, input);
        self.sync_creative_slots(&before)
    }

    /// Sends creative slot packets for the inventory slots (5..=45) that differ from `before`.
    pub fn sync_creative_slots(&mut self, before: &[ItemStack]) -> Result<()> {
        let changed: Vec<(i16, ItemStack)> = self
            .state
            .inventory
            .player
            .slots
            .iter()
            .enumerate()
            .filter(|(i, s)| (5..=45).contains(i) && before.get(*i) != Some(*s))
            .map(|(i, s)| (i as i16, s.clone()))
            .collect();
        for (slot, item) in changed {
            self.send_creative_slot(slot, item)?;
        }
        Ok(())
    }

    /// Updates the render distance the server streams and re-announces client information.
    pub fn set_view_distance(&mut self, distance: i8) -> Result<()> {
        self.config.view_distance = distance.clamp(2, 32);
        self.send_play(&S::ClientInformation(c::ClientInformation {
            language: self.config.language.clone(),
            view_distance: self.config.view_distance,
            chat_visibility: self.config.chat_visibility,
            chat_colors: true,
            model_customisation: 0x7f,
            main_hand: self.config.main_hand,
            text_filtering_enabled: false,
            allows_listing: true,
            particle_status: self.config.particle_status,
        }))
    }

    pub fn click_container_button(&mut self, button: i32) -> Result<()> {
        let container_id = self
            .state
            .inventory
            .open
            .as_ref()
            .map_or(PLAYER_MENU_ID, |c| c.id);
        self.send_play(&S::ContainerButtonClick(sb::ContainerButtonClick {
            container_id,
            button_id: button,
        }))
    }

    pub fn set_crafter_slot_state(&mut self, slot: i32, enabled: bool) -> Result<()> {
        let container_id = self
            .state
            .inventory
            .open
            .as_ref()
            .map_or(PLAYER_MENU_ID, |c| c.id);
        self.send_play(&S::ContainerSlotStateChanged(
            sb::ContainerSlotStateChanged {
                slot_id: slot,
                container_id,
                new_state: enabled,
            },
        ))
    }

    pub fn request_command_suggestions(&mut self, id: i32, command: &str) -> Result<()> {
        self.send_play(&S::CommandSuggestion(sb::CommandSuggestion {
            id,
            command: command.chars().take(32500).collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::types::game::GameType;
    use uuid::Uuid;

    use super::*;
    use crate::config::ClientConfig;
    use crate::inventory::HOTBAR_START;

    fn holding(count: i32) -> ClientState {
        let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
        s.player.held_slot = 2;
        let item = s.data.item_by_name("minecraft:cobblestone").unwrap() as i32;
        s.inventory.player.slots[HOTBAR_START + 2] = ItemStack::new(item, count);
        s
    }

    fn held(s: &ClientState) -> i32 {
        let stack = &s.inventory.player.slots[HOTBAR_START + 2];
        if stack.is_empty() { 0 } else { stack.count }
    }

    #[test]
    fn dropping_one_item_shrinks_the_held_stack() {
        let mut s = holding(5);
        assert_eq!(s.predict_drop(false), Some(true));
        assert_eq!(held(&s), 4);
        let mut last = holding(1);
        assert_eq!(last.predict_drop(false), Some(true));
        assert_eq!(held(&last), 0);
    }

    #[test]
    fn dropping_the_stack_empties_the_hand() {
        let mut s = holding(64);
        assert_eq!(s.predict_drop(true), Some(true));
        assert_eq!(held(&s), 0);
    }

    #[test]
    fn an_empty_hand_still_sends_but_drops_nothing() {
        let mut s = holding(1);
        s.inventory.player.slots[HOTBAR_START + 2] = ItemStack::default();
        assert_eq!(s.predict_drop(false), Some(false));
        assert_eq!(s.predict_drop(true), Some(false));
    }

    #[test]
    fn spectators_never_drop() {
        let mut s = holding(5);
        s.player.game_mode = GameType::Spectator;
        assert_eq!(s.predict_drop(true), None);
        assert_eq!(held(&s), 5);
    }
}
