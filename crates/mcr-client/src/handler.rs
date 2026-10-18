use std::time::Instant;

use mcr_buf::{BlockPos, IdOr};
use mcr_physics::Abilities;
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::clientbound::entity::{
    EntityPositionSync, MoveEntityPos, MoveEntityPosRot, MoveEntityRot, Relatives, TeleportEntity,
};
use mcr_protocol::play::clientbound::player::{
    BoundChatType, CommonPlayerSpawnInfo, EntityAnchor, PlayerInfoAction, PlayerLookAt,
};
use mcr_protocol::play::clientbound::ui::{BossEventOperation, ObjectiveAction};
use mcr_protocol::play::clientbound::world::SoundSource;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::types::Component;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::AttributeModifier;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::GameType;
use mcr_protocol::types::registry::KnownPack;
use mcr_protocol::types::text::Style;
use mcr_world::{ChunkColumn, ChunkData, ChunkPos, LightData};
use uuid::Uuid;

use crate::entity::{AttributeState, Entity, MAX_ENTITIES, MobEffect, end_of_delta, end_of_path};
use crate::error::Result;
use crate::events::{ChatKind, ChatTag, ClientEvent, DamageEffects};
use crate::inventory::Inventory;
use crate::registry::{CHAT_TYPE, Dimension};
use crate::social::{BossBar, ChatMessage, Score};
use crate::state::{
    ClientState, MAX_BLOCK_ENTITIES, MAX_COOKIE_BYTES, MAX_COOKIES, Outbox, SpawnPoint,
    resource_pack_responses,
};
use crate::toast::ToastEvent;

pub const KNOWN_PACK_NAMESPACE: &str = "minecraft";
pub const KNOWN_PACK_ID: &str = "core";
pub const KNOWN_PACK_VERSION: &str = "26.3";

pub fn core_pack() -> KnownPack {
    KnownPack {
        namespace: KNOWN_PACK_NAMESPACE.into(),
        id: KNOWN_PACK_ID.into(),
        version: KNOWN_PACK_VERSION.into(),
    }
}

fn v3(v: &mcr_protocol::types::Vec3d) -> [f64; 3] {
    [v.x, v.y, v.z]
}

fn sane(v: f64) -> bool {
    v.is_finite() && v.abs() < 3.0e8
}

fn sane3(v: [f64; 3]) -> bool {
    v.iter().copied().all(sane)
}

const MAX_SERVER_ICON_BYTES: usize = 64 * 1024;
/// `ClientboundAnimatePacket.WAKE_UP`.
const ANIMATE_WAKE_UP: u8 = 0;

