//! One enum per (state, direction) over every packet, decoded by id through [`PacketIds`].

use mcr_buf::{Decode, Encode};

use crate::ids::{PacketIds, PacketSet};
use crate::{Direction, Packet, ProtocolError, ProtocolState};

const fn index_of(names: &[&str], name: &str) -> usize {
    let mut i = 0;
    while i < names.len() {
        if str_eq(names[i], name) {
            return i;
        }
        i += 1;
    }
    panic!("packet name missing from its dispatch enum");
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn same_direction(a: Direction, b: Direction) -> bool {
    matches!(
        (a, b),
        (Direction::Clientbound, Direction::Clientbound)
            | (Direction::Serverbound, Direction::Serverbound)
    )
}

macro_rules! packets {
    (
        $enum:ident $(<$lt:lifetime>)?, $state:ident, $dir:ident;
        $($name:literal => $variant:ident($ty:ty),)*
    ) => {
        #[derive(Debug, Clone, PartialEq)]
        #[allow(clippy::large_enum_variant)] // decoded and consumed immediately; boxing would cost an allocation per packet
        pub enum $enum $(<$lt>)? {
            $($variant($ty)),*
        }

        impl $(<$lt>)? PacketSet for $enum $(<$lt>)? {
            const STATE: ProtocolState = ProtocolState::$state;
            const DIRECTION: Direction = Direction::$dir;
            const NAMES: &'static [&'static str] = &[$(concat!("minecraft:", $name)),*];
        }

        impl $(<$lt>)? $enum $(<$lt>)? {
            const DECODERS: [fn(&mut &$($lt)? [u8]) -> mcr_buf::Result<Self>; [$($name),*].len()] =
                [$(|input| Ok(Self::$variant(Decode::decode(input)?))),*];

            const NAMES_MATCH_TYPES: () = {
                $(assert!(
                    str_eq(<$ty as Packet>::NAME, concat!("minecraft:", $name))
                        && same_direction(<$ty as Packet>::DIRECTION, Direction::$dir),
                    concat!("packet type does not match ", $name),
                );)*
            };

            fn variant_index(&self) -> usize {
                match self {
                    $(Self::$variant(_) => const {
                        index_of(<Self as PacketSet>::NAMES, concat!("minecraft:", $name))
                    },)*
                }
            }

            pub fn name(&self) -> &'static str {
                <Self as PacketSet>::NAMES[self.variant_index()]
            }

            pub fn id(&self, ids: &PacketIds) -> i32 {
                ids.id(ProtocolState::$state, Direction::$dir, self.variant_index())
            }

            /// Fails unless `payload` is consumed exactly.
            pub fn decode(
                id: i32,
                payload: &$($lt)? [u8],
                ids: &PacketIds,
            ) -> Result<Self, ProtocolError> {
                let () = Self::NAMES_MATCH_TYPES;
                let state = ProtocolState::$state;
                let direction = Direction::$dir;
                let variant = ids
                    .variant(state, direction, id)
                    .ok_or(ProtocolError::UnknownId { state, direction, id })?;
                let packet_name = <Self as PacketSet>::NAMES[variant];
                let mut input = payload;
                let packet = (Self::DECODERS[variant])(&mut input).map_err(|source| {
                    ProtocolError::Decode { packet: packet_name, state, direction, source }
                })?;
                if !input.is_empty() {
                    return Err(ProtocolError::TrailingBytes {
                        packet: packet_name,
                        state,
                        direction,
                        remaining: input.len(),
                    });
                }
                Ok(packet)
            }

            /// Appends the payload (no id) to `out` and returns the packet id.
            pub fn encode_into(&self, ids: &PacketIds, out: &mut Vec<u8>) -> i32 {
                match self {
                    $(Self::$variant(packet) => Encode::encode(packet, out),)*
                }
                self.id(ids)
            }

            pub fn encode(&self, ids: &PacketIds) -> (i32, Vec<u8>) {
                let mut out = Vec::new();
                let id = self.encode_into(ids, &mut out);
                (id, out)
            }
        }
    };
}

packets! {
    HandshakeServerbound, Handshake, Serverbound;
    "intention" => Intention(crate::handshake::Intention),
}

