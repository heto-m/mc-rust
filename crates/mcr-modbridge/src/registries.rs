//! Java's static registries (ABI 21): Rust rebuilds its tables from them and makes them active, so it
//! decodes chunks, items and entities exactly as Java does after the loader's registry sync.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use mcr_data::{
    Aabb, Export, ExportBlock, ExportEntityType, ExportFluid, ExportFluidState, ExportItem,
    ExportState, ExportTool, Version,
};
use mcr_protocol::types::modded::{ModdedCodecs, Shape};
use serde_json::Value;

/// Java's report for a 134-mod pack is a few MiB.
pub const MAX_REPORT_BYTES: usize = 64 << 20;
const MAX_PROBLEMS: usize = 64;
/// A directory where every report Java sends is written, for debugging.
pub const DUMP_ENV: &str = "MCR_REGISTRY_DUMP";

#[derive(Debug, Default)]
pub(crate) struct RegistryShared {
    latest: Mutex<Option<Arc<Vec<u8>>>>,
    generation: AtomicU64,
}

impl RegistryShared {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn store(&self, bytes: Vec<u8>) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(bytes));
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

pub(crate) extern "C" fn registry_report(ptr: i64, len: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Ok(len) = usize::try_from(len) else {
            return;
        };
        if ptr == 0 || len == 0 || len > MAX_REPORT_BYTES {
            tracing::warn!(len, "registry report refused");
            return;
        }
        // SAFETY: Java passes a live confined-arena segment of `len` bytes for the duration of this downcall.
        let bytes = unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len) }.to_vec();
        if let Some(dir) = std::env::var_os(DUMP_ENV) {
            let name = if is_configuration(&bytes) {
                "configuration"
            } else {
                "title"
            };
            let _ = std::fs::write(
                std::path::Path::new(&dir).join(format!("registries-{name}.json")),
                &bytes,
            );
        }
        if let Some(shared) = crate::abi::shared_now() {
            shared.registries.store(bytes);
        }
    }));
}

/// Java's render report (fluid models, and the quads of blocks Rust asked for with `MSG_MODELS`).
pub(crate) extern "C" fn render_report(ptr: i64, len: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Ok(len) = usize::try_from(len) else {
            return;
        };
        if ptr == 0 || len == 0 || len > MAX_REPORT_BYTES {
            tracing::warn!(len, "render report refused");
            return;
        }
        // SAFETY: Java passes a live confined-arena segment of `len` bytes for the duration of this downcall.
        let bytes = unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len) }.to_vec();
        if let Some(shared) = crate::abi::shared_now() {
            shared.render.store(bytes);
        }
    }));
}

/// The newest render report and its generation (counts every report Java sent).
pub fn render_latest() -> Option<(u64, Arc<Vec<u8>>)> {
    let shared = crate::abi::shared_now()?;
    let latest = shared
        .render
        .latest
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()?;
    Some((shared.render.generation.load(Ordering::Acquire), latest))
}

/// How many reports Java has sent this process.
pub fn generation() -> u64 {
    crate::abi::shared_now().map_or(0, |s| s.registries.generation.load(Ordering::Acquire))
}

/// The newest report and its generation.
pub fn latest() -> Option<(u64, Arc<Vec<u8>>)> {
    let shared = crate::abi::shared_now()?;
    let latest = shared
        .registries
        .latest
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()?;
    Some((shared.registries.generation.load(Ordering::Acquire), latest))
}

