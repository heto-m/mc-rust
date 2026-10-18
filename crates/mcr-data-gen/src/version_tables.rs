//! Packet and registry id tables between an older version and the canonical one, by name.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};

#[path = "../../mcr-protocol/src/version/tables.rs"]
pub mod format;

use format::{ABSENT, IdMap, VersionTables};

use crate::read_json;

/// One version's `generated/reports` plus its name and protocol number.
#[derive(Debug, Clone)]
pub struct Reports<'a> {
    pub dir: &'a Path,
    pub name: &'a str,
    pub protocol: u32,
}

pub fn version_tables(old: &Reports<'_>, new: &Reports<'_>) -> Result<Vec<u8>> {
    let mut maps = BTreeMap::new();
    let old_packets = read_json(&old.dir.join("packets.json"))?;
    packet_maps(
        &old_packets,
        &read_json(&new.dir.join("packets.json"))?,
        &mut maps,
    )?;
    let mut names = Vec::new();
    for key in maps.keys() {
        let mut parts = key.split('/').skip(1);
        let (Some(state), Some(dir)) = (parts.next(), parts.next()) else {
            continue;
        };
        names.push((
            key.clone(),
            dense(&packet_ids(&old_packets, state, dir)?, key)?,
        ));
    }
    registry_maps(
        &read_json(&old.dir.join("registries.json"))?,
        &read_json(&new.dir.join("registries.json"))?,
        &mut maps,
    )?;
    let states = block_state_map(
        &read_json(&old.dir.join("blocks.json"))?,
        &read_json(&new.dir.join("blocks.json"))?,
    )?;
    if let Some(states) = states {
        maps.insert("block_state".to_owned(), states);
    }
    Ok(VersionTables {
        old_name: old.name.to_owned(),
        old_protocol: old.protocol,
        new_name: new.name.to_owned(),
        new_protocol: new.protocol,
        maps: maps.into_iter().collect(),
        names,
    }
    .encode())
}

fn dense(map: &HashMap<String, usize>, what: &str) -> Result<Vec<String>> {
    let mut by_id = vec![None; map.len()];
    for (name, &id) in map {
        ensure!(
            id < by_id.len() && by_id[id].is_none(),
            "{what}: ids not dense"
        );
        by_id[id] = Some(name.clone());
    }
    Ok(by_id.into_iter().map(Option::unwrap).collect())
}

fn id16(id: usize, what: &str) -> Result<u16> {
    u16::try_from(id)
        .ok()
        .filter(|&v| v != ABSENT)
        .with_context(|| format!("{what}: id {id} does not fit the table format"))
}

/// `None` when both sides name the same ids.
fn name_map(
    old: &HashMap<String, usize>,
    new: &HashMap<String, usize>,
    what: &str,
) -> Result<Option<IdMap>> {
    if old == new {
        return Ok(None);
    }
    let old_names = dense(old, what)?;
    let new_names = dense(new, what)?;
    let lookup = |names: &[String], other: &HashMap<String, usize>| -> Result<Vec<u16>> {
        names
            .iter()
            .map(|n| other.get(n).map_or(Ok(ABSENT), |&id| id16(id, what)))
            .collect()
    };
    Ok(Some(IdMap {
        forward: lookup(&old_names, new)?,
        back: lookup(&new_names, old)?,
    }))
}

fn packet_ids(
    report: &Map<String, Value>,
    state: &str,
    dir: &str,
) -> Result<HashMap<String, usize>> {
    let Some(packets) = report.get(state).and_then(|s| s.get(dir)) else {
        return Ok(HashMap::new());
    };
    packets
        .as_object()
        .context("packet set")?
        .iter()
        .map(|(name, v)| {
            let id = v["protocol_id"].as_u64().context("protocol_id")?;
            Ok((name.clone(), id as usize))
        })
        .collect()
}

