use std::time::{SystemTime, UNIX_EPOCH};

use mcr_buf::{BlockPos, FixedBitSet, LengthPrefixed};
use mcr_protocol::common as c;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::play::serverbound::{ContainerInput, Facing};
use mcr_protocol::types::game::Hand;
use mcr_protocol::types::{ItemStack, Vec3d, Vec3f};

use crate::client::Client;
use crate::error::Result;
use crate::inventory::PLAYER_MENU_ID;

#[derive(Debug, Clone, Copy)]
pub struct Digging {
    pub pos: BlockPos,
    pub face: Facing,
    pub started_tick: u64,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl Client {
    pub(crate) fn next_sequence(&mut self) -> i32 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    /// Unsigned, as vanilla sends without profile keys.
    pub fn send_chat(&mut self, message: &str) -> Result<()> {
        self.send_play(&S::Chat(sb::Chat {
            message: message.to_owned(),
            timestamp_ms: now_millis(),
            salt: 0,
            signature: None,
            last_seen: sb::LastSeenUpdate {
                offset: 0,
                acknowledged: FixedBitSet::new(),
                checksum: 0,
            },
        }))
    }

    /// `command` has no leading slash.
    pub fn send_command(&mut self, command: &str) -> Result<()> {
        self.send_play(&S::ChatCommand(sb::ChatCommand {
            command: command.trim_start_matches('/').to_owned(),
        }))
    }

    pub fn swing(&mut self) -> Result<()> {
        self.send_play(&S::Punch(sb::Punch))
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

    pub fn start_digging(&mut self, pos: BlockPos, face: Facing) -> Result<()> {
        let sequence = self.next_sequence();
        self.digging = Some(crate::actions::Digging {
            pos,
            face,
            started_tick: self.ticks,
        });
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::StartDestroyBlock,
            pos,
            face,
            sequence,
        }))
    }

    /// Creative break: the server removes the block on the start action, no digging state is kept.
    pub fn break_instantly(&mut self, pos: BlockPos, face: Facing) -> Result<()> {
        let sequence = self.next_sequence();
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::StartDestroyBlock,
            pos,
            face,
            sequence,
        }))
    }

    pub fn finish_digging(&mut self) -> Result<()> {
        let Some(d) = self.digging.take() else {
            return Ok(());
        };
        let sequence = self.next_sequence();
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::StopDestroyBlock,
            pos: d.pos,
            face: d.face,
            sequence,
        }))
    }

    pub fn abort_digging(&mut self) -> Result<()> {
        let Some(d) = self.digging.take() else {
            return Ok(());
        };
        let sequence = self.next_sequence();
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action: sb::PlayerActionKind::AbortDestroyBlock,
            pos: d.pos,
            face: d.face,
            sequence,
        }))
    }

    pub fn digging(&self) -> Option<Digging> {
        self.digging
    }

    /// Ticks to hold the break with an empty hand; 1 when instant (creative) and `None` if unbreakable.
    pub fn estimate_dig_ticks(&self, pos: BlockPos) -> Option<u32> {
        if self.state.player.instabuild {
            return Some(1);
        }
        let data = self.state.data;
        let state = self.state.chunks.block(pos.x, pos.y, pos.z)?;
        let block = data.block_state(u32::from(state.0))?.block;
        let hardness = data.block_info(block)?.destroy_time;
        if hardness < 0.0 {
            return None;
        }
        Some((hardness * 30.0).ceil() as u32 + 2)
    }

    fn simple_action(&mut self, action: sb::PlayerActionKind) -> Result<()> {
        self.send_play(&S::PlayerAction(sb::PlayerAction {
            action,
            pos: BlockPos::new(0, 0, 0),
            face: Facing::Down,
            sequence: 0,
        }))
    }

    pub fn drop_item(&mut self, whole_stack: bool) -> Result<()> {
        self.simple_action(if whole_stack {
            sb::PlayerActionKind::DropAllItems
        } else {
            sb::PlayerActionKind::DropItem
        })
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
        let data = self.state.data;
        let prediction =
            self.state
                .inventory
                .predict_click(data, container_id, slot, button, input);
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
        self.send_play(&S::SetCreativeModeSlot(sb::SetCreativeModeSlot {
            slot_num: slot,
            item,
        }))
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

    pub fn select_trade(&mut self, index: i32) -> Result<()> {
        self.send_play(&S::SelectTrade(sb::SelectTrade { item: index }))
    }

    pub fn rename_item(&mut self, name: &str) -> Result<()> {
        self.send_play(&S::RenameItem(sb::RenameItem {
            name: name.chars().take(50).collect(),
        }))
    }

    pub fn set_beacon(&mut self, primary: Option<i32>, secondary: Option<i32>) -> Result<()> {
        self.send_play(&S::SetBeacon(sb::SetBeacon {
            primary: primary.map(mcr_buf::VarInt),
            secondary: secondary.map(mcr_buf::VarInt),
        }))
    }

    pub fn request_command_suggestions(&mut self, id: i32, command: &str) -> Result<()> {
        self.send_play(&S::CommandSuggestion(sb::CommandSuggestion {
            id,
            command: command.chars().take(32500).collect(),
        }))
    }
}
