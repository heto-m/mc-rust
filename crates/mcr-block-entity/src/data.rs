use base64::Engine as _;
use mcr_nbt::{Compound, List, Value};
use mcr_protocol::types::text::Component;
use uuid::Uuid;

use crate::kind::BlockEntityKind;

pub const MAX_PATTERNS: usize = 16;
pub const MAX_PROFILE_NAME: usize = 16;
pub const MAX_ID_LEN: usize = 96;
pub const MAX_SIGN_CHARS: usize = 384;
pub const MAX_TEXTURE_VALUE: usize = 8192;
pub const MAX_URL: usize = 256;
pub const MAX_ITEM_SLOTS: usize = 8;
pub const MAX_BEAM_SECTIONS: usize = 64;
pub const MAX_BEAM_HEIGHT: i32 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DyeColor(pub u8);

const NAMES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];
const TEXTURE_DIFFUSE: [u32; 16] = [
    16383998, 16351261, 13061821, 3847130, 16701501, 8439583, 15961002, 4673362, 10329495, 1481884,
    8991416, 3949738, 8606770, 6192150, 11546150, 1908001,
];
const TEXT_COLOR: [u32; 16] = [
    16777215, 16738335, 16711935, 10141901, 16776960, 12582656, 16738740, 8421504, 13882323, 65535,
    10494192, 255, 9127187, 65280, 16711680, 0,
];

impl DyeColor {
    pub const WHITE: DyeColor = DyeColor(0);
    pub const BLACK: DyeColor = DyeColor(15);

    pub fn from_name(name: &str) -> Option<Self> {
        NAMES
            .iter()
            .position(|n| *n == name.strip_prefix("minecraft:").unwrap_or(name))
            .map(|i| DyeColor(i as u8))
    }

    pub fn from_id(id: i32) -> Self {
        DyeColor(id.clamp(0, 15) as u8)
    }