impl ClientState {
    pub fn handle_play(&mut self, packet: P<'_>, out: &mut Outbox, now: Instant) -> Result<()> {
        match packet {
            P::Login(p) => self.on_login(&p, out)?,
            P::Respawn(p) => self.on_respawn(&p.spawn_info, p.data_to_keep, out)?,
            P::KeepAlive(p) => {
                out.replies
                    .push(S::KeepAlive(mcr_protocol::common::ServerboundKeepAlive {
                        id: p.id,
                    }))
            }
            P::Ping(p) => out
                .replies
                .push(S::Pong(mcr_protocol::common::Pong { id: p.id })),
            P::CookieRequest(p) => {
                let payload = self.server.cookies.get(p.key.as_str()).cloned();
                out.cookie_responses.push((p.key, payload));
            }
            P::StoreCookie(p) => {
                let key = p.key.as_str().to_owned();
                let fits = p.payload.0.len() <= MAX_COOKIE_BYTES
                    && (self.server.cookies.contains_key(&key)
                        || self.server.cookies.len() < MAX_COOKIES);
                if fits {
                    self.server.cookies.insert(key, p.payload.0.to_vec());
                }
            }
            P::CustomPayload(p) => {
                if p.channel.as_str() == "minecraft:brand" {
                    let mut input = p.data.0;
                    if let Ok(brand) = mcr_buf::decode_str(&mut input, 128) {
                        self.server.brand = Some(brand.to_owned());
                    }
                }
            }
            P::ResourcePackPush(p) => {
                for response in resource_pack_responses(self.resource_packs, p.id) {
                    out.replies.push(S::ResourcePack(response));
                }
            }
            P::ServerLinks(p) => self.server.links = crate::state::trusted_links(p.links),
            P::CustomReportDetails(p) => {
                self.server.report_details = crate::state::bounded_report_details(p.details.0)
            }
            P::LowDiskSpaceWarning(_) => out.event(ClientEvent::Toast(ToastEvent::LowDiskSpace)),
            P::ServerData(p) => {
                self.server.motd = Some(p.motd);
                if let Some(icon) = p.icon.filter(|i| i.len() <= MAX_SERVER_ICON_BYTES) {
                    self.server.icon = Some(icon);
                }
            }
            P::GameRuleValues(p) => {
                for (k, v) in p.values.0.into_iter().take(crate::state::MAX_GAME_RULES) {
                    self.server.set_game_rule(k.as_str(), v);
                }
            }
            P::UpdateTags(p) => self.registries.apply_tags(&p),
            P::UpdateRecipes(p) => self.registries.apply_recipes(&p),
            P::StartConfiguration(_) => {
                self.registries.clear();
                if let Some(ack) = self.secure.take_ack() {
                    out.replies.push(S::ChatAck(ack));
                }
                out.replies
                    .push(S::ConfigurationAcknowledged(sb::ConfigurationAcknowledged));
                out.event(ClientEvent::Reconfiguring);
            }
            P::Transfer(p) => out.event(ClientEvent::Transfer {
                host: p.host,
                port: u16::try_from(p.port).unwrap_or(25565),
            }),
            P::Disconnect(p) => out.event(ClientEvent::Disconnected(
                crate::events::DisconnectReason::Server(p.reason),
            )),

            P::LevelChunkWithLight(p) => self.on_chunk(p.x, p.z, p.payload.0, out),
            P::LightUpdate(p) => self.on_light(p.x, p.z, p.payload.0, out),
            P::ForgetLevelChunk(p) => {
                let pos = ChunkPos::new(p.pos.x, p.pos.z);
                self.unload_chunk(pos, out);
            }
            P::ChunksBiomes(p) => {
                for chunk in &p.chunks {
                    let pos = ChunkPos::new(chunk.pos.x, chunk.pos.z);
                    if matches!(self.chunks.apply_biomes(pos, chunk.buffer.0), Ok(true)) {
                        out.event(ClientEvent::LightUpdated(pos));
                    }
                }
            }
            P::ChunkBatchStart(_) => {
                self.in_batch = true;
                self.chunk_batches.on_batch_start(now);
            }
            P::ChunkBatchFinished(p) => {
                self.in_batch = false;
                self.chunk_batches.on_batch_finished(p.batch_size, now);
                out.replies
                    .push(S::ChunkBatchReceived(sb::ChunkBatchReceived {
                        desired_chunks_per_tick: self.chunk_batches.desired_chunks_per_tick(),
                    }));
            }
            P::SetChunkCacheCenter(p) => {
                self.world.chunk_center = (p.x, p.z);
                self.unload_out_of_range(out);
            }
            P::SetChunkCacheRadius(p) => {
                self.world.view_distance = p.radius.clamp(0, 64);
                self.unload_out_of_range(out);
            }
            P::SetSimulationDistance(p) => {
                self.world.simulation_distance = p.simulation_distance.clamp(0, 64)
            }
            P::AddTransientBlock(p) => {
                let pos = [p.pos.x, p.pos.y, p.pos.z];
                self.transient_blocks.retain(|b| b.pos != pos);
                if self.transient_blocks.len() < crate::state::MAX_TRANSIENT_BLOCKS {
                    self.transient_blocks.push(crate::state::TransientBlock {
                        pos,
                        state: p.block_state,
                        ticks: crate::state::TRANSIENT_BLOCK_TICKS,
                    });
                }
            }
            P::BlockUpdate(p) => self.set_block(p.pos, p.block_state.max(0) as u32, out),
            P::SectionBlocksUpdate(p) => {
                for change in &p.changes {
                    let pos = BlockPos::new(
                        p.section.x.wrapping_mul(16) + i32::from(change.x & 15),
                        p.section.y.wrapping_mul(16) + i32::from(change.y & 15),
                        p.section.z.wrapping_mul(16) + i32::from(change.z & 15),
                    );
                    self.set_block(pos, change.block_state, out);
                }
            }
            P::BlockEntityData(p) => {
                let loaded = self.chunks.block(p.pos.x, p.pos.y, p.pos.z).is_some();
                if loaded
                    && (self.block_entities.contains_key(&p.pos)
                        || self.block_entities.len() < MAX_BLOCK_ENTITIES)
                {
                    self.block_entities
                        .insert(p.pos, (p.block_entity_type, p.tag.0));
                    out.event(ClientEvent::BlockEntityChanged(p.pos));
                }
            }
            P::BlockEvent(p) => out.event(ClientEvent::BlockEvent {
                pos: p.pos,
                b0: p.b0,
                b1: p.b1,
                block: p.block,
            }),
            P::BlockDestruction(p) => out.event(ClientEvent::BlockDestruction {
                entity_id: p.entity_id,
                pos: p.pos,
                progress: p.progress,
            }),
            P::BlockChangedAck(p) => {
                self.block_ack_sequence = self.block_ack_sequence.max(p.sequence);
                for (pos, state) in self.finish_predictions(p.sequence) {
                    out.event(ClientEvent::BlockChanged { pos, state });
                }
                out.event(ClientEvent::BlockAckReceived(p.sequence));
            }
            P::LevelEvent(p) => out.event(ClientEvent::LevelEvent {
                event: p.event,
                pos: p.pos,
                data: p.data,
                global: p.global,
                predicted: false,
            }),
            P::LevelParticles(p) => out.event(ClientEvent::Particle {
                particle: p.particle,
                pos: [p.x, p.y, p.z],
                count: p.count,
                delta: [p.x_dist, p.y_dist, p.z_dist],
                speed: [p.x_max_speed, p.y_max_speed, p.z_max_speed],
                always_show: p.always_show,
            }),
            P::Sound(p) => out.event(ClientEvent::Sound {
                sound: p.sound,
                source: p.source,
                pos: Some([
                    f64::from(p.x) / 8.0,
                    f64::from(p.y) / 8.0,
                    f64::from(p.z) / 8.0,
                ]),
                entity_id: None,
                volume: p.volume,
                pitch: p.pitch,
            }),
            P::SoundEntity(p) => out.event(ClientEvent::Sound {
                sound: p.sound,
                source: p.source,
                pos: None,
                entity_id: Some(p.entity_id),
                volume: p.volume,
                pitch: p.pitch,
            }),
            P::StopSound(p) => out.event(ClientEvent::StopSound {
                source: p.source,
                name: p.name,
            }),
            P::Explode(p) => {
                if let Some(k) = p.player_knockback.as_ref().filter(|k| sane3(v3(k))) {
                    let vel = &mut self.player.physics.vel;
                    vel[0] += k.x;
                    vel[1] += k.y;
                    vel[2] += k.z;
                }
                out.event(ClientEvent::Explosion {
                    center: v3(&p.center),
                    radius: p.radius,
                    particle: p.particle,
                    sound: p.play_sound.then_some(p.sound),
                    block_count: p.block_count.max(0),
                    block_particles: p
                        .block_particles
                        .into_iter()
                        .take(crate::events::MAX_EXPLOSION_PARTICLE_KINDS)
                        .collect(),
                });
            }
            P::SetTime(p) => {
                self.world.game_time = p.game_time;
                for update in p.clock_updates.iter().take(crate::state::MAX_CLOCKS) {
                    let ticks = update.total_ticks as f64 + f64::from(update.partial_tick);
                    self.world.set_clock(
                        update.clock,
                        crate::state::ClockState {
                            ticks,
                            rate: update.rate,
                        },
                    );
                }
                out.event(ClientEvent::TimeChanged);
            }
            P::GameEvent(p) => self.on_game_event(p.event, p.param, out),
            P::ChangeDifficulty(p) => {
                self.world.difficulty = p.difficulty as u8;
                self.world.difficulty_locked = p.locked;
            }
            P::SetDefaultSpawnPosition(p) => {
                self.world.spawn = Some(SpawnPoint {
                    dimension: p.dimension,
                    pos: p.pos,
                    yaw: p.yaw,
                    pitch: p.pitch,
                });
            }
            P::InitializeBorder(p) => {
                let b = &mut self.world.border;
                b.set_center(p.new_center_x, p.new_center_z);
                b.absolute_max_size = p.new_absolute_max_size;
                b.warning_blocks = p.warning_blocks;
                b.warning_time = p.warning_time;
                b.set_lerp(p.old_size, p.new_size, p.lerp_time);
            }
            P::SetBorderCenter(p) => self.world.border.set_center(p.new_center_x, p.new_center_z),
            P::SetBorderSize(p) => self.world.border.set_lerp(p.size, p.size, 0),
            P::SetBorderLerpSize(p) => {
                self.world
                    .border
                    .set_lerp(p.old_size, p.new_size, p.lerp_time)
            }
            P::SetBorderWarningDelay(p) => self.world.border.warning_time = p.warning_delay,
            P::SetBorderWarningDistance(p) => self.world.border.warning_blocks = p.warning_blocks,
            P::TickingState(p) => {
                if p.tick_rate.is_finite() {
                    self.world.tick_rate = p.tick_rate.clamp(1.0, 10_000.0);
                }
                self.world.frozen = p.is_frozen;
            }
            P::TickingStep(p) => self.world.frozen_ticks_to_run = p.tick_steps.max(0),

            P::AddEntity(p) => {
                let mut pos = [p.x, p.y, p.z];
                if Some(p.entity_type) == self.sim_kinds.knot_type {
                    // The packet carries the fence's block position; the knot sits on the post.
                    pos = [
                        pos[0].floor() + 0.5,
                        pos[1].floor() + 0.375,
                        pos[2].floor() + 0.5,
                    ];
                }
                if sane3(pos) && self.entities.len() < MAX_ENTITIES {
                    let mut e = Entity::new(p.entity_id, p.uuid, p.entity_type, pos);
                    e.yaw = p.y_rot.degrees();
                    e.pitch = p.x_rot.degrees();
                    e.target_yaw = e.yaw;
                    e.target_pitch = e.pitch;
                    e.snap_head(p.y_head_rot.degrees());
                    e.velocity = clamp_motion(v3(&p.velocity.0));
                    e.object_data = p.data;
                    e.sim = self.sim_kinds.get(p.entity_type);
                    if self.entities.insert(e) {
                        out.event(ClientEvent::EntityAdded(p.entity_id));
                    }
                }
            }
            P::PostEffects(p) => {
                self.post_effects = p
                    .post_effects
                    .iter()
                    .take(crate::state::MAX_POST_EFFECTS)
                    .map(|id| id.as_str().to_owned())
                    .collect();
            }
            P::SetCamera(p) => {
                self.camera_entity = (p.camera_id != self.player.entity_id
                    && self.entities.get(p.camera_id).is_some())
                .then_some(p.camera_id);
            }
            P::RemoveEntities(p) => {
                for id in p.entity_ids.iter().take(1 << 16) {
                    if self.entities.remove(*id).is_some() {
                        out.event(ClientEvent::EntityRemoved(*id));
                    }
                    if self.camera_entity == Some(*id) {
                        self.camera_entity = None;
                    }
                }
            }
            P::MoveEntityPos(MoveEntityPos {
                entity_id,
                delta,
                on_ground,
            }) => self.move_entity(entity_id, Some(&delta), None, on_ground, out),
            P::MoveEntityPosRot(MoveEntityPosRot {
                entity_id,
                delta,
                y_rot,
                x_rot,
                on_ground,
            }) => self.move_entity(
                entity_id,
                Some(&delta),
                Some((y_rot.degrees(), x_rot.degrees())),
                on_ground,
                out,
            ),
            P::MoveEntityRot(MoveEntityRot {
                entity_id,
                on_ground,
                y_rot,
                x_rot,
            }) => self.move_entity(
                entity_id,
                None,
                Some((y_rot.degrees(), x_rot.degrees())),
                on_ground,
                out,
            ),
            P::MoveVehicle(p) => {
                if let Some(id) = self.controlled_vehicle
                    && let Some(e) = self.entities.get_mut(id)
                {
                    let pos = [p.position.x, p.position.y, p.position.z];
                    if sane3(pos) && p.y_rot.is_finite() && p.x_rot.is_finite() {
                        e.snap_to(pos, p.y_rot, p.x_rot.clamp(-90.0, 90.0));
                        self.vehicle_corrections += 1;
                        out.replies
                            .push(mcr_protocol::dispatch::PlayServerbound::MoveVehicle(
                                mcr_protocol::play::serverbound::MoveVehicle {
                                    position: p.position,
                                    y_rot: p.y_rot,
                                    x_rot: p.x_rot,
                                    on_ground: e.on_ground,
                                },
                            ));
                    }
                }
            }
            P::TeleportEntity(p) => self.teleport_entity(&p, out),
            P::EntityPositionSync(p) => self.position_sync(&p, out),
            P::RotateHead(p) => {
                if let Some(e) = self.entities.get_mut(p.entity_id) {
                    e.lerp_head_to(p.y_head_rot.degrees());
                }
            }
            P::SetEntityMotion(p) => {
                let vel = v3(&p.movement.0);
                if sane3(vel) {
                    if p.entity_id == self.player.entity_id {
                        self.player.physics.vel = vel;
                    } else if let Some(e) = self.entities.get_mut(p.entity_id) {
                        e.velocity = clamp_motion(vel);
                    }
                }
            }
            P::SetEntityData(p) if p.entity_id == self.player.entity_id => {
                let mut bed = None;
                for entry in p.values.entries.into_iter().take(64) {
                    if entry.index == crate::player::SLEEPING_POS_INDEX
                        && let EntityDataValue::OptionalBlockPos(Some(pos)) = entry.value
                    {
                        bed = Some(pos);
                    }
                    self.player.data.insert(entry.index, entry.value);
                }
                crate::glide::apply_player_flags(&mut self.player);
                if let Some(bed) = bed {
                    self.place_on_bed(bed);
                }
            }
            P::SetEntityData(p) => {
                if let Some(e) = self.entities.get_mut(p.entity_id) {
                    for entry in p.values.entries {
                        e.data.insert(entry.index, entry.value);
                    }
                    out.event(ClientEvent::EntityDataChanged(p.entity_id));
                }
            }
            P::SetEquipment(p) => {
                if let Some(e) = self.entities.get_mut(p.entity_id) {
                    for entry in p.slots {
                        e.set_equipment(entry.slot, entry.item);
                    }
                    out.event(ClientEvent::EntityEquipmentChanged(p.entity_id));
                }
            }
            P::SetPassengers(p) => self.on_set_passengers(p.vehicle, &p.passengers, out),
            P::SetEntityLink(p) => {
                if let Some(e) = self.entities.get_mut(p.source_id) {
                    e.leash_holder = (p.dest_id > 0).then_some(p.dest_id);
                }
            }
            P::UpdateMobEffect(p) => {
                let effect = MobEffect {
                    amplifier: p.amplifier,
                    duration_ticks: p.duration_ticks,
                    flags: p.flags,
                };
                if p.entity_id == self.player.entity_id {
                    if self.player.effects.len() < 64 || self.player.effects.contains_key(&p.effect)
                    {
                        self.player.effects.insert(p.effect, effect);
                    }
                    self.refresh_physics_inputs();
                } else if let Some(e) = self.entities.get_mut(p.entity_id) {
                    e.upsert_effect(p.effect, effect);
                }
            }
            P::RemoveMobEffect(p) => {
                if p.entity_id == self.player.entity_id {
                    self.player.effects.remove(&p.effect);
                    self.refresh_physics_inputs();
                } else if let Some(e) = self.entities.get_mut(p.entity_id) {
                    e.effects.remove(&p.effect);
                }
            }
            P::UpdateAttributes(p) => {
                let names = self.data.registry("minecraft:attribute");
                let apply = |attrs: &mut dyn FnMut(i32, AttributeState)| {
                    for a in p.attributes.0.iter() {
                        let range = names
                            .and_then(|r| r.name_of(u32::try_from(a.attribute).ok()?))
                            .and_then(crate::entity::attribute_range);
                        attrs(
                            a.attribute,
                            AttributeState {
                                base: a.base,
                                modifiers: a
                                    .modifiers
                                    .iter()
                                    .take(64)
                                    .cloned()
                                    .collect::<Vec<AttributeModifier>>(),
                                range,
                            },
                        );
                    }
                };
                if p.entity_id == self.player.entity_id {
                    let map = &mut self.player.attributes;
                    apply(&mut |id, state| {
                        if map.len() < 128 || map.contains_key(&id) {
                            map.insert(id, state);
                        }
                    });
                    self.refresh_physics_inputs();
                } else if let Some(e) = self.entities.get_mut(p.entity_id) {
                    apply(&mut |id, state| e.upsert_attribute(id, state));
                }
            }
            P::EntityEvent(p) => {
                if p.entity_id == self.player.entity_id && (24..=28).contains(&p.event_id) {
                    self.player.permission_level = (p.event_id - 24) as u8;
                }
                self.apply_entity_event(p.entity_id, p.event_id);
                out.event(ClientEvent::EntityEvent {
                    entity_id: p.entity_id,
                    event: p.event_id,
                });
            }
            P::Animate(p) => {
                if p.entity_id == self.player.entity_id && p.action == ANIMATE_WAKE_UP {
                    self.player.wake_up();
                }
                out.event(ClientEvent::EntityAnimation {
                    entity_id: p.entity_id,
                    action: p.action,
                })
            }
            P::SwingAnimation(p) => out.event(ClientEvent::EntitySwing {
                entity_id: p.entity_id,
                offhand: p.hand == mcr_protocol::types::game::Hand::Off,
            }),
            P::HurtAnimation(p) => {
                if p.entity_id == self.player.entity_id && p.yaw.is_finite() {
                    self.player.start_hurt();
                    self.player.hurt_dir = p.yaw;
                }
                out.event(ClientEvent::EntityHurt {
                    entity_id: p.entity_id,
                });
            }
            P::DamageEvent(p) => {
                let effects = self.damage_effects(p.source_type);
                if p.entity_id == self.player.entity_id {
                    self.player.start_hurt();
                    self.player.damage_cooldown = 20;
                }
                out.event(ClientEvent::EntityDamaged {
                    entity_id: p.entity_id,
                    source_type: p.source_type,
                    effects,
                });
            }
            P::TakeItemEntity(p) => self.take_item_entity(&p, out),

            P::PlayerPosition(p) => self.on_player_position(&p, out),
            P::PlayerRotation(p) => {
                let phys = &mut self.player.physics;
                let yaw = if p.relative_y {
                    phys.yaw + p.y_rot
                } else {
                    p.y_rot
                };
                let pitch = if p.relative_x {
                    phys.pitch + p.x_rot
                } else {
                    p.x_rot
                };
                if yaw.is_finite() && pitch.is_finite() {
                    let (pos, vel) = (phys.pos, phys.vel);
                    phys.snap_to(pos, vel, yaw % 360.0, pitch.clamp(-90.0, 90.0));
                    let (y_rot, x_rot) = (phys.yaw, phys.pitch);
                    self.player.view_snapped = true;
                    out.replies.push(S::MovePlayerRot(sb::MovePlayerRot {
                        y_rot,
                        x_rot,
                        flags: sb::MoveFlags::new(false, false),
                    }));
                    out.event(ClientEvent::PlayerRotated);
                }
            }
            P::PlayerLookAt(p) => self.on_look_at(&p, out),
            P::PlayerAbilities(p) => {
                let abilities = Abilities {
                    flying: p.flying,
                    may_fly: p.can_fly,
                    flying_speed: p.flying_speed,
                };
                self.player.walking_speed = p.walking_speed;
                self.player
                    .set_abilities(abilities, p.instabuild, p.invulnerable);
                self.refresh_physics_inputs();
                out.event(ClientEvent::AbilitiesChanged);
            }
            P::SetHealth(p) => {
                self.player
                    .hurt_to(if p.health.is_finite() { p.health } else { 0.0 });
                self.player.set_food(p.food);
                self.player.saturation = p.saturation;
                out.event(ClientEvent::HealthChanged);
                if self.player.health <= 0.0 && !self.player.dead {
                    self.player.dead = true;
                    out.event(ClientEvent::Died { message: None });
                } else if self.player.health > 0.0 {
                    self.player.dead = false;
                }
            }
            P::SetExperience(p) => {
                self.player.set_experience(p.progress, p.total, p.level);
                out.event(ClientEvent::ExperienceChanged);
            }
            P::SetHeldSlot(p) => {
                if (0..=8).contains(&p.slot) {
                    self.player.held_slot = p.slot as u8;
                    out.event(ClientEvent::HeldSlotChanged(p.slot as u8));
                }
            }
            P::PlayerCombatKill(p) => {
                if p.player_id == self.player.entity_id {
                    self.player.dead = true;
                    if !self.player.show_death_screen {
                        out.replies.push(S::ClientCommand(sb::ClientCommand {
                            action: sb::ClientCommandAction::PerformRespawn,
                        }));
                    }
                    out.event(ClientEvent::Died {
                        message: Some(p.message),
                    });
                }
            }

            P::OpenScreen(p) => {
                self.inventory
                    .open_container(p.container_id, p.menu_type, p.title.clone());
                if let Some(open) = self.inventory.open.as_mut() {
                    open.player_start =
                        crate::menu::Layout::menu_player_start(self.data, p.menu_type);
                }
                out.event(ClientEvent::ContainerOpened {
                    id: p.container_id,
                    menu_type: p.menu_type,
                    title: p.title,
                });
            }
            P::MountScreenOpen(p) => {
                self.inventory
                    .open_container(p.container_id, -1, Component::text(""));
                let entity_type = self.entities.get(p.entity_id).map(|e| e.kind);
                if let (Some(open), Some(entity_type)) = (self.inventory.open.as_mut(), entity_type)
                {
                    open.mount = Some(crate::menu::MountInfo {
                        columns: usize::try_from(p.inventory_columns).unwrap_or(0).min(16),
                        entity_type,
                    });
                }
                out.event(ClientEvent::ContainerOpened {
                    id: p.container_id,
                    menu_type: -1,
                    title: Component::text(""),
                });
            }
            P::ContainerClose(p) => {
                self.inventory.close(p.container_id);
                out.event(ClientEvent::ContainerClosed(p.container_id));
            }
            P::ContainerSetContent(p) => {
                if self
                    .inventory
                    .set_content(p.container_id, p.state_id, p.items, p.carried_item)
                {
                    out.event(ClientEvent::ContainerUpdated(p.container_id));
                    out.event(ClientEvent::InventoryChanged);
                }
            }
            P::ContainerSetSlot(p) => {
                if self
                    .inventory
                    .set_slot(p.container_id, p.state_id, p.slot, p.item)
                {
                    out.event(ClientEvent::ContainerUpdated(p.container_id));
                    out.event(ClientEvent::InventoryChanged);
                }
            }
            P::ContainerSetData(p) => {
                if let Some(c) = self
                    .inventory
                    .open
                    .as_mut()
                    .filter(|c| c.id == p.container_id)
                    && (c.data.len() < 256 || c.data.contains_key(&p.id))
                {
                    c.data.insert(p.id, p.value);
                    out.event(ClientEvent::ContainerUpdated(p.container_id));
                }
            }
            P::SetCursorItem(p) => self.inventory.cursor = p.contents,
            P::SetPlayerInventory(p) => {
                if self.inventory.set_player_inventory(p.slot, p.contents) {
                    out.event(ClientEvent::InventoryChanged);
                }
            }
            P::Cooldown(p) => {
                let ticks = p.duration.clamp(0, 1 << 20);
                let key = p.cooldown_group.as_str().to_owned();
                if ticks == 0 {
                    self.cooldowns.remove(&key);
                } else {
                    crate::gamestate::insert_bounded(
                        &mut self.cooldowns,
                        1024,
                        key,
                        crate::state::Cooldown {
                            ends_at: self.game_ticks + ticks as u64,
                            total: ticks,
                        },
                    );
                }
            }

            P::SystemChat(p) if p.overlay => self.set_overlay(p.content, out),
            P::SystemChat(p) => self.push_chat(ChatKind::System, None, p.content, None, None, out),
            P::PlayerChat(p) => self.on_player_chat(p, out),
            P::DisguisedChat(p) => {
                let shown = self.format_chat(&p.chat_type, p.message);
                self.push_chat(ChatKind::Disguised, None, shown, None, None, out);
            }
            P::CustomChatCompletions(p) => self.apply_chat_completions(p.action, p.entries),
            P::DeleteChat(p) => self.on_delete_chat(&p.message_signature, out),
            P::PlayerInfoUpdate(p) => {
                self.players.apply(&p);
                if p.actions.get(PlayerInfoAction::InitializeChat.bit()) {
                    for entry in &p.entries {
                        if self.players.get(entry.uuid).is_some() {
                            self.secure
                                .init_session(entry.uuid, entry.chat_session.as_ref());
                        }
                    }
                }
                out.event(ClientEvent::PlayerListChanged);
            }
            P::PlayerInfoRemove(p) => {
                self.players.remove(&p.profile_ids);
                self.secure.remove_players(&p.profile_ids);
                out.event(ClientEvent::PlayerListChanged);
            }
            P::TabList(p) => {
                self.tab_list.header = p.header;
                self.tab_list.footer = p.footer;
                out.event(ClientEvent::TabListChanged);
            }
            P::SetTitleText(p) => {
                self.titles.set_title(p.text.clone());
                out.event(ClientEvent::Title {
                    title: Some(p.text),
                    subtitle: None,
                });
            }
            P::SetSubtitleText(p) => {
                self.titles.set_subtitle(p.text.clone());
                out.event(ClientEvent::Title {
                    title: None,
                    subtitle: Some(p.text),
                });
            }
            P::SetActionBarText(p) => self.set_overlay(p.text, out),
            P::SetTitlesAnimation(p) => self.titles.set_times(p.fade_in, p.stay, p.fade_out),
            P::ClearTitles(p) => {
                self.titles.clear(p.reset_times);
                out.event(ClientEvent::TitlesCleared);
            }
            P::BossEvent(p) => self.on_boss_event(p.id, p.operation, now, out),
            P::SetObjective(p) => {
                match &p.action {
                    ObjectiveAction::Add(d) => {
                        self.scoreboard.set_objective(&p.objective_name, Some(d));
                    }
                    ObjectiveAction::Change(d) => {
                        self.scoreboard.change_objective(&p.objective_name, d);
                    }
                    ObjectiveAction::Remove => {
                        self.scoreboard.set_objective(&p.objective_name, None)
                    }
                }
                out.event(ClientEvent::ScoreboardChanged);
            }
            P::SetScore(p) => {
                self.scoreboard.set_score(
                    &p.owner,
                    &p.objective_name,
                    Score {
                        value: p.score,
                        display: p.display,
                        number_format: p.number_format,
                    },
                );
                out.event(ClientEvent::ScoreboardChanged);
            }
            P::ResetScore(p) => {
                self.scoreboard
                    .reset_score(&p.owner, p.objective_name.as_deref());
                out.event(ClientEvent::ScoreboardChanged);
            }
            P::SetDisplayObjective(p) => {
                self.scoreboard.set_display(p.slot, &p.objective_name);
                out.event(ClientEvent::ScoreboardChanged);
            }
            P::SetPlayerTeam(p) => {
                self.scoreboard.apply_team(&p.name, &p.action);
                out.event(ClientEvent::ScoreboardChanged);
            }
            P::Commands(p) => {
                self.commands = crate::commands::CommandTree::from_packet(&p);
            }
            P::CommandSuggestions(p) => {
                self.command_reply = Some(crate::state::CommandReply {
                    id: p.id,
                    start: usize::try_from(p.start).unwrap_or(0).min(1 << 16),
                    length: usize::try_from(p.length).unwrap_or(0).min(1 << 16),
                    items: p
                        .suggestions
                        .iter()
                        .take(1024)
                        .map(|s| {
                            (
                                s.text.chars().take(256).collect(),
                                s.tooltip.as_ref().map(|t| t.to_plain_string()),
                            )
                        })
                        .collect(),
                });
                self.last_command_suggestions = Some((
                    p.id,
                    p.suggestions
                        .into_iter()
                        .take(1024)
                        .map(|s| s.text)
                        .collect(),
                ));
            }
            P::RecipeBookAdd(p) => {
                for toast in self.gameplay.recipe_book.add(p) {
                    out.event(ClientEvent::Toast(toast));
                }
                out.event(ClientEvent::RecipeBookUpdated);
            }
            P::RecipeBookRemove(p) => {
                self.gameplay
                    .recipe_book
                    .remove(p.recipes.iter().map(|v| v.0));
                out.event(ClientEvent::RecipeBookUpdated);
            }
            P::RecipeBookSettings(p) => {
                self.gameplay.recipe_book.settings = p;
                self.gameplay.recipe_book.revision += 1;
                out.event(ClientEvent::RecipeBookUpdated);
            }
            P::UpdateAdvancements(p) => {
                for toast in self.gameplay.advancements.apply(p) {
                    out.event(ClientEvent::Toast(toast));
                }
                out.event(ClientEvent::AdvancementsUpdated);
            }
            P::PlaceGhostRecipe(p) => {
                self.gameplay
                    .set_ghost_recipe(p.container_id, p.recipe_display);
            }
            P::SelectAdvancementsTab(p) => {
                self.gameplay.advancements.revision += 1;
                self.gameplay.advancements.selected_tab = p.tab.map(|t| t.as_str().to_owned());
            }
            P::MapItemData(p) => {
                let id = p.map_id;
                self.gameplay.apply_map(p);
                out.event(ClientEvent::MapUpdated(id));
            }
            P::MerchantOffers(p) => {
                self.gameplay.merchant = Some(p.into());
                self.gameplay.merchant_revision += 1;
                out.event(ClientEvent::MerchantOffersUpdated);
            }
            P::AwardStats(p) => {
                for (stat, value) in p.stats.0.into_iter().take(crate::gamestate::MAX_STATS) {
                    self.gameplay.set_stat(stat.stat_type, stat.value, value.0);
                }
                self.gameplay.stats_revision += 1;
                out.event(ClientEvent::StatsUpdated);
            }
            P::Waypoint(p) => self.gameplay.waypoints.apply(p),
            P::OpenSignEditor(p) => {
                self.gameplay.sign_editor = Some(crate::gamestate::SignEditor {
                    pos: p.pos,
                    side: p.side,
                });
                out.event(ClientEvent::SignEditorOpened {
                    pos: p.pos,
                    side: p.side,
                });
            }
            P::ShowDialog(p) => {
                let dialog = match &p.dialog {
                    mcr_buf::IdOr::Id(id) => {
                        crate::dialogs::registry_dialog_by_index(&self.registries, *id)
                    }
                    mcr_buf::IdOr::Inline(nbt) => crate::resolve_dialog(&self.registries, &nbt.0),
                };
                if let Some(dialog) = dialog {
                    out.dialogs.push(crate::DialogRequest::Show(dialog));
                }
            }
            P::ClearDialog(_) => out.dialogs.push(crate::DialogRequest::Clear),
            P::TestInstanceBlockStatus(p) => out.event(ClientEvent::TestInstanceStatus {
                status: p.status,
                size: p.size.map(|v| [v.x, v.y, v.z]),
            }),
            P::OpenBook(p) => {
                let stack = self.hand_stack(p.hand);
                self.gameplay.book_editor = self.is_item(&stack, "writable_book").then_some(p.hand);
                out.event(ClientEvent::BookOpened { hand: p.hand });
            }
            other => {
                *self.unhandled.entry(other.name()).or_insert(0) += 1;
            }
        }
        Ok(())
    }

