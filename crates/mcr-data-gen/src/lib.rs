use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

#[path = "../../mcr-data/src/tables.rs"]
pub mod tables;
pub mod version_tables;

use tables::{
    Aabb, DIRECTION_NAMES, PacketEntry, PropDef, Registry, STATE_NAMES, Tables, flag, fluid_bits,
};

#[derive(Deserialize)]
struct ExtractedBlocks {
    blocks: Vec<ExtractedBlock>,
}

#[derive(Deserialize)]
struct ExtractedBlock {
    name: String,
    friction: f32,
    speed_factor: f32,
    jump_factor: f32,
    destroy_time: f32,
    #[serde(default)]
    requires_tool: bool,
    explosion_resistance: f32,
    sound_type: String,
    states: Vec<ExtractedState>,
}

#[derive(Deserialize)]
struct ExtractedFluid {
    amount: u16,
    source: bool,
    falling: bool,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct ExtractedState {
    id: u32,
    collision: Vec<[f64; 6]>,
    outline: Vec<[f64; 6]>,
    occlusion: Vec<[f64; 6]>,
    light_emission: u8,
    light_block: u8,
    propagates_skylight_down: bool,
    can_occlude: bool,
    use_shape_for_light_occlusion: bool,
    solid_render: bool,
    full_collision: bool,
    sturdy_faces: u8,
    occlusion_faces: u8,
    air: bool,
    liquid: bool,
    replaceable: bool,
    has_block_entity: bool,
    redstone_conductor: bool,
    legacy_solid: bool,
    map_color: u8,
    fluid: Option<ExtractedFluid>,
}

#[derive(Deserialize)]
struct ExtractedItem {
    id: u32,
    name: String,
    max_stack_size: u16,
    max_damage: u32,
    block: Option<String>,
}

#[derive(Deserialize)]
struct ExtractedEntity {
    id: u32,
    name: String,
    width: f32,
    height: f32,
    eye_height: f32,
    category: String,
}

#[derive(Deserialize)]
struct ExtractedFluidType {
    id: u32,
    name: String,
    source: bool,
}

#[derive(Default)]
struct Interner {
    strings: Vec<String>,
    index: HashMap<String, u32>,
}

impl Interner {
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = u32::try_from(self.strings.len()).expect("string count");
        self.strings.push(s.to_owned());
        self.index.insert(s.to_owned(), i);
        i
    }
}

#[derive(Default)]
struct ShapePool {
    starts: Vec<u32>,
    boxes: Vec<Aabb>,
    index: HashMap<Vec<[u32; 6]>, u16>,
}

