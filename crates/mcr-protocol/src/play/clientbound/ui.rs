mod advancement;
mod commands;
mod common;
mod recipe;

use mcr_buf::{BlockPos, Decode, DecodeError, Encode, Identifier, Result, read_len, take};
use mcr_nbt::{NetworkNbt, OptionalNetworkNbt};
use uuid::Uuid;

pub use self::{
    advancement::{
        Advancement, AdvancementFrame, AdvancementProgress, DisplayInfo, PositionedAdvancement,
        SelectAdvancementsTab, UpdateAdvancements,
    },
    commands::{ArgumentParser, CommandNode, Commands, NodeKind, StringKind},
    common::{
        ComponentPatch, ComponentPredicate, HolderSet, ItemStackTemplate, Pairs, optional_varint,
    },
    recipe::{
        PlaceGhostRecipe, RecipeBookAdd, RecipeBookEntry, RecipeBookRemove, RecipeBookSettings,
        RecipeBookTypeSettings, RecipeDisplay, RecipeDisplayEntry, SlotDisplay, StonecutterRecipe,
        TrimPattern, UpdateRecipes,
    },
};
use crate::{
    packet,
    types::{Component, ItemStack},
};

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ContainerClose {
    #[mcr(varint)]
    pub container_id: i32,
}