    /// Every stored column is checked, so a long jump of the centre cannot strand old ones.
    fn unload_out_of_range(&mut self, out: &mut Outbox) {
        let doomed: Vec<_> = self
            .chunks
            .positions()
            .filter(|c| !self.chunk_in_range(c.x, c.z))
            .collect();
        for pos in doomed {
            self.unload_chunk(pos, out);
        }
    }

    fn on_login(
        &mut self,
        p: &mcr_protocol::play::clientbound::player::Login,
        out: &mut Outbox,
    ) -> Result<()> {
        self.player.entity_id = p.player_id;
        self.post_effects.clear();
        self.apply_spawn_world(&p.spawn_info, true, None)?;
        self.world.hardcore = p.hardcore;
        self.world.view_distance = p.chunk_radius.clamp(0, 64);
        self.world.simulation_distance = p.simulation_distance.clamp(0, 64);
        self.world.chunk_center = (0, 0);
        self.server.max_players = p.max_players;
        self.server.reduced_debug_info = p.reduced_debug_info;
        self.server.online_mode = p.online_mode;
        self.server.enforces_secure_chat = p.enforces_secure_chat;
        let now_ms = crate::secure_chat::now_millis();
        if let Some(update) = self
            .secure
            .on_login(p.online_mode, p.enforces_secure_chat, now_ms)
        {
            out.replies.push(S::ChatSessionUpdate(update));
        }
        if self.secure.should_warn_insecure() {
            out.event(ClientEvent::Toast(ToastEvent::InsecureServer));
        }
        self.server.do_limited_crafting = p.do_limited_crafting;
        self.player.show_death_screen = p.show_death_screen;
        self.inventory = Inventory::default();
        self.players.clear();
        self.scoreboard.clear();
        self.boss_bars.clear();
        self.in_batch = false;
        out.event(ClientEvent::Joined {
            entity_id: p.player_id,
        });
        Ok(())
    }

