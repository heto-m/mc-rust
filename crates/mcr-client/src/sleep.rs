//! Sleeping in a bed: the bed's pose for the camera and renderers, and getting up.

use mcr_buf::BlockPos;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::play::serverbound::Facing;

use crate::client::Client;
use crate::error::Result;
use crate::state::ClientState;

impl ClientState {
    /// Vanilla `AbstractBedBlock#getBedOrientation`: the facing of the bed block at `pos`.
    pub fn bed_orientation(&self, pos: BlockPos) -> Option<Facing> {
        let state = self.chunks.block(pos.x, pos.y, pos.z)?.0;
        let info = self.data.block_state(state)?;
        if !self.data.block_name(info.block)?.ends_with("_bed") {
            return None;
        }
        let facing = self
            .data
            .state_properties(state)
            .find_map(|(k, v)| (k == "facing").then_some(v))?;
        Some(match facing {
            "north" => Facing::North,
            "south" => Facing::South,
            "west" => Facing::West,
            "east" => Facing::East,
            _ => return None,
        })
    }

    /// `LivingEntity#setPosToBed`: the client lays a sleeper on its bed as soon as it learns the bed.
    pub(crate) fn place_on_bed(&mut self, bed: BlockPos) {
        if self.bed_orientation(bed).is_none() {
            return;
        }
        let Some(state) = self.chunks.block(bed.x, bed.y, bed.z) else {
            return;
        };
        let top = self
            .data
            .outline_shape(state.0)
            .iter()
            .map(|b| f64::from(b.max[1]))
            .fold(None, |m: Option<f64>, y| Some(m.map_or(y, |m| m.max(y))));
        let Some(top) = top else {
            return;
        };
        let p = &mut self.player.physics;
        p.pos = [
            f64::from(bed.x) + 0.5,
            f64::from(bed.y) + top + 0.125,
            f64::from(bed.z) + 0.5,
        ];
    }
}

/// Vanilla `Direction#toYRot`.
pub fn facing_yaw(facing: Facing) -> f32 {
    match facing {
        Facing::South => 0.0,
        Facing::West => 90.0,
        Facing::North => 180.0,
        Facing::East => 270.0,
        Facing::Down | Facing::Up => 0.0,
    }
}

impl Client {
    /// The in-bed screen's Leave Bed button: vanilla `InBedChatScreen#sendWakeUp`.
    pub fn stop_sleeping(&mut self) -> Result<()> {
        let entity_id = self.state.player.entity_id;
        self.send_play(&S::PlayerCommand(sb::PlayerCommand {
            entity_id,
            action: sb::PlayerCommandKind::StopSleeping,
            data: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use mcr_buf::BlockPos;
    use mcr_physics::PlayerInput;
    use mcr_protocol::dispatch::{PlayClientbound as P, PlayServerbound as S};
    use mcr_protocol::play::clientbound::entity::SetEntityData;
    use mcr_protocol::play::serverbound::{Facing, PlayerCommandKind};
    use mcr_protocol::types::entity_data::{EntityDataEntry, EntityDataValue, EntityDataValues};
    use mcr_world::{BlockState, ChunkColumn, ChunkPos};

    use crate::client::Client;
    use crate::state::Outbox;

    const BED: BlockPos = BlockPos { x: 1, y: 64, z: 1 };

    fn bed_state(c: &Client) -> u32 {
        let data = c.state.data;
        let bed = data.block_by_name("minecraft:red_bed").unwrap();
        let (first, end) = data.block_info(bed).unwrap().states;
        (first..end)
            .find(|&s| {
                let props: Vec<_> = data.state_properties(s).collect();
                props.contains(&("facing", "east")) && props.contains(&("part", "head"))
            })
            .unwrap()
    }

    fn in_bed() -> Client {
        let mut c = Client::for_test();
        let cfg = *c.state.chunks.config();
        c.state
            .chunks
            .insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
        let bed = bed_state(&c);
        c.state
            .chunks
            .set_block(BED.x, BED.y, BED.z, BlockState(bed));
        c.state.player.loaded = true;
        c.state.player.placed = true;
        c.state.player.physics.pos = [0.5, 65.0, 0.5];
        let packet = P::SetEntityData(SetEntityData {
            entity_id: c.state.player.entity_id,
            values: EntityDataValues {
                entries: vec![EntityDataEntry {
                    index: 14,
                    value: EntityDataValue::OptionalBlockPos(Some(BED)),
                }],
            },
        });
        c.state
            .handle_play(packet, &mut Outbox::default(), Instant::now())
            .unwrap();
        c
    }

    #[test]
    fn a_sleeper_lies_on_the_bed_and_cannot_move() {
        let mut c = in_bed();
        assert!(c.state.player.is_sleeping());
        assert_eq!(c.state.bed_orientation(BED), Some(Facing::East));
        assert_eq!(
            c.state.player.physics.pos,
            [1.5, 64.0 + 0.5625 + 0.125, 1.5]
        );
        let walk = PlayerInput {
            forward: true,
            jump: true,
            pitch: 40.0,
            ..PlayerInput::default()
        };
        for _ in 0..20 {
            c.player_tick(&walk).unwrap();
        }
        let p = &c.state.player.physics;
        assert_eq!((p.pos[0], p.pos[2]), (1.5, 1.5));
        assert_eq!(p.pose, mcr_physics::Pose::Sleeping);
        assert_eq!(p.pitch, 0.0);
    }

    #[test]
    fn leave_bed_sends_stop_sleeping() {
        let mut c = in_bed();
        c.take_sent();
        c.stop_sleeping().unwrap();
        let sent = c.take_sent();
        assert!(matches!(
            sent.as_slice(),
            [S::PlayerCommand(p)] if p.action == PlayerCommandKind::StopSleeping
        ));
    }

    #[test]
    fn the_wake_up_animation_ends_sleep_with_the_last_fade_ticks() {
        use mcr_protocol::play::clientbound::entity::Animate;
        let mut c = in_bed();
        c.state.player.sleep_timer = 40;
        let packet = P::Animate(Animate {
            entity_id: c.state.player.entity_id,
            action: 0,
        });
        c.state
            .handle_play(packet, &mut Outbox::default(), Instant::now())
            .unwrap();
        assert!(!c.state.player.is_sleeping());
        assert!(!c.state.player.physics.sleeping);
        assert_eq!(c.state.player.sleep_timer, 100);
    }

    #[test]
    fn the_sleep_fade_rises_to_100_and_fades_out_after_waking() {
        let mut c = in_bed();
        for _ in 0..150 {
            c.state.player.tick_sleep();
        }
        assert_eq!(c.state.player.sleep_timer, 100);
        c.state.player.data.remove(&14);
        c.state.player.tick_sleep();
        assert_eq!(c.state.player.sleep_timer, 101);
        for _ in 0..9 {
            c.state.player.tick_sleep();
        }
        assert_eq!(c.state.player.sleep_timer, 0);
    }
}
