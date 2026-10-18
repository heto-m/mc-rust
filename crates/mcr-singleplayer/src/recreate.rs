//! Re-Create: a world's saved creation settings, read back for a new world.

use std::path::Path;

use mcr_nbt::{Compound, List, Value, read_disk};
use serde_json::{Map, Value as Json, json};

use crate::level::{Difficulty, GameMode, LevelInfo, parse_level};
use crate::props::WorldType;
use crate::{Result, SpError};

const MAX_SAVED_DATA: u64 = 4 << 20;
const LIMITS: mcr_nbt::Limits = mcr_nbt::Limits {
    max_depth: 128,
    max_bytes: 16 << 20,
};
const MAX_LEVEL_DAT: u64 = 32 << 20;
const LEVEL_LIMITS: mcr_nbt::Limits = mcr_nbt::Limits {
    max_depth: 512,
    max_bytes: 64 << 20,
};
const MAX_RULE_ENTRIES: usize = 512;
const MAX_LAYERS: usize = crate::flat::MAX_LAYERS;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Recreate {
    pub level: LevelInfo,
    pub seed: String,
    pub structures: bool,
    pub bonus_chest: bool,
    pub world_type: WorldType,
    /// Validated superflat `generator-settings` JSON.
    pub generator_settings: Option<String>,
    pub game_rules: Vec<(String, String)>,
    pub experiments: Vec<String>,
    /// Entries of the world's own `datapacks` folder.
    pub data_packs: Vec<String>,
}

fn refused(why: &str) -> SpError {
    SpError::Refused(why.into())
}

fn read_nbt(path: &Path, max: u64) -> Result<Option<Compound>> {
    use std::io::Read;
    let file = match crate::safe::open_read(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(SpError::TooLarge(path.display().to_string(), max));
    }
    Ok(Some(read_disk(&bytes, LIMITS)?.1))
}

/// The `data` compound of a 26.x saved data file such as `data/minecraft/world_gen_settings.dat`.
fn saved_data(world: &Path, name: &str) -> Result<Option<Compound>> {
    let path = world
        .join("data")
        .join("minecraft")
        .join(format!("{name}.dat"));
    Ok(
        read_nbt(&path, MAX_SAVED_DATA)?.and_then(|mut root| match root.remove("data") {
            Some(Value::Compound(c)) => Some(c),
            _ => None,
        }),
    )
}

fn flat_json(settings: &Compound) -> Option<String> {
    let mut out = Map::new();
    let layers = match settings.get_list("layers") {
        Some(List::Compound(items)) => items
            .iter()
            .take(MAX_LAYERS + 1)
            .map(|l| {
                Some(json!({
                    "block": l.get_str("block")?,
                    "height": l.get_i32("height")?,
                }))
            })
            .collect::<Option<Vec<_>>>()?,
        Some(List::Empty) | None => Vec::new(),
        Some(_) => return None,
    };
    out.insert("layers".into(), Json::Array(layers));
    if let Some(biome) = settings.get_str("biome") {
        out.insert("biome".into(), Json::String(biome.into()));
    }
    for key in ["features", "lakes"] {
        if let Some(v) = settings.get_bool(key) {
            out.insert(key.into(), Json::Bool(v));
        }
    }
    match settings.get("structure_overrides") {
        Some(Value::String(s)) => {
            out.insert("structure_overrides".into(), Json::String(s.clone()));
        }
        Some(Value::List(List::String(items))) => {
            out.insert(
                "structure_overrides".into(),
                Json::Array(items.iter().take(65).cloned().map(Json::String).collect()),
            );
        }
        _ => {}
    }
    crate::flat::generator_settings(&Json::Object(out).to_string(), &|_| true).ok()
}

/// The world type and flat settings of an overworld generator, or a refusal for types mcr cannot create.
fn generator(wgen: &Compound) -> Result<(WorldType, Option<String>)> {
    let kind = wgen.get_str("type").unwrap_or_default();
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        "flat" => {
            let settings = wgen
                .get_compound("settings")
                .and_then(flat_json)
                .ok_or_else(|| refused("The superflat layers of this world cannot be read"))?;
            Ok((WorldType::Flat, Some(settings)))
        }
        "noise" => {
            let fixed = wgen
                .get_compound("biome_source")
                .and_then(|b| b.get_str("type"))
                .is_some_and(|t| t.ends_with("fixed"));
            if fixed {
                return Err(refused("Single biome worlds cannot be re-created by mcr"));
            }
            match wgen.get_str("settings").unwrap_or("minecraft:overworld") {
                "minecraft:overworld" => Ok((WorldType::Normal, None)),
                "minecraft:large_biomes" => Ok((WorldType::LargeBiomes, None)),
                "minecraft:amplified" => Ok((WorldType::Amplified, None)),
                _ => Err(refused("This world type cannot be re-created by mcr")),
            }
        }
        _ => Err(refused("This world type cannot be re-created by mcr")),
    }
}