/// Java writes the reason first, so the end-of-configuration report is told apart without parsing.
pub fn is_configuration(bytes: &[u8]) -> bool {
    bytes.starts_with(br#"{"format":2,"reason":"configuration""#)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub reason: String,
    /// `Block.BLOCK_STATE_REGISTRY.size()` in Java.
    pub state_count: usize,
    pub export: Export,
    pub problems: Vec<String>,
    /// Wire shapes of modded data component types, particle types and entity data serializers.
    pub codecs: ModdedCodecs,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReportError {
    #[error("registry report is not valid JSON: {0}")]
    Json(String),
    #[error("registry report: {0}")]
    Shape(String),
    #[error(transparent)]
    Rebuild(#[from] mcr_data::RebuildError),
    #[error("Java decodes {java} block states, the rebuilt tables hold {rust}")]
    StateCount { java: usize, rust: usize },
    #[error(transparent)]
    Slots(#[from] mcr_data::SlotsFull),
}

fn shape(what: &str) -> ReportError {
    ReportError::Shape(what.to_owned())
}

fn s(v: &Value, key: &str) -> Result<String, ReportError> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| shape(&format!("missing {key}")))
}

fn f(v: &Value, key: &str) -> Result<f32, ReportError> {
    v[key]
        .as_f64()
        .map(|x| x as f32)
        .ok_or_else(|| shape(&format!("missing {key}")))
}

fn u(v: &Value, key: &str) -> Result<u64, ReportError> {
    v[key]
        .as_u64()
        .ok_or_else(|| shape(&format!("missing {key}")))
}

fn small(v: &Value, key: &str) -> Result<u8, ReportError> {
    u8::try_from(u(v, key)?.min(255)).map_err(|_| shape(key))
}

fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, ReportError> {
    v[key]
        .as_array()
        .ok_or_else(|| shape(&format!("missing {key}")))
}

fn shape_boxes(v: &Value) -> Result<Vec<Aabb>, ReportError> {
    v.as_array()
        .ok_or_else(|| shape("shape"))?
        .iter()
        .map(|b| {
            let n: Vec<f32> = b
                .as_array()
                .filter(|a| a.len() == 6)
                .ok_or_else(|| shape("box"))?
                .iter()
                .map(|x| {
                    x.as_f64()
                        .map(|x| x as f32)
                        .ok_or_else(|| shape("box value"))
                })
                .collect::<Result<_, _>>()?;
            Ok(Aabb {
                min: [n[0], n[1], n[2]],
                max: [n[3], n[4], n[5]],
            })
        })
        .collect()
}

fn fluid_state(v: &Value) -> Result<ExportFluidState, ReportError> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 4)
        .ok_or_else(|| shape("fluid state"))?;
    Ok(ExportFluidState {
        fluid: a[0].as_str().ok_or_else(|| shape("fluid name"))?.to_owned(),
        amount: a[1].as_u64().map_or(0, |x| x.min(15) as u8),
        source: a[2].as_bool().unwrap_or(false),
        falling: a[3].as_bool().unwrap_or(false),
    })
}

/// `[collision, outline, occlusion, light, light block, flags, map colour, sturdy, occluding faces, fluid]`.
fn state(v: &Value) -> Result<ExportState, ReportError> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 10)
        .ok_or_else(|| shape("state"))?;
    let n = |i: usize| a[i].as_u64().ok_or_else(|| shape("state field"));
    let index = |i: usize| n(i).and_then(|x| u32::try_from(x).map_err(|_| shape("index")));
    let byte = |i: usize| n(i).map(|x| x.min(255) as u8);
    Ok(ExportState {
        collision: index(0)?,
        outline: index(1)?,
        occlusion: index(2)?,
        light_emission: byte(3)?,
        light_block: byte(4)?,
        flags: (n(5)? & 0xffff) as u16,
        map_color: byte(6)?,
        sturdy_faces: byte(7)?,
        occlusion_faces: byte(8)?,
        fluid: match a[9].as_i64().ok_or_else(|| shape("state fluid"))? {
            i if i < 0 => None,
            i => Some(u32::try_from(i).map_err(|_| shape("fluid index"))?),
        },
    })
}

fn block(v: &Value) -> Result<ExportBlock, ReportError> {
    let properties = array(v, "properties")?
        .iter()
        .map(|p| {
            let pair = p
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| shape("property"))?;
            let name = pair[0].as_str().ok_or_else(|| shape("property name"))?;
            let values = pair[1]
                .as_array()
                .ok_or_else(|| shape("property values"))?
                .iter()
                .map(|x| x.as_str().map(str::to_owned).ok_or_else(|| shape("value")))
                .collect::<Result<_, _>>()?;
            Ok((name.to_owned(), values))
        })
        .collect::<Result<_, ReportError>>()?;
    Ok(ExportBlock {
        friction: f(v, "friction")?,
        speed_factor: f(v, "speed_factor")?,
        jump_factor: f(v, "jump_factor")?,
        destroy_time: f(v, "destroy_time")?,
        requires_tool: v["requires_tool"].as_bool().unwrap_or(false),
        explosion_resistance: f(v, "explosion_resistance")?,
        sound_type: s(v, "sound_type")?,
        properties,
        default_state: u32::try_from(u(v, "default_state")?).map_err(|_| shape("default_state"))?,
        states: array(v, "states")?
            .iter()
            .map(state)
            .collect::<Result<_, _>>()?,
    })
}