    fn apply_chat_completions(
        &mut self,
        action: mcr_protocol::play::clientbound::player::CompletionAction,
        entries: Vec<String>,
    ) {
        use mcr_protocol::play::clientbound::player::CompletionAction as A;
        let entries = entries
            .into_iter()
            .take(crate::state::MAX_CHAT_COMPLETIONS)
            .map(|e| {
                e.chars()
                    .filter(|c| !c.is_control())
                    .take(crate::state::MAX_COMPLETION_CHARS)
                    .collect::<String>()
            })
            .filter(|e| !e.is_empty());
        let list = &mut self.chat_completions;
        match action {
            A::Set => {
                list.clear();
                list.extend(entries);
            }
            A::Add => {
                for e in entries {
                    if list.len() < crate::state::MAX_CHAT_COMPLETIONS && !list.contains(&e) {
                        list.push(e);
                    }
                }
            }
            A::Remove => {
                let drop: Vec<String> = entries.collect();
                list.retain(|e| !drop.contains(e));
            }
        }
        list.truncate(crate::state::MAX_CHAT_COMPLETIONS);
    }

    fn on_respawn(
        &mut self,
        info: &CommonPlayerSpawnInfo,
        keep: u8,
        out: &mut Outbox,
    ) -> Result<()> {
        let changed = info.dimension != self.dimension.name;
        self.camera_entity = None;
        self.post_effects.clear();
        self.apply_spawn_world(info, changed, Some(keep))?;
        self.inventory.open = None;
        self.inventory.cursor = ItemStack::default();
        out.event(ClientEvent::Respawned);
        Ok(())
    }