impl ShapePool {
    fn intern(&mut self, shape: &[[f64; 6]]) -> Result<u16> {
        if self.starts.is_empty() {
            self.starts.push(0);
        }
        let key: Vec<[u32; 6]> = shape
            .iter()
            .map(|b| b.map(|v| (v as f32).to_bits()))
            .collect();
        if let Some(&i) = self.index.get(&key) {
            return Ok(i);
        }
        let i = u16::try_from(self.index.len()).context("more than 65535 distinct shapes")?;
        for b in shape {
            let f = b.map(|v| v as f32);
            self.boxes.push(Aabb {
                min: [f[0], f[1], f[2]],
                max: [f[3], f[4], f[5]],
            });
        }
        self.starts
            .push(u32::try_from(self.boxes.len()).expect("box count"));
        self.index.insert(key, i);
        Ok(i)
    }
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

#[derive(Debug)]
pub struct Inputs {
    pub reports: PathBuf,
    pub extracted: PathBuf,
    pub version_json: PathBuf,
}

impl Inputs {
    pub fn in_cache(cache: &Path, version: &str) -> Self {
        let root = cache.join("versions").join(version);
        Self {
            reports: root.join("generated/reports"),
            extracted: root.join("extracted"),
            version_json: root.join("src/version.json"),
        }
    }
}

pub fn generate(inputs: &Inputs) -> Result<Vec<u8>> {
    let mut strings = Interner::default();
    let mut t = Tables::default();

    let version: Value = read_json(&inputs.version_json)?;
    t.version_name = version["id"].as_str().context("version id")?.to_owned();
    t.protocol = u32::try_from(
        version["protocol_version"]
            .as_u64()
            .context("protocol_version")?,
    )?;

    let registries: Map<String, Value> = read_json(&inputs.reports.join("registries.json"))?;
    for (name, reg) in &registries {
        let entries = reg["entries"].as_object().context("registry entries")?;
        let mut by_id = vec![None; entries.len()];
        for (entry, v) in entries {
            let id = v["protocol_id"].as_u64().context("protocol_id")? as usize;
            ensure!(
                id < by_id.len() && by_id[id].is_none(),
                "{name}: ids not dense"
            );
            by_id[id] = Some(entry.as_str());
        }
        let entries: Vec<u32> = by_id
            .into_iter()
            .map(|e| e.map(|e| strings.intern(e)).context("registry hole"))
            .collect::<Result<_>>()?;
        let mut sorted: Vec<u32> = (0..entries.len() as u32).collect();
        sorted.sort_by(|&a, &b| {
            strings.strings[entries[a as usize] as usize]
                .cmp(&strings.strings[entries[b as usize] as usize])
        });
        t.registries.push(Registry {
            name: strings.intern(name),
            entries,
            sorted,
        });
    }
    let reg_names = |key: &str| -> Result<Vec<String>> {
        let r = t
            .registries
            .iter()
            .find(|r| strings.strings[r.name as usize] == key)
            .with_context(|| format!("registry {key}"))?;
        Ok(r.entries
            .iter()
            .map(|&e| strings.strings[e as usize].clone())
            .collect())
    };
    let block_names = reg_names("minecraft:block")?;
    let item_names = reg_names("minecraft:item")?;
    let entity_names = reg_names("minecraft:entity_type")?;
    let fluid_names = reg_names("minecraft:fluid")?;
    let block_ids: HashMap<&str, u32> = block_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i as u32))
        .collect();

    let report: Map<String, Value> = read_json(&inputs.reports.join("blocks.json"))?;
    let extracted: ExtractedBlocks = read_json(&inputs.extracted.join("blocks.json"))?;
    ensure!(
        extracted.blocks.len() == block_names.len(),
        "block count mismatch"
    );

    let mut shapes = ShapePool::default();
    let empty = shapes.intern(&[])?;
    ensure!(empty == 0);
    let mut next_state = 0u32;
    t.blocks.prop_start.push(0);
    t.blocks.first_state.push(0);
    for (bid, block) in extracted.blocks.iter().enumerate() {
        ensure!(
            block.name == block_names[bid],
            "block order mismatch at {bid}"
        );
        let rep = report
            .get(&block.name)
            .with_context(|| format!("{} missing in blocks report", block.name))?;
        let b = &mut t.blocks;
        b.friction.push(block.friction);
        b.speed_factor.push(block.speed_factor);
        b.jump_factor.push(block.jump_factor);
        b.destroy_time.push(block.destroy_time);
        b.requires_tool.push(u8::from(block.requires_tool));
        b.explosion_resistance.push(block.explosion_resistance);
        b.sound_type.push(strings.intern(&block.sound_type));

        let mut counts = Vec::new();
        if let Some(props) = rep.get("properties").and_then(Value::as_object) {
            let mut sorted: Vec<_> = props.iter().collect();
            sorted.sort_by_key(|(name, _)| name.as_str());
            for (pname, values) in sorted {
                let values = values.as_array().context("property values")?;
                let value_start = u32::try_from(b.prop_values.len())?;
                for v in values {
                    b.prop_values
                        .push(strings.intern(v.as_str().context("value")?));
                }
                counts.push(values.len() as u32);
                b.props.push(PropDef {
                    name: strings.intern(pname),
                    value_start,
                    value_count: values.len() as u32,
                });
            }
        }
        b.prop_start.push(u32::try_from(b.props.len())?);

        let rep_states = rep["states"].as_array().context("report states")?;
        ensure!(
            rep_states.len() == block.states.len(),
            "{}: state count",
            block.name
        );
        let expected: u32 = counts.iter().product();
        ensure!(
            expected as usize == block.states.len(),
            "{}: radix size",
            block.name
        );
        let first = next_state;
        let mut default = None;
        for (k, (s, rs)) in block.states.iter().zip(rep_states).enumerate() {
            ensure!(
                s.id == next_state,
                "{}: non-contiguous state ids",
                block.name
            );
            ensure!(
                rs["id"].as_u64() == Some(u64::from(s.id)),
                "{}: report id",
                block.name
            );
            if rs["default"].as_bool() == Some(true) {
                default = Some(s.id);
            }
            check_properties(&block.name, rs, &t, &strings, bid, k as u32)?;
            push_state(&mut t, &mut shapes, bid as u16, s, &fluid_names)?;
            next_state += 1;
        }
        t.blocks
            .default_state
            .push(default.with_context(|| format!("{} has no default state", block.name))?);
        t.blocks.first_state.push(next_state);
        ensure!(first < next_state, "{} has no states", block.name);
    }
    let report_states: usize = report
        .values()
        .map(|b| b["states"].as_array().map_or(0, Vec::len))
        .sum();
    ensure!(
        report_states == next_state as usize,
        "state coverage mismatch"
    );
    t.shape_starts = shapes.starts;
    t.shape_boxes = shapes.boxes;

    let items: Vec<ExtractedItem> = read_json(&inputs.extracted.join("items.json"))?;
    ensure!(items.len() == item_names.len(), "item count mismatch");
    for item in &items {
        ensure!(
            item_names[item.id as usize] == item.name,
            "item order mismatch"
        );
        t.items.max_stack_size.push(item.max_stack_size);
        t.items.max_damage.push(item.max_damage);
        let block = match &item.block {
            Some(b) => i32::try_from(*block_ids.get(b.as_str()).context("item block")?)?,
            None => -1,
        };
        t.items.block.push(block);
    }
    item_components(inputs, &item_names, &mut strings, &mut t)?;

    let entities: Vec<ExtractedEntity> = read_json(&inputs.extracted.join("entity_types.json"))?;
    ensure!(
        entities.len() == entity_names.len(),
        "entity count mismatch"
    );
    for e in &entities {
        ensure!(
            entity_names[e.id as usize] == e.name,
            "entity order mismatch"
        );
        t.entities.width.push(e.width);
        t.entities.height.push(e.height);
        t.entities.eye_height.push(e.eye_height);
        t.entities.category.push(strings.intern(&e.category));
    }

    let fluids: Vec<ExtractedFluidType> = read_json(&inputs.extracted.join("fluids.json"))?;
    ensure!(fluids.len() == fluid_names.len(), "fluid count mismatch");
    for f in &fluids {
        ensure!(fluid_names[f.id as usize] == f.name, "fluid order mismatch");
        t.fluid_source.push(u8::from(f.source));
    }

    let packets: Map<String, Value> = read_json(&inputs.reports.join("packets.json"))?;
    for (state, dirs) in &packets {
        let state_idx = STATE_NAMES
            .iter()
            .position(|n| n == state)
            .with_context(|| format!("unknown protocol state {state}"))?;
        for (dir, entries) in dirs.as_object().context("direction map")? {
            let dir_idx = DIRECTION_NAMES
                .iter()
                .position(|n| n == dir)
                .with_context(|| format!("unknown direction {dir}"))?;
            for (name, v) in entries.as_object().context("packet map")? {
                t.packets.push(PacketEntry {
                    state: state_idx as u8,
                    direction: dir_idx as u8,
                    id: u32::try_from(v["protocol_id"].as_u64().context("packet id")?)?,
                    name: strings.intern(name),
                });
            }
        }
    }
    t.packets.sort_by(|a, b| {
        (a.state, a.direction)
            .cmp(&(b.state, b.direction))
            .then_with(|| strings.strings[a.name as usize].cmp(&strings.strings[b.name as usize]))
    });

    t.strings = strings.strings;
    Ok(t.encode())
}