fn rules(map: &Compound) -> Vec<(String, String)> {
    let pairs: Vec<(String, String)> = map
        .iter()
        .take(MAX_RULE_ENTRIES)
        .filter_map(|(k, v)| {
            let value = match v {
                Value::Byte(b) => (*b != 0).to_string(),
                Value::Int(i) => i.to_string(),
                Value::String(s) => s.clone(),
                _ => return None,
            };
            Some((k.to_owned(), value))
        })
        .collect();
    mcr_data::game_rules::changed_rules(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

fn strings(list: Option<&List>) -> Vec<String> {
    match list {
        Some(List::String(items)) => items
            .iter()
            .take(crate::packs::MAX_PACKS * 2)
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

/// Reads `world` for Re-Create: `level.dat`, the 26.x saved data files or their pre-26 places, and `datapacks`.
pub fn read(world: &Path) -> Result<Recreate> {
    let bytes = {
        use std::io::Read;
        let mut b = Vec::new();
        crate::safe::open_read(&world.join("level.dat"))
            .map_err(|_| refused("The world has no readable level.dat"))?
            .take(MAX_LEVEL_DAT + 1)
            .read_to_end(&mut b)?;
        if b.len() as u64 > MAX_LEVEL_DAT {
            return Err(SpError::TooLarge("level.dat".into(), MAX_LEVEL_DAT));
        }
        b
    };
    let level = parse_level(&bytes)?;
    let (_, root) = read_disk(&bytes, LEVEL_LIMITS)?;
    let data = root
        .get_compound("Data")
        .ok_or_else(|| refused("level.dat has no Data compound"))?;
    let gen_settings = match saved_data(world, "world_gen_settings")? {
        Some(c) => c,
        None => data
            .get_compound("WorldGenSettings")
            .cloned()
            .ok_or_else(|| refused("The world's generation settings are missing"))?,
    };
    let seed = gen_settings.get_i64("seed").unwrap_or(0);
    let overworld = gen_settings
        .get_compound("dimensions")
        .and_then(|d| d.get_compound("minecraft:overworld"))
        .and_then(|o| o.get_compound("generator"))
        .ok_or_else(|| refused("The world has no overworld generator"))?;
    let (world_type, generator_settings) = generator(overworld)?;
    let game_rules = match saved_data(world, "game_rules")? {
        Some(map) => rules(&map),
        None => data
            .get_compound("GameRules")
            .map(rules)
            .unwrap_or_default(),
    };
    let enabled = strings(
        data.get_compound("DataPacks")
            .and_then(|p| p.get_list("Enabled")),
    );
    Ok(Recreate {
        level,
        seed: seed.to_string(),
        structures: gen_settings.get_bool("generate_features").unwrap_or(true),
        bonus_chest: gen_settings.get_bool("bonus_chest").unwrap_or(false),
        world_type,
        generator_settings,
        game_rules,
        experiments: crate::packs::experiments(enabled.iter().map(String::as_str))
            .into_iter()
            .map(str::to_owned)
            .collect(),
        data_packs: crate::packs::list_packs(&world.join("datapacks")),
    })
}

impl Recreate {
    pub fn game_mode(&self) -> GameMode {
        self.level.game_mode
    }

    pub fn difficulty(&self) -> Difficulty {
        self.level.difficulty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::tests::{level_dat, modern, temp_dir};
    use mcr_nbt::{Compression, write_disk};

    fn saved(world: &Path, name: &str, data: Compound) {
        let dir = world.join("data/minecraft");
        std::fs::create_dir_all(&dir).unwrap();
        let mut root = Compound::new();
        root.insert("data", Value::Compound(data));
        root.insert("DataVersion", Value::Int(4790));
        std::fs::write(
            dir.join(format!("{name}.dat")),
            write_disk("", &root, Compression::Gzip).unwrap(),
        )
        .unwrap();
    }

    fn gen_settings(generator: Compound) -> Compound {
        let mut overworld = Compound::new();
        overworld.insert("type", "minecraft:overworld");
        overworld.insert("generator", Value::Compound(generator));
        let mut dims = Compound::new();
        dims.insert("minecraft:overworld", Value::Compound(overworld));
        let mut wgen = Compound::new();
        wgen.insert("seed", Value::Long(42));
        wgen.insert("generate_features", Value::Byte(0));
        wgen.insert("bonus_chest", Value::Byte(1));
        wgen.insert("dimensions", Value::Compound(dims));
        wgen
    }

    fn noise(settings: &str, biome_source: &str) -> Compound {
        let mut g = Compound::new();
        g.insert("type", "minecraft:noise");
        g.insert("settings", settings);
        let mut b = Compound::new();
        b.insert("type", biome_source);
        g.insert("biome_source", Value::Compound(b));
        g
    }

    #[test]
    fn modern_amplified_world_with_rules_and_packs() {
        let world = temp_dir("recreate-modern");
        let mut data = modern("Peaks", 1);
        let mut packs = Compound::new();
        packs.insert(
            "Enabled",
            Value::List(List::String(vec![
                "vanilla".into(),
                "trade_rebalance".into(),
                "file/x.zip".into(),
            ])),
        );
        data.insert("DataPacks", Value::Compound(packs));
        std::fs::write(world.join("level.dat"), level_dat(data)).unwrap();
        saved(
            &world,
            "world_gen_settings",
            gen_settings(noise("minecraft:amplified", "minecraft:multi_noise")),
        );
        let mut rules = Compound::new();
        rules.insert("minecraft:keep_inventory", Value::Byte(1));
        rules.insert("minecraft:random_tick_speed", Value::Int(3));
        rules.insert("minecraft:bogus", Value::Int(3));
        saved(&world, "game_rules", rules);
        std::fs::create_dir_all(world.join("datapacks")).unwrap();
        std::fs::write(world.join("datapacks/x.zip"), b"PK\x03\x04").unwrap();
        let r = read(&world).unwrap();
        assert_eq!(r.level.name, "Peaks");
        assert_eq!(
            (r.seed.as_str(), r.structures, r.bonus_chest),
            ("42", false, true)
        );
        assert_eq!(r.world_type, WorldType::Amplified);
        assert_eq!(
            r.game_rules,
            [("keep_inventory".to_owned(), "true".to_owned())]
        );
        assert_eq!(r.experiments, ["trade_rebalance"]);
        assert_eq!(r.data_packs, ["x.zip"]);
        let _ = std::fs::remove_dir_all(&world);
    }

    #[test]
    fn old_flat_world_in_level_dat() {
        let world = temp_dir("recreate-flat");
        let mut layer = Compound::new();
        layer.insert("block", "minecraft:bedrock");
        layer.insert("height", Value::Int(1));
        let mut settings = Compound::new();
        settings.insert("layers", Value::List(List::Compound(vec![layer])));
        settings.insert("biome", "minecraft:desert");
        let mut g = Compound::new();
        g.insert("type", "minecraft:flat");
        g.insert("settings", Value::Compound(settings));
        let mut data = modern("Flat", 1);
        data.insert("WorldGenSettings", Value::Compound(gen_settings(g)));
        std::fs::write(world.join("level.dat"), level_dat(data)).unwrap();
        let r = read(&world).unwrap();
        assert_eq!(r.world_type, WorldType::Flat);
        assert_eq!(
            crate::flat::preset_string(r.generator_settings.as_deref().unwrap()).unwrap(),
            "minecraft:bedrock;minecraft:desert"
        );
        let _ = std::fs::remove_dir_all(&world);
    }

    #[test]
    fn unsupported_generators_are_refused() {
        for (tag, generator) in [
            ("single", noise("minecraft:overworld", "minecraft:fixed")),
            ("caves", noise("minecraft:caves", "minecraft:multi_noise")),
            ("debug", {
                let mut g = Compound::new();
                g.insert("type", "minecraft:debug");
                g
            }),
        ] {
            let world = temp_dir(&format!("recreate-{tag}"));
            std::fs::write(world.join("level.dat"), level_dat(modern("W", 1))).unwrap();
            saved(&world, "world_gen_settings", gen_settings(generator));
            assert!(matches!(read(&world), Err(SpError::Refused(_))), "{tag}");
            let _ = std::fs::remove_dir_all(&world);
        }
        let empty = temp_dir("recreate-none");
        assert!(read(&empty).is_err());
        let _ = std::fs::remove_dir_all(&empty);
    }
}