    fn apply_spawn_world(
        &mut self,
        info: &CommonPlayerSpawnInfo,
        rebuild: bool,
        keep: Option<u8>,
    ) -> Result<()> {
        let dimension = Dimension {
            name: info.dimension.clone(),
            type_id: info.dimension_type,
            sea_level: info.sea_level,
            seed: info.seed,
            is_flat: info.is_flat,
            is_debug: info.is_debug,
            ..Dimension::default()
        };
        if rebuild {
            self.build_world(dimension)?;
        } else {
            self.dimension.sea_level = info.sea_level;
            self.dimension.seed = info.seed;
        }
        self.player
            .reset_for_spawn(info.game_type, info.previous_game_type, keep);
        crate::glide::apply_player_flags(&mut self.player);
        self.refresh_physics_inputs();
        Ok(())
    }

    fn on_game_event(&mut self, event: u8, param: f32, out: &mut Outbox) {
        let int = (param + 0.5).floor() as i32;
        match event {
            1 => {
                self.world.raining = true;
                self.world.rain_level = 0.0;
                out.event(ClientEvent::WeatherChanged);
            }
            2 => {
                self.world.raining = false;
                self.world.rain_level = 1.0;
                out.event(ClientEvent::WeatherChanged);
            }
            3 => {
                let mode = match int {
                    0 => GameType::Survival,
                    1 => GameType::Creative,
                    2 => GameType::Adventure,
                    3 => GameType::Spectator,
                    _ => return,
                };
                self.player.apply_game_mode(mode);
                out.event(ClientEvent::GameModeChanged);
            }
            0 => self.push_chat(
                ChatKind::System,
                None,
                Component::translatable("block.minecraft.spawn.not_valid", Vec::new()),
                None,
                None,
                out,
            ),
            4 => out.event(ClientEvent::ShowCredits),
            5 => self.on_demo_event(param, out),
            6 => {
                let [x, y, z] = self.player.physics.pos;
                let eye = y + f64::from(self.player.physics.eye_height());
                out.event(ClientEvent::LocalSound {
                    sound: "entity.arrow.hit_player",
                    pos: [x, eye, z],
                    volume: 0.18,
                    pitch: 0.45,
                    source: SoundSource::Players,
                });
            }
            9 => out.event(ClientEvent::LocalSound {
                sound: "entity.puffer_fish.sting",
                pos: self.player.physics.pos,
                volume: 1.0,
                pitch: 1.0,
                source: SoundSource::Neutral,
            }),
            10 => {
                out.event(ClientEvent::ElderGuardianGhost);
                if int == 1 {
                    out.event(ClientEvent::LocalSound {
                        sound: "entity.elder_guardian.curse",
                        pos: self.player.physics.pos,
                        volume: 1.0,
                        pitch: 1.0,
                        source: SoundSource::Hostile,
                    });
                }
            }
            7 if param.is_finite() => {
                self.world.rain_level = param.clamp(0.0, 1.0);
                out.event(ClientEvent::WeatherChanged);
            }
            8 if param.is_finite() => {
                self.world.thunder_level = param.clamp(0.0, 1.0);
                out.event(ClientEvent::WeatherChanged);
            }
            11 => self.player.show_death_screen = param == 0.0,
            12 => self.server.do_limited_crafting = param == 1.0,
            _ => {}
        }
    }