fn check_properties(
    name: &str,
    rep_state: &Value,
    t: &Tables,
    strings: &Interner,
    bid: usize,
    k: u32,
) -> Result<()> {
    let blocks = &t.blocks;
    let props = &blocks.props[blocks.prop_start[bid] as usize..blocks.prop_start[bid + 1] as usize];
    let actual = rep_state.get("properties").and_then(Value::as_object);
    ensure!(
        actual.map_or(0, Map::len) == props.len(),
        "{name}: property count mismatch"
    );
    let mut stride: u32 = props.iter().map(|p| p.value_count).product();
    for p in props {
        stride /= p.value_count;
        let idx = k / stride % p.value_count;
        let want = &strings.strings[blocks.prop_values[(p.value_start + idx) as usize] as usize];
        let key = &strings.strings[p.name as usize];
        let got = actual.and_then(|a| a.get(key)).and_then(Value::as_str);
        ensure!(
            got == Some(want.as_str()),
            "{name}: state {k} property {key} not in radix order"
        );
    }
    Ok(())
}

fn push_state(
    t: &mut Tables,
    shapes: &mut ShapePool,
    block: u16,
    s: &ExtractedState,
    fluid_names: &[String],
) -> Result<()> {
    let st = &mut t.states;
    st.block.push(block);
    let mut flags = 0;
    for (on, bit) in [
        (s.air, flag::AIR),
        (s.liquid, flag::LIQUID),
        (s.replaceable, flag::REPLACEABLE),
        (s.has_block_entity, flag::HAS_BLOCK_ENTITY),
        (s.can_occlude, flag::CAN_OCCLUDE),
        (
            s.use_shape_for_light_occlusion,
            flag::USE_SHAPE_FOR_LIGHT_OCCLUSION,
        ),
        (s.solid_render, flag::SOLID_RENDER),
        (s.full_collision, flag::FULL_COLLISION),
        (s.propagates_skylight_down, flag::PROPAGATES_SKYLIGHT_DOWN),
        (s.redstone_conductor, flag::REDSTONE_CONDUCTOR),
        (s.legacy_solid, flag::LEGACY_SOLID),
    ] {
        if on {
            flags |= bit;
        }
    }
    st.flags.push(flags);
    st.light_emission.push(s.light_emission);
    st.light_block.push(s.light_block);
    st.collision.push(shapes.intern(&s.collision)?);
    st.outline.push(shapes.intern(&s.outline)?);
    st.occlusion.push(shapes.intern(&s.occlusion)?);
    st.map_color.push(s.map_color);
    st.sturdy_faces.push(s.sturdy_faces);
    st.occlusion_faces.push(s.occlusion_faces);
    let fluid = match &s.fluid {
        None => 0,
        Some(f) => {
            let Some(id) = fluid_names.iter().position(|n| *n == f.kind) else {
                bail!("unknown fluid {}", f.kind);
            };
            ensure!(id <= fluid_bits::ID_MASK as usize && f.amount <= fluid_bits::AMOUNT_MASK);
            fluid_bits::PRESENT
                | id as u16
                | f.amount << fluid_bits::AMOUNT_SHIFT
                | if f.source { fluid_bits::SOURCE } else { 0 }
                | if f.falling { fluid_bits::FALLING } else { 0 }
        }
    };
    st.fluid.push(fluid);
    Ok(())
}