fn item(v: &Value) -> Result<ExportItem, ReportError> {
    let tool = match v.get("tool") {
        None | Some(Value::Null) => None,
        Some(t) => Some(ExportTool {
            default_mining_speed: f(t, "default_mining_speed")?,
            damage_per_block: small(t, "damage_per_block")?,
            can_destroy_blocks_in_creative: t["can_destroy_blocks_in_creative"]
                .as_bool()
                .unwrap_or(true),
            rules: array(t, "rules")?
                .iter()
                .map(|r| {
                    let r = r
                        .as_array()
                        .filter(|a| a.len() == 3)
                        .ok_or_else(|| shape("rule"))?;
                    Ok((
                        r[0].as_str()
                            .ok_or_else(|| shape("rule blocks"))?
                            .to_owned(),
                        r[1].as_f64().map(|x| x as f32),
                        r[2].as_bool(),
                    ))
                })
                .collect::<Result<_, ReportError>>()?,
        }),
    };
    Ok(ExportItem {
        max_stack_size: u16::try_from(u(v, "max_stack_size")?.min(99)).unwrap_or(64),
        max_damage: u32::try_from(u(v, "max_damage")?).unwrap_or(0),
        block: v["block"].as_str().map(str::to_owned),
        tool,
        equip_slot: v["equip_slot"].as_u64().map_or(255, |s| s.min(255) as u8),
        consume_seconds: v["consume_seconds"].as_f64().map(|x| x as f32),
        cooldown_seconds: v["cooldown_seconds"].as_f64().map(|x| x as f32),
        food: v["food"].as_bool(),
        default_components: v["default_components"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_u64)
                    .filter(|&c| c < 128)
                    .map(|c| c as u32)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn parse(bytes: &[u8]) -> Result<Report, ReportError> {
    if bytes.len() > MAX_REPORT_BYTES {
        return Err(shape("too large"));
    }
    let root: Value =
        serde_json::from_slice(bytes).map_err(|e| ReportError::Json(e.to_string()))?;
    if root["format"].as_u64() != Some(2) {
        return Err(shape("unknown format"));
    }
    let registries = root["registries"]
        .as_object()
        .ok_or_else(|| shape("missing registries"))?
        .iter()
        .map(|(name, entries)| {
            let entries = entries
                .as_array()
                .ok_or_else(|| shape("registry entries"))?
                .iter()
                .map(|e| e.as_str().map(str::to_owned).ok_or_else(|| shape("entry")))
                .collect::<Result<_, _>>()?;
            Ok((name.clone(), entries))
        })
        .collect::<Result<_, ReportError>>()?;
    let export = Export {
        registries,
        blocks: array(&root, "blocks")?
            .iter()
            .map(block)
            .collect::<Result<_, _>>()?,
        items: array(&root, "items")?
            .iter()
            .map(item)
            .collect::<Result<_, _>>()?,
        entity_types: array(&root, "entity_types")?
            .iter()
            .map(|e| {
                Ok(ExportEntityType {
                    width: f(e, "width")?,
                    height: f(e, "height")?,
                    eye_height: f(e, "eye_height")?,
                    category: s(e, "category")?,
                })
            })
            .collect::<Result<_, ReportError>>()?,
        fluids: array(&root, "fluids")?
            .iter()
            .map(|fl| ExportFluid {
                source: fl["source"].as_bool().unwrap_or(false),
            })
            .collect(),
        shapes: array(&root, "shapes")?
            .iter()
            .map(shape_boxes)
            .collect::<Result<_, _>>()?,
        fluid_states: array(&root, "fluid_states")?
            .iter()
            .map(fluid_state)
            .collect::<Result<_, _>>()?,
    };
    Ok(Report {
        reason: root["reason"]
            .as_str()
            .unwrap_or_default()
            .chars()
            .take(32)
            .collect(),
        state_count: usize::try_from(u(&root, "state_count")?).map_err(|_| shape("state_count"))?,
        export,
        codecs: codecs(&root["codecs"]),
        problems: root["problems"]
            .as_array()
            .map(|a| {
                a.iter()
                    .take(MAX_PROBLEMS)
                    .filter_map(Value::as_str)
                    .map(|p| p.chars().take(200).collect())
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Whether `export` names vanilla's entries with vanilla's state layout: a modded client whose mods
/// register nothing, so vanilla's tables serve it.
fn is_vanilla(export: &Export, vanilla: &Version) -> bool {
    let same_names = export.registries.iter().all(|(name, entries)| {
        vanilla.registry(name).is_some_and(|r| {
            r.len() == entries.len() && r.iter().zip(entries).all(|((_, a), b)| a == b)
        })
    });
    same_names
        && export.blocks.len() == vanilla.block_count()
        && export.blocks.iter().enumerate().all(|(id, b)| {
            vanilla.block_info(id as u32).is_some_and(|info| {
                (info.states.1 - info.states.0) as usize == b.states.len()
                    && info.default_state - info.states.0 == b.default_state
            })
        })
}

/// Modded registry ids above the vanilla ones a report may describe per registry.
const MAX_MODDED_CODECS: usize = 1 << 16;

fn codecs(v: &Value) -> ModdedCodecs {
    let table = |key: &str| -> HashMap<i32, Option<Shape>> {
        v[key]
            .as_object()
            .map(|o| {
                o.iter()
                    .take(MAX_MODDED_CODECS)
                    .filter_map(|(id, shape)| {
                        let id = id.parse::<i32>().ok().filter(|&i| i >= 0)?;
                        Some((id, shape.as_str().and_then(|t| Shape::parse(t).ok())))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    ModdedCodecs {
        components: table("data_component_type"),
        particles: table("particle_type"),
        entity_data: table("entity_data_serializer"),
    }
}

static INSTALLED: Mutex<Option<HashMap<u64, &'static Version>>> = Mutex::new(None);

/// Rebuilds vanilla's tables from `bytes` (once per distinct report) and makes them active.
pub fn install(bytes: &[u8]) -> Result<(&'static Version, Report), ReportError> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let key = hasher.finish();
    let report = parse(bytes)?;
    let mut installed = INSTALLED.lock().unwrap_or_else(PoisonError::into_inner);
    let cache = installed.get_or_insert_with(HashMap::new);
    let vanilla = mcr_data::vanilla_26_3();
    let version = match cache.get(&key) {
        Some(v) => *v,
        None if is_vanilla(&report.export, vanilla) => vanilla,
        None => {
            let v = vanilla.rebuild(&report.export)?;
            if v.state_count() != report.state_count {
                return Err(ReportError::StateCount {
                    java: report.state_count,
                    rust: v.state_count(),
                });
            }
            let v = mcr_data::install(v)?;
            cache.insert(key, v);
            v
        }
    };
    if version.state_count() != report.state_count {
        return Err(ReportError::StateCount {
            java: report.state_count,
            rust: version.state_count(),
        });
    }
    mcr_data::activate(version);
    let modded = !(report.codecs.components.is_empty()
        && report.codecs.particles.is_empty()
        && report.codecs.entity_data.is_empty());
    mcr_protocol::types::modded::install(modded.then(|| report.codecs.clone()));
    Ok((version, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(reg: &str) -> Vec<String> {
        mcr_data::vanilla_26_3()
            .registry(reg)
            .unwrap()
            .iter()
            .map(|(_, n)| n.to_owned())
            .collect()
    }

    fn block_json(name: &str, b: &ExportBlock) -> String {
        let states: Vec<String> = b
            .states
            .iter()
            .map(|s| {
                format!(
                    "[{},{},{},{},{},{},{},{},{},{}]",
                    s.collision,
                    s.outline,
                    s.occlusion,
                    s.light_emission,
                    s.light_block,
                    s.flags,
                    s.map_color,
                    s.sturdy_faces,
                    s.occlusion_faces,
                    s.fluid.map_or(-1, i64::from)
                )
            })
            .collect();
        format!(
            r#"{{"name":{},"friction":{},"speed_factor":{},"jump_factor":{},"destroy_time":{},"requires_tool":{},"explosion_resistance":{},"sound_type":{},"properties":{},"default_state":{},"states":[{}]}}"#,
            serde_json::to_string(name).unwrap(),
            b.friction,
            b.speed_factor,
            b.jump_factor,
            b.destroy_time,
            b.requires_tool,
            b.explosion_resistance,
            serde_json::to_string(&b.sound_type).unwrap(),
            serde_json::to_string(&b.properties).unwrap(),
            b.default_state,
            states.join(",")
        )
    }

    /// Vanilla as Java would report it, optionally with one modded lamp block and item.
    fn report_json(extra_block: bool) -> String {
        let v = mcr_data::vanilla_26_3();
        let e = v.export();
        let mut blocks = names("minecraft:block");
        let mut items = names("minecraft:item");
        let mut state_count = v.state_count();
        let mut block_json: Vec<String> = blocks
            .iter()
            .zip(&e.blocks)
            .map(|(n, b)| block_json(n, b))
            .collect();
        let mut item_json: Vec<String> = items
            .iter()
            .map(|n| {
                format!(
                    r#"{{"name":{},"max_stack_size":64,"max_damage":0}}"#,
                    serde_json::to_string(n).unwrap()
                )
            })
            .collect();
        if extra_block {
            blocks.push("test:lamp".into());
            items.push("test:lamp".into());
            state_count += 2;
            let st = r#"[1,1,0,15,0,80,3,63,0,-1]"#;
            block_json.push(format!(
                r#"{{"name":"test:lamp","friction":0.6,"speed_factor":1,"jump_factor":1,"destroy_time":0.3,"requires_tool":false,"explosion_resistance":0.3,"sound_type":"minecraft:block.glass.break","properties":[["lit",["true","false"]]],"default_state":1,"states":[{st},{st}]}}"#
            ));
            item_json.push(r##"{"name":"test:lamp","max_stack_size":64,"max_damage":0,"block":"test:lamp","tool":{"default_mining_speed":1.0,"damage_per_block":1,"can_destroy_blocks_in_creative":true,"rules":[["#minecraft:mineable/pickaxe",6.0,true]]},"default_components":[1,2,300]}"##.into());
        }
        let entities: Vec<String> = e
            .entity_types
            .iter()
            .map(|t| {
                format!(
                    r#"{{"width":{},"height":{},"eye_height":{},"category":{}}}"#,
                    t.width,
                    t.height,
                    t.eye_height,
                    serde_json::to_string(&t.category).unwrap()
                )
            })
            .collect();
        let fluids: Vec<String> = e
            .fluids
            .iter()
            .map(|f| format!(r#"{{"source":{}}}"#, f.source))
            .collect();
        let shapes: Vec<String> = e
            .shapes
            .iter()
            .map(|s| {
                let boxes: Vec<String> = s
                    .iter()
                    .map(|b| {
                        format!(
                            "[{},{},{},{},{},{}]",
                            b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]
                        )
                    })
                    .collect();
                format!("[{}]", boxes.join(","))
            })
            .collect();
        let fluid_states: Vec<String> = e
            .fluid_states
            .iter()
            .map(|f| {
                format!(
                    r#"[{},{},{},{}]"#,
                    serde_json::to_string(&f.fluid).unwrap(),
                    f.amount,
                    f.source,
                    f.falling
                )
            })
            .collect();
        format!(
            r#"{{"format":2,"reason":"title","state_count":{state_count},"registries":{{"minecraft:block":{},"minecraft:item":{},"minecraft:entity_type":{},"minecraft:fluid":{}}},"blocks":[{}],"items":[{}],"entity_types":[{}],"fluids":[{}],"shapes":[{}],"fluid_states":[{}],"problems":["x"]}}"#,
            serde_json::to_string(&blocks).unwrap(),
            serde_json::to_string(&items).unwrap(),
            serde_json::to_string(&names("minecraft:entity_type")).unwrap(),
            serde_json::to_string(&names("minecraft:fluid")).unwrap(),
            block_json.join(","),
            item_json.join(","),
            entities.join(","),
            fluids.join(","),
            shapes.join(","),
            fluid_states.join(","),
        )
    }

    #[test]
    fn parses_and_installs_a_modded_report() {
        let text = report_json(true);
        let report = parse(text.as_bytes()).unwrap();
        let lamp = report.export.items.last().unwrap();
        assert_eq!(lamp.tool.as_ref().unwrap().rules.len(), 1);
        assert_eq!(lamp.default_components, [1, 2]);
        assert_eq!(report.problems, ["x"]);
        let (v, _) = install(text.as_bytes()).unwrap();
        assert!(v.block_by_name("test:lamp").is_some());
        assert!(std::ptr::eq(mcr_data::v26_3(), v));
        let (again, _) = install(text.as_bytes()).unwrap();
        assert!(
            std::ptr::eq(v, again),
            "a repeated report reuses its tables"
        );
        let (vanilla, _) = install(report_json(false).as_bytes()).unwrap();
        assert!(std::ptr::eq(vanilla, mcr_data::vanilla_26_3()));
        assert!(std::ptr::eq(mcr_data::v26_3(), vanilla));
    }

    #[test]
    fn wrong_state_count_is_refused() {
        let text = report_json(true).replace(
            &format!(
                "\"state_count\":{}",
                mcr_data::vanilla_26_3().state_count() + 2
            ),
            "\"state_count\":7",
        );
        assert!(matches!(
            install(text.as_bytes()),
            Err(ReportError::StateCount { java: 7, .. })
        ));
    }

    #[test]
    fn hostile_reports_fail_cleanly() {
        for bad in [
            "",
            "[]",
            r#"{"format":1}"#,
            r#"{"format":2,"state_count":1,"registries":{"minecraft:block":[1]},"blocks":[],"items":[],"entity_types":[],"fluids":[],"shapes":[],"fluid_states":[]}"#,
            r#"{"format":2,"state_count":1,"registries":{},"blocks":[{"name":"x"}],"items":[],"entity_types":[],"fluids":[],"shapes":[],"fluid_states":[]}"#,
        ] {
            assert!(parse(bad.as_bytes()).is_err(), "{bad}");
        }
        let deep = "[".repeat(10_000);
        assert!(parse(deep.as_bytes()).is_err());
        let text = report_json(true).replacen("[1,1,0,15,0,80,3,63,0,-1]", "[1,1,0,15]", 1);
        assert!(parse(text.as_bytes()).is_err());
    }

    #[test]
    fn report_flags_match_the_table_bits() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../java/mcr-bridge/src/dev/mcr/bridge/BridgeAbi.java"),
        )
        .unwrap();
        let get = |name: &str| -> u16 {
            let decl = format!("static final int REG_FLAG_{name} = ");
            let start = src.find(&decl).unwrap() + decl.len();
            src[start..start + src[start..].find(';').unwrap()]
                .trim()
                .parse()
                .unwrap()
        };
        use mcr_data::flag;
        assert_eq!(get("AIR"), flag::AIR);
        assert_eq!(get("LIQUID"), flag::LIQUID);
        assert_eq!(get("REPLACEABLE"), flag::REPLACEABLE);
        assert_eq!(get("HAS_BLOCK_ENTITY"), flag::HAS_BLOCK_ENTITY);
        assert_eq!(get("CAN_OCCLUDE"), flag::CAN_OCCLUDE);
        assert_eq!(
            get("USE_SHAPE_FOR_LIGHT_OCCLUSION"),
            flag::USE_SHAPE_FOR_LIGHT_OCCLUSION
        );
        assert_eq!(get("SOLID_RENDER"), flag::SOLID_RENDER);
        assert_eq!(get("FULL_COLLISION"), flag::FULL_COLLISION);
        assert_eq!(
            get("PROPAGATES_SKYLIGHT_DOWN"),
            flag::PROPAGATES_SKYLIGHT_DOWN
        );
        assert_eq!(get("REDSTONE_CONDUCTOR"), flag::REDSTONE_CONDUCTOR);
    }
}