    fn on_demo_event(&mut self, param: f32, out: &mut Outbox) {
        use mcr_protocol::types::text::Arg;
        let key = |name: &str| {
            Arg::Component(Component::translatable(
                format!("key.keyboard.{name}"),
                Vec::new(),
            ))
        };
        let lines: Vec<(&str, Vec<Arg>)> = if param == 0.0 {
            vec![
                ("demo.help.title", Vec::new()),
                ("demo.help.fullWrapped", Vec::new()),
            ]
        } else if param == 101.0 {
            vec![(
                "demo.help.movement",
                vec![key("w"), key("a"), key("s"), key("d")],
            )]
        } else if param == 102.0 {
            vec![("demo.help.jump", vec![key("space")])]
        } else if param == 103.0 {
            vec![("demo.help.inventory", vec![key("e")])]
        } else if param == 104.0 {
            vec![("demo.day.6", vec![key("f2")])]
        } else {
            Vec::new()
        };
        for (k, args) in lines {
            self.push_chat(
                ChatKind::System,
                None,
                Component::translatable(k, args),
                None,
                None,
                out,
            );
        }
    }

    fn set_overlay(&mut self, text: Component, out: &mut Outbox) {
        self.titles.set_overlay(text.clone());
        out.event(ClientEvent::Chat {
            kind: ChatKind::ActionBar,
            sender: None,
            message: text,
            tag: None,
            signature: None,
        });
    }

    /// Whether `vehicle` already carries the player, directly or through a stack of mounts.
    fn carries_player(&self, vehicle: i32) -> bool {
        let mut at = self.player.vehicle;
        for _ in 0..16 {
            match at {
                Some(v) if v == vehicle => return true,
                Some(v) => at = self.entities.get(v).and_then(|e| e.vehicle),
                None => return false,
            }
        }
        false
    }