const EQUIP_SLOTS: [&str; 8] = [
    "head", "chest", "legs", "feet", "body", "saddle", "mainhand", "offhand",
];

fn item_components(
    inputs: &Inputs,
    item_names: &[String],
    strings: &mut Interner,
    t: &mut Tables,
) -> Result<()> {
    let dir = inputs.reports.join("minecraft/components/item");
    let component_ids = component_type_ids(inputs)?;
    let it = &mut t.items;
    it.tool_start.push(0);
    for name in item_names {
        let short = name.strip_prefix("minecraft:").unwrap_or(name);
        let file = dir.join(format!("{short}.json"));
        let comps: Value = if file.is_file() {
            read_json::<Value>(&file)?["components"].clone()
        } else {
            Value::Null
        };
        let mut words = [0u32; 4];
        for key in comps.as_object().into_iter().flat_map(Map::keys) {
            let id = *component_ids
                .get(key.as_str())
                .with_context(|| format!("unknown component {key}"))?;
            ensure!(id < 128, "component id {id} out of range");
            words[id / 32] |= 1 << (id % 32);
        }
        it.default_components.extend_from_slice(&words);
        let tool = comps.get("minecraft:tool");
        let (mut default_speed, mut damage, mut flags) = (-1.0f32, 0u8, 0u8);
        if let Some(tool) = tool {
            default_speed = tool["default_mining_speed"].as_f64().unwrap_or(1.0) as f32;
            damage = tool["damage_per_block"].as_u64().unwrap_or(1) as u8;
            flags = 1 | if tool["can_destroy_blocks_in_creative"]
                .as_bool()
                .unwrap_or(true)
            {
                2
            } else {
                0
            };
            for rule in tool["rules"].as_array().map_or(&[][..], Vec::as_slice) {
                let blocks: Vec<&str> = match &rule["blocks"] {
                    Value::String(s) => vec![s.as_str()],
                    Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                    _ => Vec::new(),
                };
                for b in blocks {
                    it.tool_blocks.push(strings.intern(b));
                    it.tool_speed
                        .push(rule["speed"].as_f64().map_or(-1.0, |v| v as f32));
                    it.tool_correct
                        .push(match rule["correct_for_drops"].as_bool() {
                            None => 0,
                            Some(false) => 1,
                            Some(true) => 2,
                        });
                }
            }
        }
        it.tool_start
            .push(u32::try_from(it.tool_blocks.len()).context("tool rules")?);
        it.tool_default_speed.push(default_speed);
        it.tool_damage.push(damage);
        it.tool_flags.push(flags);
        it.equip_slot.push(
            comps
                .get("minecraft:equippable")
                .and_then(|e| e["slot"].as_str())
                .and_then(|s| EQUIP_SLOTS.iter().position(|n| *n == s))
                .map_or(255, |p| p as u8),
        );
        it.consume_seconds
            .push(comps.get("minecraft:consumable").map_or(-1.0, |c| {
                c["consume_seconds"].as_f64().unwrap_or(1.6) as f32
            }));
        it.food_flags.push(match comps.get("minecraft:food") {
            None => 0,
            Some(f) => {
                1 | if f["can_always_eat"].as_bool().unwrap_or(false) {
                    2
                } else {
                    0
                }
            }
        });
        it.cooldown_seconds.push(
            comps
                .get("minecraft:use_cooldown")
                .map_or(-1.0, |c| c["seconds"].as_f64().unwrap_or(0.0) as f32),
        );
    }
    Ok(())
}

