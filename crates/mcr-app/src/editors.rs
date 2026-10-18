//! Builds the editor screens (signs, books, operator blocks) from what the client knows.

use mcr_block_entity::DyeColor;
use mcr_buf::BlockPos;
use mcr_client::{BlockEditor, ClientState};
use mcr_nbt::{Compound, Limits, Value, read_network};
use mcr_protocol::play::serverbound::{
    CommandBlockMode, JointType, Mirror, Rotation, StructureMode, TestBlockMode,
};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::DataComponent;
use mcr_protocol::types::game::{Hand, SignSide};
use mcr_protocol::types::text::Component;
use mcr_ui::{
    BookEditScreen, BookViewScreen, CommandBlockInfo, CommandBlockScreen, JigsawBlockScreen,
    JigsawInfo, MinecartCommandInfo, Screen, SharedTestStatus, SignEditScreen, SignInfo,
    StructureBlockScreen, StructureInfo, TestBlockScreen, TestInstanceBlockScreen,
    TestInstanceInfo,
};
use mcr_world::ChunkPos;

fn block_name_at(state: &ClientState, pos: BlockPos) -> Option<(u32, &'static str)> {
    let id = state.chunks.block(pos.x, pos.y, pos.z)?.0;
    let name = mcr_data::v26_3().state_block_name(id)?;
    Some((id, name.strip_prefix("minecraft:").unwrap_or(name)))
}

fn property(state_id: u32, key: &str) -> Option<String> {
    mcr_data::v26_3()
        .state_properties(state_id)
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_owned())
}

/// The block entity data the client holds for a position: the latest update packet, else the chunk's copy.
pub fn block_entity_tag(state: &ClientState, pos: BlockPos) -> Option<Compound> {
    if let Some((_, tag)) = state.block_entities.get(&pos) {
        return Some(tag.clone());
    }
    let column = state.chunks.column(ChunkPos::new(pos.x >> 4, pos.z >> 4))?;
    let entry = column.block_entities().iter().find(|be| {
        be.x() == (pos.x & 15) as usize
            && be.z() == (pos.z & 15) as usize
            && i32::from(be.y) == pos.y
    })?;
    match read_network(&mut &entry.nbt[..], Limits::NETWORK) {
        Ok(Some(Value::Compound(c))) => Some(c),
        _ => None,
    }
}

fn component_text(value: &Value) -> String {
    Component::from_nbt_lenient(value).to_plain_string()
}

fn wood_of(block: &str) -> String {
    for suffix in ["_wall_hanging_sign", "_hanging_sign", "_wall_sign", "_sign"] {
        if let Some(wood) = block.strip_suffix(suffix) {
            return wood.to_owned();
        }
    }
    "oak".into()
}