    fn on_set_passengers(&mut self, vehicle: i32, passengers: &[i32], out: &mut Outbox) {
        let was_mounted = self.carries_player(vehicle);
        self.entities.set_passengers(vehicle, passengers);
        if vehicle < 0 {
            return;
        }
        let rides = passengers.contains(&self.player.entity_id);
        if !rides {
            if self.player.vehicle == Some(vehicle) {
                self.player.vehicle = None;
            }
            return;
        }
        self.player.vehicle = Some(vehicle);
        if was_mounted {
            return;
        }
        let Some(e) = self.entities.get(vehicle) else {
            return;
        };
        let name = self.data.entity_type_name(e.kind as u32).unwrap_or("");
        if name.ends_with("_boat") || name.ends_with("_raft") {
            let yaw = e.yaw;
            let phys = &mut self.player.physics;
            let (pos, vel, pitch) = (phys.pos, phys.vel, phys.pitch);
            phys.snap_to(pos, vel, yaw, pitch);
            self.player.view_snapped = true;
            out.event(ClientEvent::PlayerRotated);
        }
        let key = if name == "minecraft:cushion" {
            "mount.sit"
        } else {
            "mount.onboard"
        };
        let sneak = Component::translatable("key.keyboard.left.shift", Vec::new());
        self.set_overlay(
            Component::translatable(key, vec![mcr_protocol::types::text::Arg::Component(sneak)]),
            out,
        );
    }

    fn on_chunk(&mut self, x: i32, z: i32, payload: &[u8], out: &mut Outbox) {
        if !self.chunk_in_range(x, z) {
            return;
        }
        let cfg = *self.chunks.config();
        let mut input = payload;
        let decoded = ChunkData::decode(&mut input, &cfg)
            .and_then(|data| Ok((data, LightData::decode(&mut input, &cfg)?)));
        let Ok((data, light)) = decoded else {
            tracing::warn!(x, z, "dropping undecodable chunk");
            return;
        };
        let mut column = ChunkColumn::from_data(data, &cfg);
        column.apply_light(light);
        let pos = ChunkPos::new(x, z);
        self.chunks.insert(pos, column);
        self.chunks_received += 1;
        out.event(ClientEvent::ChunkLoaded(pos));
    }

    fn on_light(&mut self, x: i32, z: i32, payload: &[u8], out: &mut Outbox) {
        let cfg = *self.chunks.config();
        let mut input = payload;
        let Ok(light) = LightData::decode(&mut input, &cfg) else {
            return;
        };
        let pos = ChunkPos::new(x, z);
        if self.chunks.apply_light(pos, light) {
            out.event(ClientEvent::LightUpdated(pos));
        }
    }

    pub(crate) fn unload_chunk(&mut self, pos: ChunkPos, out: &mut Outbox) {
        if self.chunks.remove(pos).is_some() {
            self.block_entities
                .retain(|p, _| ChunkPos::from_block(p.x, p.z) != pos);
            out.event(ClientEvent::ChunkUnloaded(pos));
        }
    }

    fn set_block(&mut self, pos: BlockPos, state: u32, out: &mut Outbox) {
        if state >= self.chunks.config().block_state_count() {
            return;
        }
        let new = mcr_world::BlockState(state);
        if self.predictions.hold(pos, new.0) {
            return;
        }
        if self.chunks.set_block(pos.x, pos.y, pos.z, new).is_some() {
            let has_entity = self
                .data
                .block_state(state)
                .is_some_and(|s| s.has_block_entity());
            if !has_entity {
                self.block_entities.remove(&pos);
            }
            out.event(ClientEvent::BlockChanged { pos, state });
        }
    }

    fn move_entity(
        &mut self,
        id: i32,
        delta: Option<&mcr_protocol::play::clientbound::entity::VecDelta>,
        rot: Option<(f32, f32)>,
        on_ground: bool,
        out: &mut Outbox,
    ) {
        if self.controlled_vehicle == Some(id) {
            return;
        }
        let Some(e) = self.entities.get_mut(id) else {
            return;
        };
        match delta {
            Some(d) => {
                let end = end_of_delta(e.target_pos, d);
                e.move_or_interpolate_to(end, rot);
            }
            None => {
                if let Some((yaw, pitch)) = rot {
                    e.rotate_or_interpolate_to(yaw, pitch);
                }
            }
        }
        e.on_ground = on_ground;
        out.event(ClientEvent::EntityMoved(id));
    }

    fn take_item_entity(
        &mut self,
        p: &mcr_protocol::play::clientbound::entity::TakeItemEntity,
        out: &mut Outbox,
    ) {
        let Some(item) = self.entities.get(p.item_id).cloned() else {
            return;
        };
        let name = self.data.entity_type_name(item.kind as u32);
        let experience_orb = name == Some("minecraft:experience_orb");
        let is_item = name == Some("minecraft:item");
        let silent = matches!(item.data.get(&4), Some(EntityDataValue::Boolean(true)));
        out.event(ClientEvent::ItemTaken {
            item_id: p.item_id,
            collector_id: p.player_id,
            amount: p.amount,
            pos: item.pos,
            experience_orb,
            silent,
        });
        if let Some(target) = self.pickup_target(p.player_id)
            && self.pickups.len() < crate::pickup::MAX_PICKUPS
        {
            self.pickups
                .push(crate::pickup::Pickup::new(item, p.player_id, target));
        }
        if experience_orb {
            return;
        }
        let emptied = match self.entities.get_mut(p.item_id) {
            Some(e) if is_item => match e.data.get_mut(&8) {
                Some(EntityDataValue::ItemStack(stack)) => {
                    stack.count = stack.count.saturating_sub(p.amount);
                    stack.count <= 0
                }
                _ => true,
            },
            _ => true,
        };
        if emptied && self.entities.remove(p.item_id).is_some() {
            out.event(ClientEvent::EntityRemoved(p.item_id));
        }
    }

    fn teleport_entity(&mut self, p: &TeleportEntity, out: &mut Outbox) {
        let Some(e) = self.entities.get_mut(p.entity_id) else {
            return;
        };
        let rel = |flag: u32| Relatives(p.relatives.0).contains(flag);
        let pos = [
            if rel(Relatives::X) {
                e.target_pos[0] + p.change.position.x
            } else {
                p.change.position.x
            },
            if rel(Relatives::Y) {
                e.target_pos[1] + p.change.position.y
            } else {
                p.change.position.y
            },
            if rel(Relatives::Z) {
                e.target_pos[2] + p.change.position.z
            } else {
                p.change.position.z
            },
        ];
        let yaw = if rel(Relatives::Y_ROT) {
            e.target_yaw + p.change.y_rot
        } else {
            p.change.y_rot
        };
        let pitch = if rel(Relatives::X_ROT) {
            e.target_pitch + p.change.x_rot
        } else {
            p.change.x_rot
        };
        if !sane3(pos) || !yaw.is_finite() || !pitch.is_finite() {
            return;
        }
        let vel = v3(&p.change.delta_movement);
        if sane3(vel) {
            e.velocity = clamp_motion(vel);
        }
        e.move_or_interpolate_to(pos, Some((yaw, pitch.clamp(-90.0, 90.0))));
        e.on_ground = p.on_ground;
        out.event(ClientEvent::EntityMoved(p.entity_id));
    }

    fn position_sync(&mut self, p: &EntityPositionSync, out: &mut Outbox) {
        let Some(e) = self.entities.get_mut(p.entity_id) else {
            return;
        };
        let pos = end_of_path(&p.position);
        if !sane3(pos) || !p.y_rot.is_finite() || !p.x_rot.is_finite() {
            return;
        }
        e.move_or_interpolate_to(pos, Some((p.y_rot, p.x_rot.clamp(-90.0, 90.0))));
        e.on_ground = p.on_ground;
        out.event(ClientEvent::EntityMoved(p.entity_id));
    }