/// Tooltip-relevant default components per item, one line per item that has any:
/// `<item> [r<rarity>] [b] [a=<attribute>,<id>,<amount>,<operation>,<slot>[,hidden]]* [j=<song>] [h=<instrument>]
/// [f=<flight>] [o=<amplifier>] [scale=<potion duration scale>] [honey=<level>] [bees] [potion] [stew]`.
pub fn item_tooltips(inputs: &Inputs) -> Result<String> {
    let registries: Map<String, Value> = read_json(&inputs.reports.join("registries.json"))?;
    let entries = registries["minecraft:item"]["entries"]
        .as_object()
        .context("item entries")?;
    let mut names: Vec<(u64, &str)> = entries
        .iter()
        .map(|(n, e)| {
            Ok((
                e["protocol_id"].as_u64().context("protocol_id")?,
                n.as_str(),
            ))
        })
        .collect::<Result<_>>()?;
    names.sort_unstable();
    let dir = inputs.reports.join("minecraft/components/item");
    let short = |s: &str| s.strip_prefix("minecraft:").unwrap_or(s).to_owned();
    let mut out = String::new();
    for (_, name) in names {
        let file = dir.join(format!("{}.json", short(name)));
        if !file.is_file() {
            continue;
        }
        let comps = read_json::<Value>(&file)?["components"].clone();
        let get = |k: &str| comps.get(format!("minecraft:{k}"));
        let mut tokens = Vec::new();
        let rarity = match get("rarity").and_then(Value::as_str) {
            Some("uncommon") => 1,
            Some("rare") => 2,
            Some("epic") => 3,
            _ => 0,
        };
        if rarity != 0 {
            tokens.push(format!("r{rarity}"));
        }
        if get("item_name")
            .and_then(|n| n["translate"].as_str())
            .is_some_and(|k| k.starts_with("block."))
        {
            tokens.push("b".to_owned());
        }
        for m in get("attribute_modifiers")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            let field = |k: &str| {
                m[k].as_str()
                    .map(short)
                    .with_context(|| format!("{name}: {k}"))
            };
            let amount = m["amount"].as_f64().context("amount")?;
            let mut token = format!(
                "a={},{},{amount},{},{}",
                field("type")?,
                field("id")?,
                field("operation")?,
                field("slot").unwrap_or_else(|_| "any".to_owned())
            );
            if m["display"]["type"].as_str() == Some("hidden") {
                token.push_str(",hidden");
            }
            tokens.push(token);
        }
        if let Some(song) = get("jukebox_playable").and_then(Value::as_str) {
            tokens.push(format!("j={}", short(song)));
        }
        if let Some(horn) = get("instrument").and_then(Value::as_str) {
            tokens.push(format!("h={}", short(horn)));
        }
        if let Some(flight) = get("fireworks").and_then(|f| f["flight_duration"].as_i64()) {
            tokens.push(format!("f={flight}"));
        }
        if let Some(amp) = get("ominous_bottle_amplifier").and_then(Value::as_i64) {
            tokens.push(format!("o={amp}"));
        }
        if let Some(scale) = get("potion_duration_scale").and_then(Value::as_f64) {
            tokens.push(format!("scale={scale}"));
        }
        if let Some(level) = get("block_state").and_then(|b| b["honey_level"].as_str()) {
            tokens.push(format!("honey={level}"));
        }
        for (key, token) in [
            ("bees", "bees"),
            ("potion_contents", "potion"),
            ("suspicious_stew_effects", "stew"),
        ] {
            if get(key).is_some() {
                tokens.push(token.to_owned());
            }
        }
        if !tokens.is_empty() {
            out.push_str(&short(name));
            for t in tokens {
                out.push(' ');
                out.push_str(&t);
            }
            out.push('\n');
        }
    }
    Ok(out)
}