fn scale_rgb(rgb: u32, factor: f32) -> u32 {
    let ch = |shift: u32| (((rgb >> shift) & 0xFF) as f32 * factor) as u32;
    0xFF00_0000 | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

pub fn sign_screen(state: &ClientState, pos: BlockPos, side: SignSide) -> SignEditScreen {
    let block = block_name_at(state, pos).map_or("oak_sign", |(_, n)| n);
    let tag = block_entity_tag(state, pos).unwrap_or_default();
    let text = tag
        .get_compound(match side {
            SignSide::Front => "front_text",
            SignSide::Back => "back_text",
        })
        .cloned()
        .unwrap_or_default();
    let mut lines: [String; 4] = Default::default();
    if let Some(list) = text.get_list("messages") {
        let values: Vec<Value> = match list {
            mcr_nbt::List::String(v) => v.iter().cloned().map(Value::String).collect(),
            mcr_nbt::List::Compound(v) => v.iter().cloned().map(Value::Compound).collect(),
            mcr_nbt::List::List(v) => v.iter().cloned().map(Value::List).collect(),
            _ => Vec::new(),
        };
        for (slot, value) in values.iter().take(4).enumerate() {
            lines[slot] = component_text(value);
        }
    }
    let color = text
        .get_str("color")
        .and_then(DyeColor::from_name)
        .unwrap_or(DyeColor::BLACK);
    let glowing = text.get_bool("has_glowing_text").unwrap_or(false);
    let rgb = color.text_rgb();
    let text_color = if glowing {
        0xFF00_0000 | rgb
    } else {
        scale_rgb(rgb, 0.4)
    };
    SignEditScreen::new(SignInfo {
        wood: wood_of(block),
        hanging: block.ends_with("hanging_sign"),
        wall: block.contains("wall"),
        lines,
        text_color,
    })
}

fn enum_name(tag: &Compound, key: &str) -> String {
    tag.get_str(key).unwrap_or("").to_ascii_lowercase()
}

fn rotation_of(name: &str) -> Rotation {
    match name {
        "clockwise_90" => Rotation::Clockwise90,
        "clockwise_180" => Rotation::Clockwise180,
        "counterclockwise_90" => Rotation::CounterClockwise90,
        _ => Rotation::None,
    }
}

fn component_string(value: Option<&Value>) -> String {
    value.map(component_text).unwrap_or_default()
}

pub fn command_block_screen(state: &ClientState, pos: BlockPos) -> Option<CommandBlockScreen> {
    let (id, name) = block_name_at(state, pos)?;
    let tag = block_entity_tag(state, pos).unwrap_or_default();
    Some(CommandBlockScreen::block(CommandBlockInfo {
        pos,
        command: tag.get_str("Command").unwrap_or("").to_owned(),
        mode: match name {
            "repeating_command_block" => CommandBlockMode::Auto,
            "chain_command_block" => CommandBlockMode::Sequence,
            _ => CommandBlockMode::Redstone,
        },
        conditional: property(id, "conditional").as_deref() == Some("true"),
        automatic: tag.get_bool("auto").unwrap_or(false),
        track_output: tag.get_bool("TrackOutput").unwrap_or(true),
        last_output: component_string(tag.get("LastOutput")),
    }))
}

pub fn minecart_screen(state: &ClientState, entity_id: i32) -> Option<CommandBlockScreen> {
    use mcr_protocol::types::entity_data::EntityDataValue;
    let entity = state.entities.get(entity_id)?;
    let mut command = String::new();
    let mut last_output = String::new();
    for value in entity.data.values() {
        match value {
            EntityDataValue::String(s) => command.clone_from(s),
            EntityDataValue::Component(c) => last_output = c.to_plain_string(),
            _ => {}
        }
    }
    Some(CommandBlockScreen::minecart(MinecartCommandInfo {
        entity_id,
        command,
        track_output: true,
        last_output,
    }))
}

pub fn structure_screen(state: &ClientState, pos: BlockPos) -> StructureBlockScreen {
    let tag = block_entity_tag(state, pos).unwrap_or_default();
    let int =
        |key: &str, default: i32, min: i32| tag.get_i32(key).unwrap_or(default).clamp(min, 48);
    StructureBlockScreen::new(StructureInfo {
        pos,
        name: tag.get_str("name").unwrap_or("").to_owned(),
        mode: match enum_name(&tag, "mode").as_str() {
            "save" => StructureMode::Save,
            "load" => StructureMode::Load,
            "corner" => StructureMode::Corner,
            _ => StructureMode::Data,
        },
        offset: [
            int("posX", 0, -48),
            int("posY", 1, -48),
            int("posZ", 0, -48),
        ],
        size: [int("sizeX", 0, 0), int("sizeY", 0, 0), int("sizeZ", 0, 0)],
        mirror: match enum_name(&tag, "mirror").as_str() {
            "left_right" => Mirror::LeftRight,
            "front_back" => Mirror::FrontBack,
            _ => Mirror::None,
        },
        rotation: rotation_of(&enum_name(&tag, "rotation")),
        data: tag.get_str("metadata").unwrap_or("").to_owned(),
        ignore_entities: tag.get_bool("ignoreEntities").unwrap_or(true),
        strict: tag.get_bool("strict").unwrap_or(false),
        show_air: tag.get_bool("showair").unwrap_or(false),
        show_bounding_box: tag.get_bool("showboundingbox").unwrap_or(true),
        integrity: tag.get_f32("integrity").unwrap_or(1.0).clamp(0.0, 1.0),
        seed: tag.get_i64("seed").unwrap_or(0),
    })
}

pub fn jigsaw_screen(state: &ClientState, pos: BlockPos) -> JigsawBlockScreen {
    let tag = block_entity_tag(state, pos).unwrap_or_default();
    let vertical = block_name_at(state, pos)
        .and_then(|(id, _)| property(id, "orientation"))
        .is_some_and(|o| o.starts_with("up_") || o.starts_with("down_"));
    let text = |key: &str, default: &str| tag.get_str(key).unwrap_or(default).to_owned();
    JigsawBlockScreen::new(JigsawInfo {
        pos,
        name: text("name", "minecraft:empty"),
        target: text("target", "minecraft:empty"),
        pool: text("pool", "minecraft:empty"),
        final_state: text("final_state", "minecraft:air"),
        joint: if enum_name(&tag, "joint") == "rollable" {
            JointType::Rollable
        } else {
            JointType::Aligned
        },
        selection_priority: tag.get_i32("selection_priority").unwrap_or(0),
        placement_priority: tag.get_i32("placement_priority").unwrap_or(0),
        vertical,
    })
}

pub fn test_block_screen(state: &ClientState, pos: BlockPos) -> TestBlockScreen {
    let tag = block_entity_tag(state, pos).unwrap_or_default();
    let mode = match enum_name(&tag, "mode").as_str() {
        "log" => TestBlockMode::Log,
        "fail" => TestBlockMode::Fail,
        "accept" => TestBlockMode::Accept,
        _ => TestBlockMode::Start,
    };
    TestBlockScreen::new(pos, mode, tag.get_str("message").unwrap_or(""))
}

pub fn test_instance_screen(
    state: &ClientState,
    pos: BlockPos,
    status: SharedTestStatus,
) -> TestInstanceBlockScreen {
    let data = block_entity_tag(state, pos)
        .and_then(|t| t.get_compound("data").cloned())
        .unwrap_or_default();
    let size = match data.get("size") {
        Some(Value::IntArray(v)) if v.len() == 3 => [v[0], v[1], v[2]],
        _ => [1, 1, 1],
    };
    TestInstanceBlockScreen::new(
        TestInstanceInfo {
            pos,
            test: data.get_str("test").unwrap_or("").to_owned(),
            size,
            rotation: rotation_of(&enum_name(&data, "rotation")),
            ignore_entities: data.get_bool("ignore_entities").unwrap_or(false),
            error: data.get("error_message").map(Component::from_nbt_lenient),
        },
        status,
    )
}

pub fn block_editor_screen(
    state: &ClientState,
    pos: BlockPos,
    kind: BlockEditor,
    status: &SharedTestStatus,
) -> Option<Box<dyn Screen>> {
    Some(match kind {
        BlockEditor::CommandBlock => Box::new(command_block_screen(state, pos)?),
        BlockEditor::StructureBlock => Box::new(structure_screen(state, pos)),
        BlockEditor::Jigsaw => Box::new(jigsaw_screen(state, pos)),
        BlockEditor::TestBlock => Box::new(test_block_screen(state, pos)),
        BlockEditor::TestInstanceBlock => {
            Box::new(test_instance_screen(state, pos, status.clone()))
        }
    })
}

/// Page texts of a held book; written books carry components, writable ones plain strings.
pub fn book_pages(state: &ClientState, hand: Hand) -> Option<(bool, Vec<Component>)> {
    pages_of_stack(&state.hand_stack(hand))
}

pub fn pages_of_stack(stack: &ItemStack) -> Option<(bool, Vec<Component>)> {
    let name = u32::try_from(stack.item)
        .ok()
        .and_then(|i| mcr_data::v26_3().item_name(i))?;
    let writable = match name.strip_prefix("minecraft:").unwrap_or(name) {
        "writable_book" => true,
        "written_book" => false,
        _ => return None,
    };
    let mut pages = Vec::new();
    for component in &stack.components.added {
        match component {
            DataComponent::WrittenBookContent(book) => {
                pages = book.pages.iter().take(100).map(|p| p.raw.clone()).collect();
            }
            DataComponent::WritableBookContent(list) => {
                pages = list
                    .0
                    .iter()
                    .take(100)
                    .map(|p| Component::text(p.raw.0.clone()))
                    .collect();
            }
            _ => {}
        }
    }
    Some((writable, pages))
}

pub fn book_edit_screen(state: &ClientState, hand: Hand, author: &str) -> BookEditScreen {
    let pages = match book_pages(state, hand) {
        Some((true, pages)) => pages.iter().map(Component::to_plain_string).collect(),
        _ => Vec::new(),
    };
    BookEditScreen::new(author, pages)
}

pub fn book_view_screen(state: &ClientState, hand: Hand) -> Option<BookViewScreen> {
    let (_, pages) = book_pages(state, hand)?;
    Some(BookViewScreen::new(pages))
}

fn link_key(kind: mcr_protocol::common::KnownLinkType) -> &'static str {
    use mcr_protocol::common::KnownLinkType as K;
    match kind {
        K::BugReport => "bug_report",
        K::CommunityGuidelines => "community_guidelines",
        K::Support => "support",
        K::Status => "status",
        K::Feedback => "feedback",
        K::Community => "community",
        K::Website => "website",
        K::Forums => "forums",
        K::News => "news",
        K::Announcements => "announcements",
    }
}