fn packet_maps(
    old: &Map<String, Value>,
    new: &Map<String, Value>,
    out: &mut BTreeMap<String, IdMap>,
) -> Result<()> {
    for state in ["handshake", "status", "login", "configuration", "play"] {
        for dir in ["clientbound", "serverbound"] {
            let what = format!("{state} {dir} packets");
            let (o, n) = (packet_ids(old, state, dir)?, packet_ids(new, state, dir)?);
            if let Some(map) = name_map(&o, &n, &what)? {
                out.insert(VersionTables::packet_key(state, dir), map);
            }
        }
    }
    Ok(())
}

fn registry_ids(reg: &Value) -> Result<HashMap<String, usize>> {
    reg["entries"]
        .as_object()
        .context("registry entries")?
        .iter()
        .map(|(name, v)| {
            let id = v["protocol_id"].as_u64().context("protocol_id")?;
            Ok((name.clone(), id as usize))
        })
        .collect()
}

fn registry_maps(
    old: &Map<String, Value>,
    new: &Map<String, Value>,
    out: &mut BTreeMap<String, IdMap>,
) -> Result<()> {
    let empty = Value::Object(Map::from_iter([(
        "entries".to_owned(),
        Value::Object(Map::new()),
    )]));
    let mut names: Vec<&String> = old.keys().chain(new.keys()).collect();
    names.sort();
    names.dedup();
    for name in names {
        let o = registry_ids(old.get(name).unwrap_or(&empty))?;
        let n = registry_ids(new.get(name).unwrap_or(&empty))?;
        if let Some(map) = name_map(&o, &n, name)? {
            out.insert(VersionTables::registry_key(name), map);
        }
    }
    Ok(())
}

struct BlockStates {
    by_key: HashMap<(String, Vec<(String, String)>), usize>,
    default_of: HashMap<String, usize>,
    keys: Vec<(String, Vec<(String, String)>)>,
}

fn block_states(report: &Map<String, Value>) -> Result<BlockStates> {
    let mut by_key = HashMap::new();
    let mut default_of = HashMap::new();
    for (block, info) in report {
        for state in info["states"].as_array().context("block states")? {
            let id = state["id"].as_u64().context("state id")? as usize;
            let mut props: Vec<(String, String)> = state
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect();
            props.sort();
            if state.get("default").and_then(Value::as_bool) == Some(true) {
                default_of.insert(block.clone(), id);
            }
            ensure!(
                by_key.insert((block.clone(), props), id).is_none(),
                "{block}: duplicate state"
            );
        }
    }
    let mut keys = vec![None; by_key.len()];
    for (key, &id) in &by_key {
        ensure!(
            id < keys.len() && keys[id].is_none(),
            "block states not dense"
        );
        keys[id] = Some(key.clone());
    }
    Ok(BlockStates {
        keys: keys.into_iter().map(Option::unwrap).collect(),
        by_key,
        default_of,
    })
}