    pub fn name(self) -> &'static str {
        NAMES[usize::from(self.0 & 15)]
    }

    /// RGB multiplied into banner and bed style textures.
    pub fn diffuse_rgb(self) -> u32 {
        TEXTURE_DIFFUSE[usize::from(self.0 & 15)]
    }

    /// RGB of sign text drawn in this dye.
    pub fn text_rgb(self) -> u32 {
        TEXT_COLOR[usize::from(self.0 & 15)]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignSide {
    pub lines: [Component; 4],
    pub color: DyeColor,
    pub glowing: bool,
}

impl Default for SignSide {
    fn default() -> Self {
        SignSide {
            lines: Default::default(),
            color: DyeColor::BLACK,
            glowing: false,
        }
    }
}

impl SignSide {
    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|c| c.to_plain_string().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SignData {
    pub front: SignSide,
    pub back: SignSide,
    pub waxed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BannerLayer {
    pub pattern: String,
    pub color: DyeColor,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BannerData {
    pub layers: Vec<BannerLayer>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkullProfile {
    pub name: Option<String>,
    pub id: Option<Uuid>,
    pub skin_url: Option<String>,
    pub slim: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkullData {
    pub profile: Option<SkullProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRef {
    pub id: String,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PotData {
    pub front: Option<String>,
    pub back: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpawnerData {
    pub entity_id: Option<String>,
    pub delay: i16,
    /// `RequiredPlayerRange`: the distance within which the cage animates and smokes.
    pub range: i16,
    /// Trial spawner `next_mob_spawns_at`, synced only while the spawner is active.
    pub next_mob_spawns_at: i64,
}

pub const MAX_VAULT_CONNECTIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VaultData {
    pub display_item: Option<ItemRef>,
    /// Players linked to the vault's keyhole, at most [`MAX_VAULT_CONNECTIONS`].
    pub connected: Vec<Uuid>,
    /// Tenths of a block: linked players within this distance get connection particles.
    pub connection_range_tenths: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CampfireData {
    pub items: [Option<ItemRef>; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrushableData {
    pub item: Option<ItemRef>,
    pub hit_direction: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShelfData {
    pub items: [Option<ItemRef>; 3],
    pub align_to_bottom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GatewayData {
    pub age: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConduitData {
    pub active: bool,
    pub hunting: bool,
    /// The entity the conduit is attacking (`Target`).
    pub target: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeamSection {
    pub color: u32,
    pub height: i32,
}

/// Beacon beam sections bottom to top; computed by the caller from the world (the server does not send them).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BeaconData {
    pub sections: Vec<BeamSection>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PistonData {
    pub moved_state: u32,
    pub progress: f32,
    pub extending: bool,
    pub facing: u8,
    /// The piston's own base or head, not a block it pushes.
    pub source: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum BlockEntityData {
    #[default]
    None,
    Sign(Box<SignData>),
    Banner(BannerData),
    Skull(SkullData),
    Pot(PotData),
    Spawner(SpawnerData),
    Vault(VaultData),
    Campfire(CampfireData),
    Brushable(BrushableData),
    Shelf(ShelfData),
    Gateway(GatewayData),
    Conduit(ConduitData),
    Beacon(BeaconData),
    Piston(PistonData),
}

fn int(tag: &Compound, key: &str) -> Option<i64> {
    match tag.get(key)? {
        Value::Byte(v) => Some(i64::from(*v)),
        Value::Short(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Long(v) => Some(*v),
        _ => None,
    }
}

fn flag(tag: &Compound, key: &str) -> bool {
    int(tag, key).is_some_and(|v| v != 0)
}

fn bounded(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn id_string(s: &str) -> Option<String> {
    let ok = !s.is_empty()
        && s.len() <= MAX_ID_LEN
        && s.bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/' | b':'));
    ok.then(|| s.to_owned())
}

fn list_elements(list: &List, max: usize) -> Vec<Value> {
    match list {
        List::String(v) => v.iter().take(max).cloned().map(Value::String).collect(),
        List::Compound(v) => v.iter().take(max).cloned().map(Value::Compound).collect(),
        List::List(v) => v.iter().take(max).cloned().map(Value::List).collect(),
        _ => Vec::new(),
    }
}

fn clamp_component(c: Component) -> Component {
    if c.to_plain_string().chars().count() > MAX_SIGN_CHARS {
        Component::text(bounded(&c.to_plain_string(), MAX_SIGN_CHARS))
    } else {
        c
    }
}

fn parse_side(tag: &Compound) -> SignSide {
    let mut side = SignSide::default();
    if let Some(list) = tag.get_list("messages") {
        for (slot, value) in list_elements(list, 4).iter().enumerate() {
            side.lines[slot] = clamp_component(Component::from_nbt_lenient(value));
        }
    }
    if let Some(name) = tag.get_str("color") {
        side.color = DyeColor::from_name(name).unwrap_or(DyeColor::BLACK);
    }
    side.glowing = flag(tag, "has_glowing_text");
    side
}

fn parse_sign(tag: &Compound) -> SignData {
    SignData {
        front: tag
            .get_compound("front_text")
            .map(parse_side)
            .unwrap_or_default(),
        back: tag
            .get_compound("back_text")
            .map(parse_side)
            .unwrap_or_default(),
        waxed: flag(tag, "is_waxed"),
    }
}

fn pattern_name(value: &Value) -> Option<String> {
    let raw = match value {
        Value::String(s) => s.as_str(),
        Value::Compound(c) => c.get_str("asset_id")?,
        _ => return None,
    };
    id_string(raw.strip_prefix("minecraft:").unwrap_or(raw))
}

fn parse_banner(tag: &Compound) -> BannerData {
    let mut layers = Vec::new();
    if let Some(List::Compound(list)) = tag.get_list("patterns") {
        for layer in list.iter().take(MAX_PATTERNS) {
            let Some(pattern) = layer.get("pattern").and_then(pattern_name) else {
                continue;
            };
            let color = layer
                .get_str("color")
                .and_then(DyeColor::from_name)
                .unwrap_or(DyeColor::WHITE);
            layers.push(BannerLayer { pattern, color });
        }
    }
    BannerData { layers }
}

/// `VaultConfig.DEFAULT.deactivationRange`.
const VAULT_DEFAULT_RANGE: f64 = 4.5;

fn uuid_from_ints(ints: &[i32]) -> Option<Uuid> {
    let [a, b, c, d] = <[i32; 4]>::try_from(ints).ok()?;
    let hi = (u64::from(a as u32) << 32) | u64::from(b as u32);
    let lo = (u64::from(c as u32) << 32) | u64::from(d as u32);
    Some(Uuid::from_u64_pair(hi, lo))
}

fn skin_from_properties(list: &List) -> (Option<String>, bool) {
    for prop in list.as_compounds().iter().take(8) {
        if prop.get_str("name") != Some("textures") {
            continue;
        }
        let Some(value) = prop
            .get_str("value")
            .filter(|v| v.len() <= MAX_TEXTURE_VALUE)
        else {
            continue;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(value) else {
            continue;
        };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let skin = &json["textures"]["SKIN"];
        let url = skin["url"].as_str().filter(|u| u.len() <= MAX_URL);
        let slim = skin["metadata"]["model"].as_str() == Some("slim");
        return (url.map(str::to_owned), slim);
    }
    (None, false)
}

fn parse_profile(value: &Value) -> Option<SkullProfile> {
    match value {
        Value::String(name) => Some(SkullProfile {
            name: Some(bounded(name, MAX_PROFILE_NAME)),
            ..SkullProfile::default()
        }),
        Value::Compound(c) => {
            let (skin_url, slim) = c
                .get_list("properties")
                .map(skin_from_properties)
                .unwrap_or_default();
            Some(SkullProfile {
                name: c.get_str("name").map(|n| bounded(n, MAX_PROFILE_NAME)),
                id: c.get_int_array("id").and_then(uuid_from_ints),
                skin_url,
                slim,
            })
        }
        _ => None,
    }
}

fn item_ref(c: &Compound) -> Option<ItemRef> {
    let id = id_string(c.get_str("id")?)?;
    let count = int(c, "count").unwrap_or(1).clamp(1, 99) as i32;
    (id != "minecraft:air").then_some(ItemRef { id, count })
}

fn pot_side(tag: &Compound, key: &str) -> Option<String> {
    match tag.get(key)? {
        Value::String(s) => id_string(s),
        Value::Compound(c) => id_string(c.get_str("id")?),
        _ => None,
    }
}

fn parse_pot(tag: &Compound) -> PotData {
    let sides = tag.get_compound("sherds").unwrap_or(tag);
    PotData {
        front: pot_side(sides, "front"),
        back: pot_side(sides, "back"),
        left: pot_side(sides, "left"),
        right: pot_side(sides, "right"),
    }
}

fn spawn_entity_id(tag: &Compound) -> Option<String> {
    let data = tag
        .get_compound("SpawnData")
        .or_else(|| tag.get_compound("spawn_data"))?;
    id_string(data.get_compound("entity")?.get_str("id")?)
}

fn parse_items(tag: &Compound, key: &str, out: &mut [Option<ItemRef>]) {
    let Some(List::Compound(list)) = tag.get_list(key) else {
        return;
    };
    for entry in list.iter().take(MAX_ITEM_SLOTS) {
        let slot = int(entry, "Slot").unwrap_or(-1);
        if let (Ok(slot), Some(item)) = (usize::try_from(slot), item_ref(entry))
            && let Some(cell) = out.get_mut(slot)
        {
            *cell = Some(item);
        }
    }
}

impl BlockEntityData {
    /// Parses a network-NBT block entity tag; missing or malformed fields keep their defaults.
    pub fn parse(kind: BlockEntityKind, tag: &Compound) -> Self {
        use BlockEntityKind as K;
        match kind {
            K::Sign | K::HangingSign => Self::Sign(Box::new(parse_sign(tag))),
            K::Banner => Self::Banner(parse_banner(tag)),
            K::Skull => Self::Skull(SkullData {
                profile: tag.get("profile").and_then(parse_profile),
            }),
            K::DecoratedPot => Self::Pot(parse_pot(tag)),
            K::Spawner | K::TrialSpawner => Self::Spawner(SpawnerData {
                entity_id: spawn_entity_id(tag),
                delay: int(tag, "Delay").unwrap_or(20).clamp(0, 2000) as i16,
                range: int(tag, "RequiredPlayerRange").unwrap_or(16).clamp(0, 256) as i16,
                next_mob_spawns_at: int(tag, "next_mob_spawns_at").unwrap_or(0),
            }),
            K::Vault => {
                let shared = tag.get_compound("shared_data");
                let connected = match shared.and_then(|s| s.get_list("connected_players")) {
                    Some(List::IntArray(list)) => list
                        .iter()
                        .take(MAX_VAULT_CONNECTIONS)
                        .filter_map(|ints| uuid_from_ints(ints))
                        .collect(),
                    _ => Vec::new(),
                };
                let range = shared
                    .and_then(|s| s.get_f64("connected_particles_range"))
                    .filter(|r| r.is_finite())
                    .unwrap_or(VAULT_DEFAULT_RANGE);
                Self::Vault(VaultData {
                    display_item: shared
                        .and_then(|s| s.get_compound("display_item"))
                        .and_then(item_ref),
                    connected,
                    connection_range_tenths: (range.clamp(0.0, 128.0) * 10.0) as u16,
                })
            }
            K::Campfire => {
                let mut data = CampfireData::default();
                parse_items(tag, "Items", &mut data.items);
                Self::Campfire(data)
            }
            K::Brushable => Self::Brushable(BrushableData {
                item: tag.get_compound("item").and_then(item_ref),
                hit_direction: int(tag, "hit_direction")
                    .and_then(|d| u8::try_from(d).ok())
                    .filter(|d| *d < 6),
            }),
            K::Shelf => {
                let mut data = ShelfData {
                    align_to_bottom: flag(tag, "align_items_to_bottom"),
                    ..ShelfData::default()
                };
                parse_items(tag, "Items", &mut data.items);
                Self::Shelf(data)
            }
            K::EndGateway => Self::Gateway(GatewayData {
                age: int(tag, "Age").unwrap_or(0).max(0),
            }),
            K::Conduit => Self::Conduit(ConduitData {
                active: false,
                hunting: tag.contains_key("Target"),
                target: tag.get_int_array("Target").and_then(uuid_from_ints),
            }),
            K::Piston => Self::Piston(PistonData {
                moved_state: tag.get_compound("blockState").map_or(0, state_from_nbt),
                progress: tag.get_f32("progress").unwrap_or(0.0).clamp(0.0, 1.0),
                extending: flag(tag, "extending"),
                facing: int(tag, "facing").unwrap_or(0).clamp(0, 5) as u8,
                source: flag(tag, "source"),
            }),
            _ => Self::None,
        }
    }
}

/// Resolves a `{Name, Properties}` block state compound; unknown names give air (0).
pub fn state_from_nbt(tag: &Compound) -> u32 {
    let version = mcr_data::v26_3();
    let Some(name) = tag.get_str("Name").filter(|n| n.len() <= MAX_ID_LEN) else {
        return 0;
    };
    let Some(info) = version
        .block_by_name(name)
        .and_then(|b| version.block_info(b))
    else {
        return 0;
    };
    let props = tag.get_compound("Properties");
    let (first, end) = info.states;
    (first..end)
        .find(|&state| {
            version.state_properties(state).all(|(k, v)| {
                props
                    .and_then(|p| p.get_str(k))
                    .is_none_or(|want| want == v)
            })
        })
        .unwrap_or(info.default_state)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamBlock {
    Open,
    Tint(u32),
    Solid,
}

/// Vanilla beam colouring: the beacon itself is the first (white) section; tints open a section or mix into the
/// previous one, and the first solid block clears the beam.
pub fn beam_sections(
    base_y: i32,
    top_y: i32,
    mut at: impl FnMut(i32) -> BeamBlock,
) -> Vec<BeamSection> {
    const WHITE: u32 = 0xF9FFFE;
    let mut sections = vec![BeamSection {
        color: WHITE,
        height: 1,
    }];
    let end = top_y.min(base_y.saturating_add(MAX_BEAM_HEIGHT));
    for y in base_y.saturating_add(1)..=end {
        match at(y) {
            BeamBlock::Tint(rgb) => {
                let count = sections.len();
                let last = sections.last_mut().expect("seeded with the beacon section");
                if count <= 1 {
                    sections.push(BeamSection {
                        color: rgb,
                        height: 1,
                    });
                } else if last.color == rgb {
                    last.height += 1;
                } else if count < MAX_BEAM_SECTIONS {
                    let color = mix(last.color, rgb);
                    sections.push(BeamSection { color, height: 1 });
                } else {
                    last.height += 1;
                }
            }
            BeamBlock::Solid => return Vec::new(),
            BeamBlock::Open => sections.last_mut().expect("seeded").height += 1,
        }
    }
    sections
}

fn mix(a: u32, b: u32) -> u32 {
    let ch = |shift: u32| (((a >> shift) & 0xFF) + ((b >> shift) & 0xFF)) / 2;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// UUID vanilla derives for an unresolved profile name in offline mode.
pub fn offline_uuid(name: &str) -> Uuid {
    use md5::{Digest, Md5};
    let mut hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}")).into();
    hash[6] = (hash[6] & 0x0F) | 0x30;
    hash[8] = (hash[8] & 0x3F) | 0x80;
    Uuid::from_bytes(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_list(lines: &[&str]) -> List {
        List::String(lines.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn sign_both_sides() {
        let mut front = Compound::new();
        front.insert("messages", Value::List(text_list(&["\"hi\"", "", "", ""])));
        front.insert("color", Value::String("red".into()));
        front.insert("has_glowing_text", Value::Byte(1));
        let mut tag = Compound::new();
        tag.insert("front_text", Value::Compound(front));
        tag.insert("is_waxed", Value::Byte(1));
        let BlockEntityData::Sign(sign) = BlockEntityData::parse(BlockEntityKind::Sign, &tag)
        else {
            panic!("not a sign");
        };
        assert!(sign.waxed);
        assert!(sign.front.glowing);
        assert_eq!(sign.front.color, DyeColor::from_name("red").unwrap());
        assert!(sign.back.is_blank());
    }

    #[test]
    fn oversized_sign_text_is_cut() {
        let long = "x".repeat(10_000);
        let mut side = Compound::new();
        side.insert("messages", Value::List(text_list(&[&long, "", "", ""])));
        let mut tag = Compound::new();
        tag.insert("front_text", Value::Compound(side));
        let BlockEntityData::Sign(sign) = BlockEntityData::parse(BlockEntityKind::Sign, &tag)
        else {
            panic!("not a sign");
        };
        assert!(sign.front.lines[0].to_plain_string().chars().count() <= MAX_SIGN_CHARS);
    }

    #[test]
    fn banner_patterns_capped() {
        let layers = (0..40)
            .map(|i| {
                let mut c = Compound::new();
                c.insert("pattern", Value::String(format!("minecraft:stripe_{i}")));
                c.insert("color", Value::String("blue".into()));
                c
            })
            .collect();
        let mut tag = Compound::new();
        tag.insert("patterns", Value::List(List::Compound(layers)));
        let BlockEntityData::Banner(b) = BlockEntityData::parse(BlockEntityKind::Banner, &tag)
        else {
            panic!("not a banner");
        };
        assert_eq!(b.layers.len(), MAX_PATTERNS);
        assert_eq!(b.layers[0].pattern, "stripe_0");
    }

    #[test]
    fn hostile_banner_pattern_ids_are_dropped() {
        let mut c = Compound::new();
        c.insert("pattern", Value::String("../../etc/passwd\0".into()));
        c.insert("color", Value::String("blue".into()));
        let mut tag = Compound::new();
        tag.insert("patterns", Value::List(List::Compound(vec![c])));
        let BlockEntityData::Banner(b) = BlockEntityData::parse(BlockEntityKind::Banner, &tag)
        else {
            panic!("not a banner");
        };
        assert!(b.layers.is_empty());
    }

    #[test]
    fn skull_profile_name_is_cut() {
        let mut tag = Compound::new();
        tag.insert(
            "profile",
            Value::String("a_very_long_player_name_indeed".into()),
        );
        let BlockEntityData::Skull(s) = BlockEntityData::parse(BlockEntityKind::Skull, &tag) else {
            panic!("not a skull");
        };
        assert_eq!(
            s.profile.unwrap().name.unwrap().chars().count(),
            MAX_PROFILE_NAME
        );
    }

    #[test]
    fn skull_profile_texture_property() {
        let json = r#"{"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/abc","metadata":{"model":"slim"}}}}"#;
        let value = base64::engine::general_purpose::STANDARD.encode(json);
        let mut prop = Compound::new();
        prop.insert("name", Value::String("textures".into()));
        prop.insert("value", Value::String(value));
        let mut profile = Compound::new();
        profile.insert("name", Value::String("Steve".into()));
        profile.insert("id", Value::IntArray(vec![1, 2, 3, 4]));
        profile.insert("properties", Value::List(List::Compound(vec![prop])));
        let mut tag = Compound::new();
        tag.insert("profile", Value::Compound(profile));
        let BlockEntityData::Skull(s) = BlockEntityData::parse(BlockEntityKind::Skull, &tag) else {
            panic!("not a skull");
        };
        let p = s.profile.unwrap();
        assert!(p.slim);
        assert!(p.skin_url.unwrap().ends_with("/abc"));
        assert_eq!(p.id, Some(Uuid::from_u64_pair(1 << 32 | 2, 3 << 32 | 4)));
    }

    #[test]
    fn campfire_slots() {
        let item = |slot: i8, id: &str| {
            let mut c = Compound::new();
            c.insert("Slot", Value::Byte(slot));
            c.insert("id", Value::String(id.into()));
            c.insert("count", Value::Int(1));
            c
        };
        let mut tag = Compound::new();
        tag.insert(
            "Items",
            Value::List(List::Compound(vec![
                item(0, "minecraft:beef"),
                item(3, "minecraft:cod"),
                item(9, "minecraft:cod"),
                item(-1, "minecraft:cod"),
            ])),
        );
        let BlockEntityData::Campfire(c) = BlockEntityData::parse(BlockEntityKind::Campfire, &tag)
        else {
            panic!("not a campfire");
        };
        assert_eq!(c.items[0].as_ref().unwrap().id, "minecraft:beef");
        assert!(c.items[1].is_none());
        assert_eq!(c.items[3].as_ref().unwrap().id, "minecraft:cod");
    }

    #[test]
    fn spawner_entity() {
        let mut entity = Compound::new();
        entity.insert("id", Value::String("minecraft:pig".into()));
        let mut data = Compound::new();
        data.insert("entity", Value::Compound(entity));
        let mut tag = Compound::new();
        tag.insert("SpawnData", Value::Compound(data));
        let BlockEntityData::Spawner(s) = BlockEntityData::parse(BlockEntityKind::Spawner, &tag)
        else {
            panic!("not a spawner");
        };
        assert_eq!(s.entity_id.as_deref(), Some("minecraft:pig"));
    }

    #[test]
    fn spawner_range_and_trial_spawner_timing() {
        let mut tag = Compound::new();
        tag.insert("RequiredPlayerRange", Value::Short(i16::MAX));
        tag.insert("next_mob_spawns_at", Value::Long(1234));
        let BlockEntityData::Spawner(s) =
            BlockEntityData::parse(BlockEntityKind::TrialSpawner, &tag)
        else {
            panic!("not a spawner");
        };
        assert_eq!((s.range, s.next_mob_spawns_at), (256, 1234));
        let BlockEntityData::Spawner(d) =
            BlockEntityData::parse(BlockEntityKind::Spawner, &Compound::new())
        else {
            panic!("not a spawner");
        };
        assert_eq!(d.range, 16);
    }

    #[test]
    fn vault_links_are_bounded_and_default_to_four_and_a_half_blocks() {
        let links: Vec<Vec<i32>> = (0..100).map(|i| vec![0, 0, 0, i]).collect();
        let mut shared = Compound::new();
        shared.insert("connected_players", Value::List(List::IntArray(links)));
        let mut tag = Compound::new();
        tag.insert("shared_data", Value::Compound(shared));
        let BlockEntityData::Vault(v) = BlockEntityData::parse(BlockEntityKind::Vault, &tag) else {
            panic!("not a vault");
        };
        assert_eq!(v.connected.len(), MAX_VAULT_CONNECTIONS);
        assert_eq!(v.connection_range_tenths, 45);
        let mut shared = Compound::new();
        shared.insert("connected_particles_range", Value::Double(f64::NAN));
        let mut tag = Compound::new();
        tag.insert("shared_data", Value::Compound(shared));
        let BlockEntityData::Vault(v) = BlockEntityData::parse(BlockEntityKind::Vault, &tag) else {
            panic!("not a vault");
        };
        assert_eq!(v.connection_range_tenths, 45);
    }

    #[test]
    fn conduit_target_is_a_uuid() {
        let mut tag = Compound::new();
        tag.insert("Target", Value::IntArray(vec![0, 1, 0, 2]));
        let BlockEntityData::Conduit(c) = BlockEntityData::parse(BlockEntityKind::Conduit, &tag)
        else {
            panic!("not a conduit");
        };
        assert_eq!(c.target, Some(Uuid::from_u64_pair(1, 2)));
        assert!(c.hunting);
    }

    #[test]
    fn pot_sherds() {
        let mut brick = Compound::new();
        brick.insert("id", Value::String("minecraft:angler_pottery_sherd".into()));
        let mut tag = Compound::new();
        tag.insert("front", Value::Compound(brick));
        let BlockEntityData::Pot(p) = BlockEntityData::parse(BlockEntityKind::DecoratedPot, &tag)
        else {
            panic!("not a pot");
        };
        assert_eq!(p.front.as_deref(), Some("minecraft:angler_pottery_sherd"));
        assert!(p.back.is_none());
    }

    #[test]
    fn beam_sections_mix_tints_and_stop_at_solid() {
        let sections = beam_sections(0, 20, |y| match y {
            3 => BeamBlock::Tint(0xFF0000),
            6 => BeamBlock::Tint(0x0000FF),
            15 => BeamBlock::Solid,
            _ => BeamBlock::Open,
        });
        assert!(sections.is_empty());
        let sections = beam_sections(0, 20, |y| match y {
            3 => BeamBlock::Tint(0xFF0000),
            6 => BeamBlock::Tint(0x0000FF),
            _ => BeamBlock::Open,
        });
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[1].color, 0xFF0000);
        assert_eq!(sections[2].color, 0x7F007F);
        assert_eq!(sections.iter().map(|s| s.height).sum::<i32>(), 21);
    }

    #[test]
    fn offline_uuid_matches_vanilla() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }
}