fn component_type_ids(inputs: &Inputs) -> Result<HashMap<String, usize>> {
    let report: Map<String, Value> = read_json(&inputs.reports.join("registries.json"))?;
    let entries = report["minecraft:data_component_type"]["entries"]
        .as_object()
        .context("data_component_type entries")?;
    entries
        .iter()
        .map(|(name, e)| {
            let id = e["protocol_id"].as_u64().context("protocol_id")?;
            Ok((name.clone(), usize::try_from(id)?))
        })
        .collect()
}

pub fn write_module(out_dir: &Path, blob: &[u8], tooltips: &str) -> Result<()> {
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("data.bin"), blob)?;
    fs::write(out_dir.join("item_tooltips.txt"), tooltips)?;
    fs::write(
        out_dir.join("mod.rs"),
        "pub static BLOB: &[u8] = include_bytes!(\"data.bin\");\n\
         pub static CREATIVE_TABS: &str = include_str!(\"creative_tabs.txt\");\n\
         pub static ITEM_TOOLTIPS: &str = include_str!(\"item_tooltips.txt\");\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> Option<Inputs> {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cache = manifest
            .ancestors()
            .map(|p| p.join(".cache"))
            .find(|p| p.is_dir())?;
        let inputs = Inputs::in_cache(&cache, "26.3");
        inputs
            .extracted
            .join("blocks.json")
            .is_file()
            .then_some(inputs)
    }

    fn committed_blob() -> Vec<u8> {
        fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../mcr-data/src/generated/v26_3/data.bin"),
        )
        .unwrap()
    }

    #[test]
    fn generation_is_deterministic_and_current() {
        let Some(inputs) = inputs() else { return };
        let first = generate(&inputs).unwrap();
        assert_eq!(first, generate(&inputs).unwrap());
        assert!(
            first == committed_blob(),
            "committed tables are stale; rerun mcr-data-gen"
        );
        let tooltips = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../mcr-data/src/generated/v26_3/item_tooltips.txt"),
        )
        .unwrap();
        assert!(
            item_tooltips(&inputs).unwrap() == tooltips,
            "committed item tooltips are stale; rerun mcr-data-gen"
        );
    }

    #[test]
    fn every_report_state_is_covered() {
        let Some(inputs) = inputs() else { return };
        let report: Map<String, Value> = read_json(&inputs.reports.join("blocks.json")).unwrap();
        let v = mcr_data::v26_3();
        let mut count = 0;
        for (name, block) in &report {
            let id = v.block_by_name(name).unwrap();
            for state in block["states"].as_array().unwrap() {
                let sid = state["id"].as_u64().unwrap() as u32;
                let info = v.block_state(sid).unwrap();
                assert_eq!(info.block, id);
                let props = state["properties"].as_object();
                assert_eq!(v.state_properties(sid).count(), props.map_or(0, Map::len));
                for (k, val) in v.state_properties(sid) {
                    assert_eq!(props.unwrap()[k].as_str(), Some(val));
                }
                count += 1;
            }
        }
        assert_eq!(count, v.state_count());
    }
}