/// States match by block and property values; a state whose block exists on the other side but whose
/// property values do not falls back to that block's default state.
fn block_state_map(old: &Map<String, Value>, new: &Map<String, Value>) -> Result<Option<IdMap>> {
    let (o, n) = (block_states(old)?, block_states(new)?);
    if o.by_key == n.by_key {
        return Ok(None);
    }
    let lookup = |from: &BlockStates, to: &BlockStates| -> Result<Vec<u16>> {
        from.keys
            .iter()
            .map(|key| {
                let id = to
                    .by_key
                    .get(key)
                    .or_else(|| to.default_of.get(&key.0))
                    .copied();
                id.map_or(Ok(ABSENT), |id| id16(id, "block states"))
            })
            .collect()
    };
    Ok(Some(IdMap {
        forward: lookup(&o, &n)?,
        back: lookup(&n, &o)?,
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn packets_match_by_name() {
        let old = obj(json!({"play": {"clientbound": {
            "minecraft:a": {"protocol_id": 0}, "minecraft:gone": {"protocol_id": 1}, "minecraft:b": {"protocol_id": 2}
        }}}));
        let new = obj(json!({"play": {"clientbound": {
            "minecraft:new": {"protocol_id": 0}, "minecraft:a": {"protocol_id": 1}, "minecraft:b": {"protocol_id": 2}
        }}}));
        let mut maps = BTreeMap::new();
        packet_maps(&old, &new, &mut maps).unwrap();
        let map = &maps["packets/play/clientbound"];
        assert_eq!(map.forward, [1, ABSENT, 2]);
        assert_eq!(map.back, [ABSENT, 0, 2]);
        assert_eq!(maps.len(), 1);
    }

    #[test]
    fn identical_registries_are_omitted() {
        let reg = obj(json!({
            "minecraft:menu": {"entries": {"minecraft:a": {"protocol_id": 0}}},
            "minecraft:item": {"entries": {"minecraft:a": {"protocol_id": 0}, "minecraft:b": {"protocol_id": 1}}}
        }));
        let mut new = reg.clone();
        new["minecraft:item"] = json!({"entries": {
            "minecraft:c": {"protocol_id": 0}, "minecraft:a": {"protocol_id": 1}, "minecraft:b": {"protocol_id": 2}
        }});
        let mut maps = BTreeMap::new();
        registry_maps(&reg, &new, &mut maps).unwrap();
        assert_eq!(maps.keys().collect::<Vec<_>>(), ["registry/minecraft:item"]);
        assert_eq!(maps["registry/minecraft:item"].forward, [1, 2]);
        assert_eq!(maps["registry/minecraft:item"].back, [ABSENT, 0, 1]);
    }

    #[test]
    fn block_states_match_properties_and_fall_back_to_default() {
        let old = obj(json!({
            "minecraft:air": {"states": [{"id": 0, "default": true}]},
            "minecraft:lamp": {"states": [
                {"id": 1, "properties": {"lit": "true"}},
                {"id": 2, "default": true, "properties": {"lit": "false"}}
            ]}
        }));
        let new = obj(json!({
            "minecraft:air": {"states": [{"id": 0, "default": true}]},
            "minecraft:slab": {"states": [{"id": 1, "default": true}]},
            "minecraft:lamp": {"states": [
                {"id": 2, "properties": {"lit": "true", "on": "x"}},
                {"id": 3, "properties": {"lit": "true"}},
                {"id": 4, "default": true, "properties": {"lit": "false"}}
            ]}
        }));
        let map = block_state_map(&old, &new).unwrap().unwrap();
        assert_eq!(map.forward, [0, 3, 4]);
        assert_eq!(map.back, [0, ABSENT, 2, 1, 2]);
    }

    /// Regenerates every committed table file whose reports are in `.cache`.
    #[test]
    fn committed_tables_are_current() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let Some(cache) = manifest
            .ancestors()
            .map(|p| p.join(".cache"))
            .find(|p| p.is_dir())
        else {
            return;
        };
        let dir = manifest.join("../mcr-protocol/src/version");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path().join("tables.bin");
            let Ok(committed) = std::fs::read(&path) else {
                continue;
            };
            let t = VersionTables::decode(&committed).unwrap();
            let reports = |name: &str| cache.join("versions").join(name).join("generated/reports");
            let (old_dir, new_dir) = (reports(&t.old_name), reports(&t.new_name));
            if !old_dir.join("blocks.json").is_file() || !new_dir.join("blocks.json").is_file() {
                continue;
            }
            let fresh = version_tables(
                &Reports {
                    dir: &old_dir,
                    name: &t.old_name,
                    protocol: t.old_protocol,
                },
                &Reports {
                    dir: &new_dir,
                    name: &t.new_name,
                    protocol: t.new_protocol,
                },
            )
            .unwrap();
            assert!(
                fresh == committed,
                "{} is stale; rerun `cargo xtask version-tables --version {}`",
                path.display(),
                t.old_name
            );
        }
    }

    #[test]
    fn non_dense_ids_fail() {
        let old = obj(json!({"minecraft:item": {"entries": {"minecraft:a": {"protocol_id": 3}}}}));
        let new = obj(json!({"minecraft:item": {"entries": {"minecraft:a": {"protocol_id": 0}}}}));
        assert!(registry_maps(&old, &new, &mut BTreeMap::new()).is_err());
    }
}