packets! {
    StatusClientbound, Status, Clientbound;
    "status_response" => StatusResponse(crate::status::StatusResponse),
    "pong_response" => PongResponse(crate::status::PongResponse),
}

packets! {
    StatusServerbound, Status, Serverbound;
    "status_request" => StatusRequest(crate::status::StatusRequest),
    "ping_request" => PingRequest(crate::status::PingRequest),
}

packets! {
    LoginClientbound<'a>, Login, Clientbound;
    "login_disconnect" => LoginDisconnect(crate::login::LoginDisconnect),
    "hello" => Hello(crate::login::ClientboundHello<'a>),
    "login_finished" => LoginFinished(crate::login::LoginFinished),
    "login_compression" => LoginCompression(crate::login::LoginCompression),
    "custom_query" => CustomQuery(crate::login::CustomQuery<'a>),
    "cookie_request" => CookieRequest(crate::common::CookieRequest),
}

packets! {
    LoginServerbound<'a>, Login, Serverbound;
    "hello" => Hello(crate::login::ServerboundHello),
    "key" => Key(crate::login::Key<'a>),
    "custom_query_answer" => CustomQueryAnswer(crate::login::CustomQueryAnswer<'a>),
    "login_acknowledged" => LoginAcknowledged(crate::login::LoginAcknowledged),
    "cookie_response" => CookieResponse(crate::common::CookieResponse<'a>),
}

packets! {
    ConfigClientbound<'a>, Configuration, Clientbound;
    "cookie_request" => CookieRequest(crate::common::CookieRequest),
    "custom_payload" => CustomPayload(crate::common::ClientboundCustomPayload<'a>),
    "disconnect" => Disconnect(crate::common::Disconnect),
    "finish_configuration" => FinishConfiguration(crate::configuration::ClientboundFinishConfiguration),
    "keep_alive" => KeepAlive(crate::common::ClientboundKeepAlive),
    "ping" => Ping(crate::common::Ping),
    "reset_chat" => ResetChat(crate::configuration::ResetChat),
    "registry_data" => RegistryData(crate::configuration::RegistryData),
    "resource_pack_pop" => ResourcePackPop(crate::common::ResourcePackPop),
    "resource_pack_push" => ResourcePackPush(crate::common::ResourcePackPush),
    "post_effects" => PostEffects(crate::common::PostEffects),
    "store_cookie" => StoreCookie(crate::common::StoreCookie<'a>),
    "transfer" => Transfer(crate::common::Transfer),
    "update_enabled_features" => UpdateEnabledFeatures(crate::configuration::UpdateEnabledFeatures),
    "update_tags" => UpdateTags(crate::common::UpdateTags),
    "select_known_packs" => SelectKnownPacks(crate::configuration::ClientboundSelectKnownPacks),
    "custom_report_details" => CustomReportDetails(crate::common::CustomReportDetails),
    "server_links" => ServerLinks(crate::common::ServerLinks),
    "clear_dialog" => ClearDialog(crate::common::ClearDialog),
    "show_dialog" => ShowDialog(crate::common::ConfigShowDialog),
    "code_of_conduct" => CodeOfConduct(crate::configuration::CodeOfConduct),
}

packets! {
    ConfigServerbound<'a>, Configuration, Serverbound;
    "client_information" => ClientInformation(crate::common::ClientInformation),
    "cookie_response" => CookieResponse(crate::common::CookieResponse<'a>),
    "custom_payload" => CustomPayload(crate::common::ServerboundCustomPayload<'a>),
    "finish_configuration" => FinishConfiguration(crate::configuration::ServerboundFinishConfiguration),
    "keep_alive" => KeepAlive(crate::common::ServerboundKeepAlive),
    "pong" => Pong(crate::common::Pong),
    "resource_pack" => ResourcePack(crate::common::ResourcePackResponse),
    "select_known_packs" => SelectKnownPacks(crate::configuration::ServerboundSelectKnownPacks),
    "custom_click_action" => CustomClickAction(crate::common::CustomClickAction),
    "accept_code_of_conduct" => AcceptCodeOfConduct(crate::configuration::AcceptCodeOfConduct),
}

packets! {
    PlayClientbound<'a>, Play, Clientbound;
    "bundle_delimiter" => BundleDelimiter(crate::play::clientbound::world::BundleDelimiter),
    "add_entity" => AddEntity(crate::play::clientbound::entity::AddEntity),
    "animate" => Animate(crate::play::clientbound::entity::Animate),
    "award_stats" => AwardStats(crate::play::clientbound::player::AwardStats),
    "block_changed_ack" => BlockChangedAck(crate::play::clientbound::world::BlockChangedAck),
    "block_destruction" => BlockDestruction(crate::play::clientbound::world::BlockDestruction),
    "block_entity_data" => BlockEntityData(crate::play::clientbound::world::BlockEntityData),
    "block_event" => BlockEvent(crate::play::clientbound::world::BlockEvent),
    "block_update" => BlockUpdate(crate::play::clientbound::world::BlockUpdate),
    "boss_event" => BossEvent(crate::play::clientbound::ui::BossEvent),
    "change_difficulty" => ChangeDifficulty(crate::play::clientbound::player::ChangeDifficulty),
    "chunk_batch_finished" => ChunkBatchFinished(crate::play::clientbound::world::ChunkBatchFinished),
    "chunk_batch_start" => ChunkBatchStart(crate::play::clientbound::world::ChunkBatchStart),
    "chunks_biomes" => ChunksBiomes(crate::play::clientbound::world::ChunksBiomes<'a>),
    "clear_titles" => ClearTitles(crate::play::clientbound::ui::ClearTitles),
    "command_suggestions" => CommandSuggestions(crate::play::clientbound::ui::CommandSuggestions),
    "commands" => Commands(crate::play::clientbound::ui::Commands),
    "container_close" => ContainerClose(crate::play::clientbound::ui::ContainerClose),
    "container_set_content" => ContainerSetContent(crate::play::clientbound::ui::ContainerSetContent),
    "container_set_data" => ContainerSetData(crate::play::clientbound::ui::ContainerSetData),
    "container_set_slot" => ContainerSetSlot(crate::play::clientbound::ui::ContainerSetSlot),
    "cookie_request" => CookieRequest(crate::common::CookieRequest),
    "cooldown" => Cooldown(crate::play::clientbound::ui::Cooldown),
    "custom_chat_completions" => CustomChatCompletions(crate::play::clientbound::player::CustomChatCompletions),
    "custom_payload" => CustomPayload(crate::common::ClientboundCustomPayload<'a>),
    "damage_event" => DamageEvent(crate::play::clientbound::entity::DamageEvent),
    "debug/block_value" => DebugBlockValue(crate::play::clientbound::world::DebugBlockValue<'a>),
    "debug/chunk_value" => DebugChunkValue(crate::play::clientbound::world::DebugChunkValue<'a>),
    "debug/entity_value" => DebugEntityValue(crate::play::clientbound::world::DebugEntityValue<'a>),
    "debug/event" => DebugEvent(crate::play::clientbound::world::DebugEvent<'a>),
    "debug_sample" => DebugSample(crate::play::clientbound::world::DebugSample),
    "delete_chat" => DeleteChat(crate::play::clientbound::player::DeleteChat),
    "disconnect" => Disconnect(crate::common::Disconnect),
    "disguised_chat" => DisguisedChat(crate::play::clientbound::player::DisguisedChat),
    "entity_event" => EntityEvent(crate::play::clientbound::entity::EntityEvent),
    "entity_position_sync" => EntityPositionSync(crate::play::clientbound::entity::EntityPositionSync),
    "explode" => Explode(crate::play::clientbound::world::Explode),
    "add_transient_block" => AddTransientBlock(crate::play::clientbound::world::AddTransientBlock),
    "forget_level_chunk" => ForgetLevelChunk(crate::play::clientbound::world::ForgetLevelChunk),
    "game_event" => GameEvent(crate::play::clientbound::world::GameEvent),
    "game_rule_values" => GameRuleValues(crate::play::clientbound::player::GameRuleValues),
    "game_test_highlight_pos" => GameTestHighlightPos(crate::play::clientbound::world::GameTestHighlightPos),
    "mount_screen_open" => MountScreenOpen(crate::play::clientbound::ui::MountScreenOpen),
    "hurt_animation" => HurtAnimation(crate::play::clientbound::entity::HurtAnimation),
    "initialize_border" => InitializeBorder(crate::play::clientbound::world::InitializeBorder),
    "keep_alive" => KeepAlive(crate::common::ClientboundKeepAlive),
    "level_chunk_with_light" => LevelChunkWithLight(crate::play::clientbound::world::LevelChunkWithLight<'a>),
    "level_event" => LevelEvent(crate::play::clientbound::world::LevelEvent),
    "level_particles" => LevelParticles(crate::play::clientbound::world::LevelParticles),
    "light_update" => LightUpdate(crate::play::clientbound::world::LightUpdate<'a>),
    "login" => Login(crate::play::clientbound::player::Login),
    "low_disk_space_warning" => LowDiskSpaceWarning(crate::play::clientbound::player::LowDiskSpaceWarning),
    "map_item_data" => MapItemData(crate::play::clientbound::ui::MapItemData),
    "merchant_offers" => MerchantOffers(crate::play::clientbound::ui::MerchantOffers),
    "move_entity_pos" => MoveEntityPos(crate::play::clientbound::entity::MoveEntityPos),
    "move_entity_pos_rot" => MoveEntityPosRot(crate::play::clientbound::entity::MoveEntityPosRot),
    "move_minecart_along_track" => MoveMinecartAlongTrack(crate::play::clientbound::entity::MoveMinecartAlongTrack),
    "move_entity_rot" => MoveEntityRot(crate::play::clientbound::entity::MoveEntityRot),
    "move_vehicle" => MoveVehicle(crate::play::clientbound::entity::MoveVehicle),
    "open_book" => OpenBook(crate::play::clientbound::ui::OpenBook),
    "open_screen" => OpenScreen(crate::play::clientbound::ui::OpenScreen),
    "open_sign_editor" => OpenSignEditor(crate::play::clientbound::ui::OpenSignEditor),
    "ping" => Ping(crate::common::Ping),
    "pong_response" => PongResponse(crate::status::PongResponse),
    "place_ghost_recipe" => PlaceGhostRecipe(crate::play::clientbound::ui::PlaceGhostRecipe),
    "player_abilities" => PlayerAbilities(crate::play::clientbound::player::PlayerAbilities),
    "player_chat" => PlayerChat(crate::play::clientbound::player::PlayerChat),
    "player_combat_end" => PlayerCombatEnd(crate::play::clientbound::player::PlayerCombatEnd),
    "player_combat_enter" => PlayerCombatEnter(crate::play::clientbound::player::PlayerCombatEnter),
    "player_combat_kill" => PlayerCombatKill(crate::play::clientbound::player::PlayerCombatKill),
    "player_info_remove" => PlayerInfoRemove(crate::play::clientbound::player::PlayerInfoRemove),
    "player_info_update" => PlayerInfoUpdate(crate::play::clientbound::player::PlayerInfoUpdate),
    "player_look_at" => PlayerLookAt(crate::play::clientbound::player::PlayerLookAt),
    "player_position" => PlayerPosition(crate::play::clientbound::player::PlayerPosition),
    "player_rotation" => PlayerRotation(crate::play::clientbound::player::PlayerRotation),
    "recipe_book_add" => RecipeBookAdd(crate::play::clientbound::ui::RecipeBookAdd),
    "recipe_book_remove" => RecipeBookRemove(crate::play::clientbound::ui::RecipeBookRemove),
    "recipe_book_settings" => RecipeBookSettings(crate::play::clientbound::ui::RecipeBookSettings),
    "remove_entities" => RemoveEntities(crate::play::clientbound::entity::RemoveEntities),
    "remove_mob_effect" => RemoveMobEffect(crate::play::clientbound::entity::RemoveMobEffect),
    "reset_score" => ResetScore(crate::play::clientbound::ui::ResetScore),
    "resource_pack_pop" => ResourcePackPop(crate::common::ResourcePackPop),
    "resource_pack_push" => ResourcePackPush(crate::common::ResourcePackPush),
    "post_effects" => PostEffects(crate::common::PostEffects),
    "respawn" => Respawn(crate::play::clientbound::player::Respawn),
    "rotate_head" => RotateHead(crate::play::clientbound::entity::RotateHead),
    "section_blocks_update" => SectionBlocksUpdate(crate::play::clientbound::world::SectionBlocksUpdate),
    "select_advancements_tab" => SelectAdvancementsTab(crate::play::clientbound::ui::SelectAdvancementsTab),
    "server_data" => ServerData(crate::play::clientbound::player::ServerData),
    "set_action_bar_text" => SetActionBarText(crate::play::clientbound::ui::SetActionBarText),
    "set_border_center" => SetBorderCenter(crate::play::clientbound::world::SetBorderCenter),
    "set_border_lerp_size" => SetBorderLerpSize(crate::play::clientbound::world::SetBorderLerpSize),
    "set_border_size" => SetBorderSize(crate::play::clientbound::world::SetBorderSize),
    "set_border_warning_delay" => SetBorderWarningDelay(crate::play::clientbound::world::SetBorderWarningDelay),
    "set_border_warning_distance" => SetBorderWarningDistance(crate::play::clientbound::world::SetBorderWarningDistance),
    "set_camera" => SetCamera(crate::play::clientbound::world::SetCamera),
    "set_chunk_cache_center" => SetChunkCacheCenter(crate::play::clientbound::world::SetChunkCacheCenter),
    "set_chunk_cache_radius" => SetChunkCacheRadius(crate::play::clientbound::world::SetChunkCacheRadius),
    "set_cursor_item" => SetCursorItem(crate::play::clientbound::ui::SetCursorItem),
    "set_default_spawn_position" => SetDefaultSpawnPosition(crate::play::clientbound::world::SetDefaultSpawnPosition),
    "set_display_objective" => SetDisplayObjective(crate::play::clientbound::ui::SetDisplayObjective),
    "set_entity_data" => SetEntityData(crate::play::clientbound::entity::SetEntityData),
    "set_entity_link" => SetEntityLink(crate::play::clientbound::entity::SetEntityLink),
    "set_entity_motion" => SetEntityMotion(crate::play::clientbound::entity::SetEntityMotion),
    "set_equipment" => SetEquipment(crate::play::clientbound::entity::SetEquipment),
    "set_experience" => SetExperience(crate::play::clientbound::player::SetExperience),
    "set_health" => SetHealth(crate::play::clientbound::player::SetHealth),
    "set_held_slot" => SetHeldSlot(crate::play::clientbound::player::SetHeldSlot),
    "set_objective" => SetObjective(crate::play::clientbound::ui::SetObjective),
    "set_passengers" => SetPassengers(crate::play::clientbound::entity::SetPassengers),
    "set_player_inventory" => SetPlayerInventory(crate::play::clientbound::ui::SetPlayerInventory),
    "set_player_team" => SetPlayerTeam(crate::play::clientbound::ui::SetPlayerTeam),
    "set_score" => SetScore(crate::play::clientbound::ui::SetScore),
    "set_simulation_distance" => SetSimulationDistance(crate::play::clientbound::world::SetSimulationDistance),
    "set_subtitle_text" => SetSubtitleText(crate::play::clientbound::ui::SetSubtitleText),
    "set_time" => SetTime(crate::play::clientbound::world::SetTime),
    "set_title_text" => SetTitleText(crate::play::clientbound::ui::SetTitleText),
    "set_titles_animation" => SetTitlesAnimation(crate::play::clientbound::ui::SetTitlesAnimation),
    "sound_entity" => SoundEntity(crate::play::clientbound::world::SoundEntity),
    "sound" => Sound(crate::play::clientbound::world::Sound),
    "start_configuration" => StartConfiguration(crate::play::clientbound::player::StartConfiguration),
    "stop_sound" => StopSound(crate::play::clientbound::world::StopSound),
    "store_cookie" => StoreCookie(crate::common::StoreCookie<'a>),
    "swing_animation" => SwingAnimation(crate::play::clientbound::entity::EntitySwing),
    "system_chat" => SystemChat(crate::play::clientbound::player::SystemChat),
    "tab_list" => TabList(crate::play::clientbound::ui::TabList),
    "tag_query" => TagQuery(crate::play::clientbound::ui::TagQuery),
    "take_item_entity" => TakeItemEntity(crate::play::clientbound::entity::TakeItemEntity),
    "teleport_entity" => TeleportEntity(crate::play::clientbound::entity::TeleportEntity),
    "test_instance_block_status" => TestInstanceBlockStatus(crate::play::clientbound::world::TestInstanceBlockStatus),
    "ticking_state" => TickingState(crate::play::clientbound::world::TickingState),
    "ticking_step" => TickingStep(crate::play::clientbound::world::TickingStep),
    "transfer" => Transfer(crate::common::Transfer),
    "update_advancements" => UpdateAdvancements(crate::play::clientbound::ui::UpdateAdvancements),
    "update_attributes" => UpdateAttributes(crate::play::clientbound::entity::UpdateAttributes),
    "update_mob_effect" => UpdateMobEffect(crate::play::clientbound::entity::UpdateMobEffect),
    "update_recipes" => UpdateRecipes(crate::play::clientbound::ui::UpdateRecipes),
    "update_tags" => UpdateTags(crate::common::UpdateTags),
    "projectile_power" => ProjectilePower(crate::play::clientbound::entity::ProjectilePower),
    "custom_report_details" => CustomReportDetails(crate::common::CustomReportDetails),
    "server_links" => ServerLinks(crate::common::ServerLinks),
    "waypoint" => Waypoint(crate::play::clientbound::entity::Waypoint),
    "clear_dialog" => ClearDialog(crate::common::ClearDialog),
    "show_dialog" => ShowDialog(crate::common::ShowDialog),
}

packets! {
    PlayServerbound<'a>, Play, Serverbound;
    "accept_teleportation" => AcceptTeleportation(crate::play::serverbound::AcceptTeleportation),
    "attack" => Attack(crate::play::serverbound::Attack),
    "block_entity_tag_query" => BlockEntityTagQuery(crate::play::serverbound::BlockEntityTagQuery),
    "bundle_item_selected" => BundleItemSelected(crate::play::serverbound::BundleItemSelected),
    "change_difficulty" => ChangeDifficulty(crate::play::serverbound::ChangeDifficulty),
    "change_game_mode" => ChangeGameMode(crate::play::serverbound::ChangeGameMode),
    "chat_ack" => ChatAck(crate::play::serverbound::ChatAck),
    "chat_command" => ChatCommand(crate::play::serverbound::ChatCommand),
    "chat_command_signed" => ChatCommandSigned(crate::play::serverbound::ChatCommandSigned),
    "chat" => Chat(crate::play::serverbound::Chat),
    "chat_session_update" => ChatSessionUpdate(crate::play::serverbound::ChatSessionUpdate),
    "chunk_batch_received" => ChunkBatchReceived(crate::play::serverbound::ChunkBatchReceived),
    "client_command" => ClientCommand(crate::play::serverbound::ClientCommand),
    "client_tick_end" => ClientTickEnd(crate::play::serverbound::ClientTickEnd),
    "client_information" => ClientInformation(crate::common::ClientInformation),
    "command_suggestion" => CommandSuggestion(crate::play::serverbound::CommandSuggestion),
    "configuration_acknowledged" => ConfigurationAcknowledged(crate::play::serverbound::ConfigurationAcknowledged),
    "container_button_click" => ContainerButtonClick(crate::play::serverbound::ContainerButtonClick),
    "container_click" => ContainerClick(crate::play::serverbound::ContainerClick),
    "container_close" => ContainerClose(crate::play::serverbound::ContainerClose),
    "container_slot_state_changed" => ContainerSlotStateChanged(crate::play::serverbound::ContainerSlotStateChanged),
    "cookie_response" => CookieResponse(crate::common::CookieResponse<'a>),
    "custom_payload" => CustomPayload(crate::common::ServerboundCustomPayload<'a>),
    "debug_subscription_request" => DebugSubscriptionRequest(crate::play::serverbound::DebugSubscriptionRequest),
    "edit_book" => EditBook(crate::play::serverbound::EditBook),
    "entity_tag_query" => EntityTagQuery(crate::play::serverbound::EntityTagQuery),
    "interact" => Interact(crate::play::serverbound::Interact),
    "jigsaw_generate" => JigsawGenerate(crate::play::serverbound::JigsawGenerate),
    "keep_alive" => KeepAlive(crate::common::ServerboundKeepAlive),
    "lock_difficulty" => LockDifficulty(crate::play::serverbound::LockDifficulty),
    "move_player_pos" => MovePlayerPos(crate::play::serverbound::MovePlayerPos),
    "move_player_pos_rot" => MovePlayerPosRot(crate::play::serverbound::MovePlayerPosRot),
    "move_player_rot" => MovePlayerRot(crate::play::serverbound::MovePlayerRot),
    "move_player_status_only" => MovePlayerStatusOnly(crate::play::serverbound::MovePlayerStatusOnly),
    "move_vehicle" => MoveVehicle(crate::play::serverbound::MoveVehicle),
    "paddle_boat" => PaddleBoat(crate::play::serverbound::PaddleBoat),
    "pick_item_from_block" => PickItemFromBlock(crate::play::serverbound::PickItemFromBlock),
    "pick_item_from_entity" => PickItemFromEntity(crate::play::serverbound::PickItemFromEntity),
    "ping_request" => PingRequest(crate::status::PingRequest),
    "place_recipe" => PlaceRecipe(crate::play::serverbound::PlaceRecipe),
    "player_abilities" => PlayerAbilities(crate::play::serverbound::PlayerAbilities),
    "player_action" => PlayerAction(crate::play::serverbound::PlayerAction),
    "player_command" => PlayerCommand(crate::play::serverbound::PlayerCommand),
    "player_input" => PlayerInput(crate::play::serverbound::PlayerInput),
    "player_loaded" => PlayerLoaded(crate::play::serverbound::PlayerLoaded),
    "pong" => Pong(crate::common::Pong),
    "punch" => Punch(crate::play::serverbound::Punch),
    "recipe_book_change_settings" => RecipeBookChangeSettings(crate::play::serverbound::RecipeBookChangeSettings),
    "recipe_book_seen_recipe" => RecipeBookSeenRecipe(crate::play::serverbound::RecipeBookSeenRecipe),
    "rename_item" => RenameItem(crate::play::serverbound::RenameItem),
    "resource_pack" => ResourcePack(crate::common::ResourcePackResponse),
    "seen_advancements" => SeenAdvancements(crate::play::serverbound::SeenAdvancements),
    "select_trade" => SelectTrade(crate::play::serverbound::SelectTrade),
    "set_beacon" => SetBeacon(crate::play::serverbound::SetBeacon),
    "set_carried_item" => SetCarriedItem(crate::play::serverbound::SetCarriedItem),
    "set_command_block" => SetCommandBlock(crate::play::serverbound::SetCommandBlock),
    "set_command_minecart" => SetCommandMinecart(crate::play::serverbound::SetCommandMinecart),
    "set_creative_mode_slot" => SetCreativeModeSlot(crate::play::serverbound::SetCreativeModeSlot),
    "set_game_rule" => SetGameRule(crate::play::serverbound::SetGameRule),
    "set_jigsaw_block" => SetJigsawBlock(crate::play::serverbound::SetJigsawBlock),
    "set_structure_block" => SetStructureBlock(crate::play::serverbound::SetStructureBlock),
    "set_test_block" => SetTestBlock(crate::play::serverbound::SetTestBlock),
    "sign_update" => SignUpdate(crate::play::serverbound::SignUpdate),
    "spectator_action" => SpectatorAction(crate::play::serverbound::SpectatorAction),
    "teleport_to_entity" => TeleportToEntity(crate::play::serverbound::TeleportToEntity),
    "test_instance_block_action" => TestInstanceBlockAction(crate::play::serverbound::TestInstanceBlockAction),
    "use_item_on" => UseItemOn(crate::play::serverbound::UseItemOn),
    "use_item" => UseItem(crate::play::serverbound::UseItem),
    "custom_click_action" => CustomClickAction(crate::common::CustomClickAction),
}