    fn on_player_position(
        &mut self,
        p: &mcr_protocol::play::clientbound::player::PlayerPosition,
        out: &mut Outbox,
    ) {
        if self.player.vehicle.is_none() {
            self.player
                .apply_teleport(p.position, p.delta_movement, p.y_rot, p.x_rot, p.relatives);
        }
        self.predictions.on_teleport();
        self.player.teleports_received += 1;
        self.player.last_teleport_id = Some(p.id);
        let phys = &self.player.physics;
        out.replies
            .push(S::AcceptTeleportation(sb::AcceptTeleportation {
                id: p.id,
                x: phys.pos[0],
                y: phys.pos[1],
                z: phys.pos[2],
                y_rot: phys.yaw,
                x_rot: phys.pitch,
            }));
        out.event(ClientEvent::PlayerTeleported { id: p.id });
    }

    fn damage_effects(&self, source_type: i32) -> DamageEffects {
        let Some(item) = self
            .registries
            .get("minecraft:damage_type")
            .and_then(|r| r.get(source_type))
        else {
            return DamageEffects::Hurt;
        };
        match item.data.as_ref().and_then(|d| d.get_str("effects")) {
            Some(name) => DamageEffects::from_name(name),
            None => DamageEffects::vanilla_default(item.id.path()),
        }
    }

    fn on_look_at(&mut self, p: &PlayerLookAt, out: &mut Outbox) {
        let phys = &self.player.physics;
        let eye = f64::from(phys.eye_height());
        let from = [
            phys.pos[0],
            phys.pos[1]
                + if p.from_anchor == EntityAnchor::Eyes {
                    eye
                } else {
                    0.0
                },
            phys.pos[2],
        ];
        let target = match &p.entity {
            Some(look) => match self.entities.get(look.entity_id) {
                Some(e) => [
                    e.target_pos[0],
                    e.target_pos[1]
                        + if look.anchor == EntityAnchor::Eyes {
                            1.62
                        } else {
                            0.0
                        },
                    e.target_pos[2],
                ],
                None => return,
            },
            None => [p.x, p.y, p.z],
        };
        if !sane3(target) {
            return;
        }
        let (dx, dy, dz) = (
            target[0] - from[0],
            target[1] - from[1],
            target[2] - from[2],
        );
        let horizontal = dx.hypot(dz);
        let pitch = (-dy.atan2(horizontal)).to_degrees() as f32;
        let yaw = (dz.atan2(dx).to_degrees() - 90.0) as f32;
        let (pos, vel) = (phys.pos, phys.vel);
        self.player
            .physics
            .snap_to(pos, vel, yaw, pitch.clamp(-90.0, 90.0));
        self.player.view_snapped = true;
        out.event(ClientEvent::PlayerRotated);
    }

    fn on_boss_event(&mut self, id: Uuid, op: BossEventOperation, now: Instant, out: &mut Outbox) {
        match op {
            BossEventOperation::Add {
                name,
                progress,
                color,
                overlay,
                flags,
            } => {
                crate::social::boss_bars_insert(
                    &mut self.boss_bars,
                    id,
                    BossBar::new(name, progress, color, overlay, flags),
                );
            }
            BossEventOperation::Remove => {
                self.boss_bars.remove(&id);
            }
            BossEventOperation::UpdateProgress(v) => {
                if let Some(b) = self.boss_bars.get_mut(&id) {
                    b.set_progress(v, now);
                }
            }
            BossEventOperation::UpdateName(name) => {
                if let Some(b) = self.boss_bars.get_mut(&id) {
                    b.name = name;
                }
            }
            BossEventOperation::UpdateStyle { color, overlay } => {
                if let Some(b) = self.boss_bars.get_mut(&id) {
                    b.color = color;
                    b.overlay = overlay;
                }
            }
            BossEventOperation::UpdateProperties(flags) => {
                if let Some(b) = self.boss_bars.get_mut(&id) {
                    b.flags = flags;
                }
            }
        }
        out.event(ClientEvent::BossBarChanged(id));
    }

    /// Server and client system lines carry vanilla's system tag.
    fn push_chat(
        &mut self,
        kind: ChatKind,
        sender: Option<Uuid>,
        content: Component,
        signature: Option<Box<mcr_protocol::types::game::MessageSignature>>,
        timestamp_millis: Option<i64>,
        out: &mut Outbox,
    ) {
        let tag = matches!(kind, ChatKind::System | ChatKind::Disguised).then_some(ChatTag::System);
        self.push_chat_tagged(kind, sender, content, signature, timestamp_millis, tag, out);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn push_chat_tagged(
        &mut self,
        kind: ChatKind,
        sender: Option<Uuid>,
        content: Component,
        signature: Option<Box<mcr_protocol::types::game::MessageSignature>>,
        timestamp_millis: Option<i64>,
        tag: Option<ChatTag>,
        out: &mut Outbox,
    ) {
        self.chat.push(ChatMessage {
            kind,
            sender,
            content: content.clone(),
            signature: signature.clone(),
            timestamp_millis,
            tag: tag.clone(),
        });
        out.event(ClientEvent::Chat {
            kind,
            sender,
            message: content,
            tag,
            signature,
        });
    }

    pub(crate) fn format_chat(&self, bound: &BoundChatType, content: Component) -> Component {
        use mcr_protocol::types::text::Arg;
        let (key, params, style) = match &bound.chat_type {
            IdOr::Inline(t) => (
                t.chat.translation_key.clone(),
                t.chat
                    .parameters
                    .iter()
                    .map(|p| format!("{p:?}").to_lowercase())
                    .collect::<Vec<_>>(),
                decoration_style(&t.chat.style.0),
            ),
            IdOr::Id(id) => {
                let decoration = self
                    .registries
                    .get(CHAT_TYPE)
                    .and_then(|r| r.get(*id))
                    .and_then(|i| i.data.as_ref())
                    .and_then(|c| c.get_compound("chat"));
                let key = decoration.and_then(|d| d.get_str("translation_key"));
                let params = decoration
                    .and_then(|d| d.get_list("parameters"))
                    .map(|l| l.as_strings().to_vec());
                match (key, params) {
                    (Some(k), Some(p)) => (
                        k.to_owned(),
                        p,
                        decoration
                            .and_then(|d| d.get("style"))
                            .map(decoration_style)
                            .unwrap_or_default(),
                    ),
                    _ => (
                        "chat.type.text".to_owned(),
                        vec!["sender".into(), "content".into()],
                        Style::default(),
                    ),
                }
            }
        };
        let args: Vec<Arg> = params
            .iter()
            .take(8)
            .filter_map(|p| match p.as_str() {
                "sender" => Some(Arg::Component(bound.name.clone())),
                "target" => bound.target_name.clone().map(Arg::Component),
                "content" => Some(Arg::Component(content.clone())),
                _ => None,
            })
            .collect();
        let fallback = vec!["%s"; args.len()].join(" ");
        let mut line = Component::from_content(mcr_protocol::types::text::Content::Translatable {
            key,
            fallback: Some(fallback),
            args,
        });
        line.style = style;
        line
    }
}

/// A chat type decoration's `style` compound, read the way text components read theirs.
fn decoration_style(style: &mcr_nbt::Value) -> Style {
    match style {
        mcr_nbt::Value::Compound(c) if !c.is_empty() => {
            let mut c = c.clone();
            c.insert("text", "");
            Component::from_nbt_lenient(&mcr_nbt::Value::Compound(c)).style
        }
        _ => Style::default(),
    }
}

/// Vanilla clamps synced motion to this per axis.
fn clamp_motion(v: [f64; 3]) -> [f64; 3] {
    v.map(|c| c.clamp(-3.9, 3.9))
}