packet!(ContainerClose, "container_close", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ContainerSetContent {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub state_id: i32,
    pub items: Vec<ItemStack>,
    pub carried_item: ItemStack,
}

packet!(ContainerSetContent, "container_set_content", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ContainerSetData {
    #[mcr(varint)]
    pub container_id: i32,
    pub id: i16,
    pub value: i16,
}

packet!(ContainerSetData, "container_set_data", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ContainerSetSlot {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub state_id: i32,
    pub slot: i16,
    pub item: ItemStack,
}

packet!(ContainerSetSlot, "container_set_slot", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetCursorItem {
    pub contents: ItemStack,
}

packet!(SetCursorItem, "set_cursor_item", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetPlayerInventory {
    #[mcr(varint)]
    pub slot: i32,
    pub contents: ItemStack,
}

packet!(SetPlayerInventory, "set_player_inventory", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct MountScreenOpen {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub inventory_columns: i32,
    pub entity_id: i32,
}

packet!(MountScreenOpen, "mount_screen_open", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct OpenScreen {
    #[mcr(varint)]
    pub container_id: i32,
    #[mcr(varint)]
    pub menu_type: i32,
    pub title: Component,
}

packet!(OpenScreen, "open_screen", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Hand {
    MainHand,
    OffHand,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct OpenBook {
    pub hand: Hand,
}

packet!(OpenBook, "open_book", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum SignSide {
    Back,
    Front,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct OpenSignEditor {
    pub pos: BlockPos,
    pub side: SignSide,
}

packet!(OpenSignEditor, "open_sign_editor", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ItemCost {
    #[mcr(varint)]
    pub item: i32,
    #[mcr(varint)]
    pub count: i32,
    pub components: ComponentPredicate,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MerchantOffer {
    pub cost_a: ItemCost,
    pub result: ItemStack,
    pub cost_b: Option<ItemCost>,
    pub out_of_stock: bool,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub special_price_diff: i32,
    pub price_multiplier: f32,
    pub demand: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MerchantOffers {
    #[mcr(varint)]
    pub container_id: i32,
    pub offers: Vec<MerchantOffer>,
    #[mcr(varint)]
    pub villager_level: i32,
    #[mcr(varint)]
    pub villager_xp: i32,
    pub show_progress: bool,
    pub can_restock: bool,
}

packet!(MerchantOffers, "merchant_offers", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Cooldown {
    pub cooldown_group: Identifier,
    #[mcr(varint)]
    pub duration: i32,
}

packet!(Cooldown, "cooldown", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MapDecoration {
    #[mcr(varint)]
    pub decoration_type: i32,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    pub name: Option<Component>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPatch {
    pub start_x: u8,
    pub start_y: u8,
    pub width: u8,
    pub height: u8,
    pub map_colors: Vec<u8>,
}

mod map_patch {
    use super::*;

    pub fn encode(patch: &Option<MapPatch>, out: &mut Vec<u8>) {
        match patch {
            Some(p) => {
                out.extend_from_slice(&[p.width, p.height, p.start_x, p.start_y]);
                p.map_colors.encode(out);
            }
            None => out.push(0),
        }
    }

    pub fn decode(input: &mut &[u8]) -> Result<Option<MapPatch>> {
        let width = u8::decode(input)?;
        if width == 0 {
            return Ok(None);
        }
        let [height, start_x, start_y] = take(input, 3)?.try_into().unwrap_or_default();
        let len = read_len(input, input.len())?;
        Ok(Some(MapPatch {
            start_x,
            start_y,
            width,
            height,
            map_colors: take(input, len)?.to_vec(),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MapItemData {
    #[mcr(varint)]
    pub map_id: i32,
    pub scale: i8,
    pub locked: bool,
    pub decorations: Option<Vec<MapDecoration>>,
    #[mcr(with = map_patch)]
    pub color_patch: Option<MapPatch>,
}

packet!(MapItemData, "map_item_data", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BossBarColor {
    Pink,
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BossBarOverlay {
    Progress,
    Notched6,
    Notched10,
    Notched12,
    Notched20,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BossBarFlags {
    pub darken_screen: bool,
    pub play_music: bool,
    pub create_world_fog: bool,
}

impl Encode for BossBarFlags {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(
            u8::from(self.darken_screen)
                | u8::from(self.play_music) << 1
                | u8::from(self.create_world_fog) << 2,
        );
    }
}

impl Decode<'_> for BossBarFlags {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let flags = u8::decode(input)?;
        Ok(Self {
            darken_screen: flags & 1 != 0,
            play_music: flags & 2 != 0,
            create_world_fog: flags & 4 != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum BossEventOperation {
    Add {
        name: Component,
        progress: f32,
        color: BossBarColor,
        overlay: BossBarOverlay,
        flags: BossBarFlags,
    },
    Remove,
    UpdateProgress(f32),
    UpdateName(Component),
    UpdateStyle {
        color: BossBarColor,
        overlay: BossBarOverlay,
    },
    UpdateProperties(BossBarFlags),
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BossEvent {
    pub id: Uuid,
    pub operation: BossEventOperation,
}

packet!(BossEvent, "boss_event", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClearTitles {
    pub reset_times: bool,
}

packet!(ClearTitles, "clear_titles", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetTitleText {
    pub text: Component,
}

packet!(SetTitleText, "set_title_text", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetSubtitleText {
    pub text: Component,
}

packet!(SetSubtitleText, "set_subtitle_text", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetActionBarText {
    pub text: Component,
}

packet!(SetActionBarText, "set_action_bar_text", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetTitlesAnimation {
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
}

packet!(SetTitlesAnimation, "set_titles_animation", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TabList {
    pub header: Component,
    pub footer: Component,
}

packet!(TabList, "tab_list", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TagQuery {
    #[mcr(varint)]
    pub transaction_id: i32,
    pub tag: OptionalNetworkNbt,
}

packet!(TagQuery, "tag_query", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Suggestion {
    pub text: String,
    pub tooltip: Option<Component>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct CommandSuggestions {
    #[mcr(varint)]
    pub id: i32,
    #[mcr(varint)]
    pub start: i32,
    #[mcr(varint)]
    pub length: i32,
    pub suggestions: Vec<Suggestion>,
}

packet!(CommandSuggestions, "command_suggestions", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum DisplaySlot {
    List,
    Sidebar,
    BelowName,
    TeamBlack,
    TeamDarkBlue,
    TeamDarkGreen,
    TeamDarkAqua,
    TeamDarkRed,
    TeamDarkPurple,
    TeamGold,
    TeamGray,
    TeamDarkGray,
    TeamBlue,
    TeamGreen,
    TeamAqua,
    TeamRed,
    TeamLightPurple,
    TeamYellow,
    TeamWhite,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SetDisplayObjective {
    pub slot: DisplaySlot,
    pub objective_name: String,
}

packet!(SetDisplayObjective, "set_display_objective", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum NumberFormat {
    Blank,
    Styled(NetworkNbt),
    Fixed(Component),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ObjectiveRenderType {
    Integer,
    Hearts,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ObjectiveDisplay {
    pub display_name: Component,
    pub render_type: ObjectiveRenderType,
    pub number_format: Option<NumberFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectiveAction {
    Add(ObjectiveDisplay),
    Remove,
    Change(ObjectiveDisplay),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetObjective {
    pub objective_name: String,
    pub action: ObjectiveAction,
}

packet!(SetObjective, "set_objective", Clientbound);

impl Encode for SetObjective {
    fn encode(&self, out: &mut Vec<u8>) {
        self.objective_name.encode(out);
        match &self.action {
            ObjectiveAction::Add(display) => {
                0i8.encode(out);
                display.encode(out);
            }
            ObjectiveAction::Remove => 1i8.encode(out),
            ObjectiveAction::Change(display) => {
                2i8.encode(out);
                display.encode(out);
            }
        }
    }
}

impl Decode<'_> for SetObjective {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let objective_name = String::decode(input)?;
        let action = match i8::decode(input)? {
            0 => ObjectiveAction::Add(ObjectiveDisplay::decode(input)?),
            1 => ObjectiveAction::Remove,
            2 => ObjectiveAction::Change(ObjectiveDisplay::decode(input)?),
            method => {
                return Err(DecodeError::InvalidValue {
                    what: "objective method",
                    value: method.into(),
                });
            }
        };
        Ok(Self {
            objective_name,
            action,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SetScore {
    pub owner: String,
    pub objective_name: String,
    #[mcr(varint)]
    pub score: i32,
    pub display: Option<Component>,
    pub number_format: Option<NumberFormat>,
}

packet!(SetScore, "set_score", Clientbound);

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResetScore {
    pub owner: String,
    pub objective_name: Option<String>,
}

packet!(ResetScore, "reset_score", Clientbound);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum NameTagVisibility {
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum CollisionRule {
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum TeamColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TeamParameters {
    pub display_name: Component,
    pub player_prefix: Component,
    pub player_suffix: Component,
    pub name_tag_visibility: NameTagVisibility,
    pub collision_rule: CollisionRule,
    pub color: Option<TeamColor>,
    pub options: i8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TeamAction {
    Add {
        parameters: TeamParameters,
        players: Vec<String>,
    },
    Remove,
    Change(TeamParameters),
    Join(Vec<String>),
    Leave(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetPlayerTeam {
    pub name: String,
    pub action: TeamAction,
}

packet!(SetPlayerTeam, "set_player_team", Clientbound);

impl Encode for SetPlayerTeam {
    fn encode(&self, out: &mut Vec<u8>) {
        self.name.encode(out);
        match &self.action {
            TeamAction::Add {
                parameters,
                players,
            } => {
                0i8.encode(out);
                parameters.encode(out);
                players.encode(out);
            }
            TeamAction::Remove => 1i8.encode(out),
            TeamAction::Change(parameters) => {
                2i8.encode(out);
                parameters.encode(out);
            }
            TeamAction::Join(players) => {
                3i8.encode(out);
                players.encode(out);
            }
            TeamAction::Leave(players) => {
                4i8.encode(out);
                players.encode(out);
            }
        }
    }
}

impl Decode<'_> for SetPlayerTeam {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let name = String::decode(input)?;
        let action = match i8::decode(input)? {
            0 => TeamAction::Add {
                parameters: TeamParameters::decode(input)?,
                players: Vec::decode(input)?,
            },
            1 => TeamAction::Remove,
            2 => TeamAction::Change(TeamParameters::decode(input)?),
            3 => TeamAction::Join(Vec::decode(input)?),
            4 => TeamAction::Leave(Vec::decode(input)?),
            method => {
                return Err(DecodeError::InvalidValue {
                    what: "team method",
                    value: method.into(),
                });
            }
        };
        Ok(Self { name, action })
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::VarInt;

    use super::*;
    use crate::test_util::roundtrip;

    fn uuid() -> Uuid {
        Uuid::from_u128(1)
    }

    fn uuid_bytes() -> Vec<u8> {
        let mut bytes = vec![0; 15];
        bytes.push(1);
        bytes
    }

    #[test]
    fn container_small_packets() {
        assert_eq!(roundtrip(&ContainerClose { container_id: 3 }), [3]);
        assert_eq!(
            roundtrip(&ContainerSetData {
                container_id: 1,
                id: 2,
                value: -1,
            }),
            [1, 0, 2, 0xff, 0xff]
        );
        assert_eq!(
            roundtrip(&MountScreenOpen {
                container_id: 4,
                inventory_columns: 5,
                entity_id: 0x0102_0304,
            }),
            [4, 5, 1, 2, 3, 4]
        );
        assert_eq!(
            roundtrip(&OpenSignEditor {
                pos: BlockPos::new(0, 0, 1),
                side: SignSide::Front,
            }),
            [0, 0, 0, 0, 0, 0, 0x10, 0, 1]
        );
        assert_eq!(
            roundtrip(&OpenBook {
                hand: Hand::OffHand
            }),
            [1]
        );
        assert_eq!(
            roundtrip(&Cooldown {
                cooldown_group: Identifier::parse("a:b").unwrap(),
                duration: 20,
            }),
            [3, b'a', b':', b'b', 20]
        );
    }

    #[test]
    fn titles() {
        assert_eq!(roundtrip(&ClearTitles { reset_times: true }), [1]);
        assert_eq!(
            roundtrip(&SetTitlesAnimation {
                fade_in: 10,
                stay: 70,
                fade_out: 20,
            }),
            [0, 0, 0, 10, 0, 0, 0, 70, 0, 0, 0, 20]
        );
    }

    #[test]
    fn boss_event_operations() {
        let mut expected = uuid_bytes();
        expected.extend_from_slice(&[1]);
        assert_eq!(
            roundtrip(&BossEvent {
                id: uuid(),
                operation: BossEventOperation::Remove,
            }),
            expected
        );
        let mut expected = uuid_bytes();
        expected.extend_from_slice(&[2, 0x3f, 0x80, 0, 0]);
        assert_eq!(
            roundtrip(&BossEvent {
                id: uuid(),
                operation: BossEventOperation::UpdateProgress(1.0),
            }),
            expected
        );
        let mut expected = uuid_bytes();
        expected.extend_from_slice(&[4, 6, 4]);
        assert_eq!(
            roundtrip(&BossEvent {
                id: uuid(),
                operation: BossEventOperation::UpdateStyle {
                    color: BossBarColor::White,
                    overlay: BossBarOverlay::Notched20,
                },
            }),
            expected
        );
        let mut expected = uuid_bytes();
        expected.extend_from_slice(&[5, 5]);
        assert_eq!(
            roundtrip(&BossEvent {
                id: uuid(),
                operation: BossEventOperation::UpdateProperties(BossBarFlags {
                    darken_screen: true,
                    play_music: false,
                    create_world_fog: true,
                }),
            }),
            expected
        );
    }

    #[test]
    fn map_data() {
        let packet = MapItemData {
            map_id: 9,
            scale: 2,
            locked: true,
            decorations: Some(vec![MapDecoration {
                decoration_type: 3,
                x: -1,
                y: 1,
                rot: 8,
                name: None,
            }]),
            color_patch: Some(MapPatch {
                start_x: 4,
                start_y: 5,
                width: 2,
                height: 1,
                map_colors: vec![7, 8],
            }),
        };
        #[rustfmt::skip]
        assert_eq!(roundtrip(&packet), [
            9, 2, 1,
            1, 1, 3, 0xff, 1, 8, 0,
            2, 1, 4, 5, 2, 7, 8,
        ]);
        let empty = MapItemData {
            map_id: 0,
            scale: 0,
            locked: false,
            decorations: None,
            color_patch: None,
        };
        assert_eq!(roundtrip(&empty), [0, 0, 0, 0, 0]);
    }

    #[test]
    fn objectives_and_scores() {
        let remove = SetObjective {
            objective_name: "o".into(),
            action: ObjectiveAction::Remove,
        };
        assert_eq!(roundtrip(&remove), [1, b'o', 1]);
        assert!(SetObjective::decode(&mut &[1u8, b'o', 9][..]).is_err());
        let score = SetScore {
            owner: "a".into(),
            objective_name: "o".into(),
            score: 300,
            display: None,
            number_format: Some(NumberFormat::Blank),
        };
        assert_eq!(roundtrip(&score), [1, b'a', 1, b'o', 0xac, 2, 0, 1, 0]);
        assert_eq!(
            roundtrip(&ResetScore {
                owner: "a".into(),
                objective_name: None,
            }),
            [1, b'a', 0]
        );
        assert_eq!(
            roundtrip(&SetDisplayObjective {
                slot: DisplaySlot::TeamWhite,
                objective_name: "o".into(),
            }),
            [18, 1, b'o']
        );
    }

    #[test]
    fn team_membership() {
        let join = SetPlayerTeam {
            name: "t".into(),
            action: TeamAction::Join(vec!["p".into()]),
        };
        assert_eq!(roundtrip(&join), [1, b't', 3, 1, 1, b'p']);
        let remove = SetPlayerTeam {
            name: "t".into(),
            action: TeamAction::Remove,
        };
        assert_eq!(roundtrip(&remove), [1, b't', 1]);
        roundtrip(&SetPlayerTeam {
            name: "t".into(),
            action: TeamAction::Leave(vec![]),
        });
        assert!(SetPlayerTeam::decode(&mut &[1u8, b't', 5][..]).is_err());
    }

    #[test]
    fn suggestions_and_tag_query() {
        let packet = CommandSuggestions {
            id: 1,
            start: 2,
            length: 3,
            suggestions: vec![Suggestion {
                text: "me".into(),
                tooltip: None,
            }],
        };
        assert_eq!(roundtrip(&packet), [1, 2, 3, 1, 2, b'm', b'e', 0]);
        assert_eq!(
            roundtrip(&TagQuery {
                transaction_id: 5,
                tag: OptionalNetworkNbt(None),
            }),
            [5, 0]
        );
    }

    #[test]
    fn recipe_ids_are_varints() {
        let remove = RecipeBookRemove {
            recipes: vec![VarInt(1)],
        };
        roundtrip(&remove);
    }

    macro_rules! needs_placeholder {
        ($($name:ident: $reason:literal => $value:expr;)*) => {$(
            #[test]
            #[ignore = $reason]
            fn $name() {
                roundtrip(&$value);
            }
        )*};
    }

    needs_placeholder! {
        container_set_content: "needs item stacks" => ContainerSetContent {
            container_id: 1,
            state_id: 2,
            items: vec![ItemStack::default()],
            carried_item: ItemStack::default(),
        };
        container_set_slot: "needs item stacks" => ContainerSetSlot {
            container_id: 1,
            state_id: 2,
            slot: 3,
            item: ItemStack::default(),
        };
        set_cursor_item: "needs item stacks" => SetCursorItem { contents: ItemStack::default() };
        set_player_inventory: "needs item stacks" => SetPlayerInventory {
            slot: 1,
            contents: ItemStack::default(),
        };
        open_screen: "needs text components" => OpenScreen {
            container_id: 1,
            menu_type: 2,
            title: Component::default(),
        };
        merchant_offers: "needs item stacks and data component predicates" => MerchantOffers {
            container_id: 1,
            offers: vec![MerchantOffer {
                cost_a: ItemCost {
                    item: 1,
                    count: 2,
                    components: ComponentPredicate::default(),
                },
                result: ItemStack::default(),
                cost_b: None,
                out_of_stock: false,
                uses: 1,
                max_uses: 2,
                xp: 3,
                special_price_diff: 4,
                price_multiplier: 0.05,
                demand: 5,
            }],
            villager_level: 2,
            villager_xp: 10,
            show_progress: true,
            can_restock: true,
        };
        boss_event_add: "needs text components" => BossEvent {
            id: Uuid::from_u128(1),
            operation: BossEventOperation::Add {
                name: Component::default(),
                progress: 0.5,
                color: BossBarColor::Red,
                overlay: BossBarOverlay::Progress,
                flags: BossBarFlags::default(),
            },
        };
        text_packets: "needs text components" => TabList {
            header: Component::default(),
            footer: Component::default(),
        };
        title_text: "needs text components" => SetTitleText { text: Component::default() };
        subtitle_text: "needs text components" => SetSubtitleText { text: Component::default() };
        action_bar_text: "needs text components" => SetActionBarText { text: Component::default() };
        set_objective_add: "needs text components" => SetObjective {
            objective_name: "o".into(),
            action: ObjectiveAction::Add(ObjectiveDisplay {
                display_name: Component::default(),
                render_type: ObjectiveRenderType::Hearts,
                number_format: Some(NumberFormat::Fixed(Component::default())),
            }),
        };
        team_add: "needs text components" => SetPlayerTeam {
            name: "t".into(),
            action: TeamAction::Add {
                parameters: TeamParameters {
                    display_name: Component::default(),
                    player_prefix: Component::default(),
                    player_suffix: Component::default(),
                    name_tag_visibility: NameTagVisibility::Never,
                    collision_rule: CollisionRule::PushOwnTeam,
                    color: Some(TeamColor::Gold),
                    options: 3,
                },
                players: vec!["p".into()],
            },
        };
    }
}