/// Labels and URLs of the links the server advertised, for server link dialogs.
pub fn server_links(state: &ClientState) -> Vec<(Component, String)> {
    use mcr_protocol::common::ServerLinkLabel;
    state
        .server
        .links
        .iter()
        .map(|link| {
            let label = match &link.label {
                ServerLinkLabel::Custom(c) => c.clone(),
                ServerLinkLabel::Known(k) => Component::translatable(
                    format!("known_server_link.{}", link_key(*k)),
                    Vec::new(),
                ),
            };
            (label, link.url.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wood_comes_from_the_block_name() {
        assert_eq!(wood_of("oak_sign"), "oak");
        assert_eq!(wood_of("dark_oak_wall_sign"), "dark_oak");
        assert_eq!(wood_of("pale_oak_hanging_sign"), "pale_oak");
        assert_eq!(wood_of("bamboo_wall_hanging_sign"), "bamboo");
    }

    #[test]
    fn unlit_sign_text_is_darkened_and_opaque() {
        assert_eq!(scale_rgb(0xFFFFFF, 0.4), 0xFF66_6666);
        assert_eq!(scale_rgb(0, 0.4), 0xFF00_0000);
    }

    #[test]
    fn books_are_recognized_by_item_without_components() {
        let data = mcr_data::v26_3();
        let item = |name: &str| ItemStack::new(data.item_by_name(name).unwrap() as i32, 1);
        let (writable, pages) = pages_of_stack(&item("minecraft:writable_book")).unwrap();
        assert!(writable && pages.is_empty());
        let (writable, _) = pages_of_stack(&item("minecraft:written_book")).unwrap();
        assert!(!writable);
        assert!(pages_of_stack(&item("minecraft:stick")).is_none());
    }
}
