//! Models Java baked, from the mod bridge's render report: fluid models (sprites, layer, tint) and the
//! quads of block states Rust's own baker cannot read (code-registered or custom-loader models).

use std::collections::HashMap;
use std::sync::Arc;

use mcr_assets::Face;
use serde_json::Value;

const MAX_SPRITES: usize = 1 << 16;
const MAX_NAME: usize = 256;
const MAX_FLUIDS: usize = 4096;
const MAX_PACKS: usize = 256;
const MAX_BLOCKS: usize = 4096;
const MAX_STATES: usize = 4096;
const MAX_QUADS_PER_STATE: usize = 4096;
const MAX_QUADS: usize = 1 << 20;
/// Model coordinates and sprite-relative UVs stay within this range (vanilla allows -16..32 pixels).
const COORD_RANGE: std::ops::RangeInclusive<f32> = -16.0..=17.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JavaTint {
    #[default]
    None,
    Grass,
    Foliage,
    DryFoliage,
    Water,
    /// `0xRRGGBB`.
    Rgb(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct JavaFluid {
    /// The source fluid's name: flowing and still forms of one fluid share it.
    pub family: String,
    pub still: Arc<str>,
    pub flow: Arc<str>,
    pub overlay: Option<Arc<str>>,
    pub translucent: bool,
    pub tint: JavaTint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JavaQuad {
    pub pos: [[f32; 3]; 4],
    /// Within the sprite, 0..1.
    pub uv: [[f32; 2]; 4],
    pub normal: Face,
    pub cull: Option<Face>,
    pub tint: i32,
    pub shade: Face,
    pub emission: u8,
    pub sprite: Arc<str>,
    pub translucent: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct JavaState {
    pub ao: bool,
    pub particle: Option<Arc<str>>,
    /// What tint indices 0 and 1 mean for this state.
    pub tints: [JavaTint; 2],
    pub quads: Vec<JavaQuad>,
}

/// Everything Java reported so far; later reports add blocks and replace fluids.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JavaModels {
    /// Java's selected resource packs from its pack folder, lowest priority first.
    pub packs: Vec<String>,
    pub fluids: HashMap<String, JavaFluid>,
    /// Block name to its states by `k=v,...` properties sorted by name.
    pub blocks: HashMap<String, HashMap<String, JavaState>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum JavaModelsError {
    #[error("render report is not valid JSON: {0}")]
    Json(String),
    #[error("render report: {0}")]
    Shape(&'static str),
}

fn bad(what: &'static str) -> JavaModelsError {
    JavaModelsError::Shape(what)
}

fn face(v: &Value) -> Result<Face, JavaModelsError> {
    v.as_u64()
        .and_then(|i| Face::ALL.get(usize::try_from(i).ok()?).copied())
        .ok_or_else(|| bad("direction"))
}

fn tint(v: &Value) -> Result<JavaTint, JavaModelsError> {
    Ok(match v {
        Value::String(s) => match s.as_str() {
            "grass" => JavaTint::Grass,
            "foliage" => JavaTint::Foliage,
            "dry_foliage" => JavaTint::DryFoliage,
            "water" => JavaTint::Water,
            _ => JavaTint::None,
        },
        Value::Number(n) => {
            JavaTint::Rgb(n.as_u64().ok_or_else(|| bad("tint"))? as u32 & 0xff_ffff)
        }
        _ => return Err(bad("tint")),
    })
}

fn name(v: &Value) -> Result<String, JavaModelsError> {
    v.as_str()
        .filter(|s| !s.is_empty() && s.len() <= MAX_NAME)
        .map(str::to_owned)
        .ok_or_else(|| bad("name"))
}

impl JavaModels {
    pub fn is_empty(&self) -> bool {
        self.fluids.is_empty() && self.blocks.is_empty()
    }

    /// Whether Java's quads for `block` are known (possibly none).
    pub fn has_block(&self, block: &str) -> bool {
        self.blocks.contains_key(block)
    }

    /// The state of `block` with `props` (pairs in any order).
    pub fn state(&self, block: &str, props: &[(&str, &str)]) -> Option<&JavaState> {
        let states = self.blocks.get(block)?;
        let mut sorted = props.to_vec();
        sorted.sort_unstable_by(|a, b| a.0.cmp(b.0));
        let key = sorted
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        states.get(&key)
    }

    /// Adds the blocks of `other` and takes its fluids when it has any.
    pub fn merge(&mut self, other: Self) {
        self.packs = other.packs;
        if !other.fluids.is_empty() {
            self.fluids = other.fluids;
        }
        if self.blocks.len() + other.blocks.len() <= MAX_BLOCKS {
            self.blocks.extend(other.blocks);
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, JavaModelsError> {
        let root: Value =
            serde_json::from_slice(bytes).map_err(|e| JavaModelsError::Json(e.to_string()))?;
        if root["format"].as_u64() != Some(1) {
            return Err(bad("unknown format"));
        }
        let sprites: Vec<Arc<str>> = root["sprites"]
            .as_array()
            .ok_or_else(|| bad("sprites"))?
            .iter()
            .take(MAX_SPRITES + 1)
            .map(|v| name(v).map(Arc::from))
            .collect::<Result<_, _>>()?;
        if sprites.len() > MAX_SPRITES {
            return Err(bad("too many sprites"));
        }
        let sprite = |v: &Value| -> Result<Arc<str>, JavaModelsError> {
            v.as_u64()
                .and_then(|i| sprites.get(usize::try_from(i).ok()?))
                .cloned()
                .ok_or_else(|| bad("sprite index"))
        };
        let mut out = Self::default();
        if let Some(packs) = root["packs"].as_array() {
            if packs.len() > MAX_PACKS {
                return Err(bad("too many packs"));
            }
            out.packs = packs
                .iter()
                .map(|p| {
                    p.as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 4096)
                        .map(str::to_owned)
                        .ok_or_else(|| bad("pack"))
                })
                .collect::<Result<_, _>>()?;
        }
        let fluids = root["fluids"].as_array().ok_or_else(|| bad("fluids"))?;
        if fluids.len() > MAX_FLUIDS {
            return Err(bad("too many fluids"));
        }
        for f in fluids {
            let overlay = match f["overlay"].as_i64() {
                Some(-1) => None,
                _ => Some(sprite(&f["overlay"])?),
            };
            out.fluids.insert(
                name(&f["name"])?,
                JavaFluid {
                    family: name(&f["family"])?,
                    still: sprite(&f["still"])?,
                    flow: sprite(&f["flow"])?,
                    overlay,
                    translucent: f["translucent"].as_bool().unwrap_or(false),
                    tint: tint(&f["tint"])?,
                },
            );
        }
        let blocks = root["blocks"].as_array().ok_or_else(|| bad("blocks"))?;
        if blocks.len() > MAX_BLOCKS {
            return Err(bad("too many blocks"));
        }
        let mut total = 0usize;
        for b in blocks {
            let states = b["states"].as_array().ok_or_else(|| bad("states"))?;
            if states.len() > MAX_STATES {
                return Err(bad("too many states"));
            }
            let mut by_props = HashMap::with_capacity(states.len());
            for s in states {
                let quads = s["quads"].as_array().ok_or_else(|| bad("quads"))?;
                if quads.len() > MAX_QUADS_PER_STATE {
                    return Err(bad("too many quads"));
                }
                total += quads.len();
                if total > MAX_QUADS {
                    return Err(bad("too many quads"));
                }
                let tints = s["tints"].as_array().ok_or_else(|| bad("tints"))?;
                let state = JavaState {
                    ao: s["ao"].as_bool().unwrap_or(true),
                    particle: sprite(&s["particle"]).ok(),
                    tints: [
                        tints.first().map_or(Ok(JavaTint::None), tint)?,
                        tints.get(1).map_or(Ok(JavaTint::None), tint)?,
                    ],
                    quads: quads
                        .iter()
                        .map(|q| quad(q, &sprite))
                        .collect::<Result<_, _>>()?,
                };
                let props = s["props"].as_str().ok_or_else(|| bad("props"))?;
                if props.len() > 4096 {
                    return Err(bad("props"));
                }
                by_props.insert(props.to_owned(), state);
            }
            out.blocks.insert(name(&b["name"])?, by_props);
        }
        Ok(out)
    }
}

fn quad(
    q: &Value,
    sprite: &impl Fn(&Value) -> Result<Arc<str>, JavaModelsError>,
) -> Result<JavaQuad, JavaModelsError> {
    let a = q
        .as_array()
        .filter(|a| a.len() == 27)
        .ok_or_else(|| bad("quad"))?;
    let num = |i: usize| -> Result<f32, JavaModelsError> {
        a[i].as_f64()
            .map(|v| v as f32)
            .filter(|v| COORD_RANGE.contains(v))
            .ok_or_else(|| bad("quad coordinate"))
    };
    let mut pos = [[0.0; 3]; 4];
    let mut uv = [[0.0; 2]; 4];
    for v in 0..4 {
        let o = v * 5;
        pos[v] = [num(o)?, num(o + 1)?, num(o + 2)?];
        uv[v] = [num(o + 3)?, num(o + 4)?];
    }
    let cull = match a[21].as_i64() {
        Some(-1) => None,
        _ => Some(face(&a[21])?),
    };
    Ok(JavaQuad {
        pos,
        uv,
        normal: face(&a[20])?,
        cull,
        tint: a[22].as_i64().map_or(-1, |t| t.clamp(-1, 255) as i32),
        shade: face(&a[23])?,
        emission: a[24].as_u64().map_or(0, |e| e.min(15) as u8),
        sprite: sprite(&a[25])?,
        translucent: a[26].as_u64() == Some(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = r#"{"format":1,"packs":["/g/java/resourcepacks/a.zip"],
        "fluids":[{"name":"test:goo","family":"test:goo","still":0,"flow":1,"overlay":-1,"translucent":true,"tint":3355443}],
        "blocks":[{"name":"test:lamp","states":[
            {"props":"facing=north,lit=true","ao":false,"particle":2,"tints":["grass",65280],"quads":[
                [0,0,0,0,0, 0,1,0,0,1, 1,1,0,1,1, 1,0,0,1,0, 2,2,0,2,15,2,0]]},
            {"props":"facing=north,lit=false","ao":true,"particle":2,"tints":["none","none"],"quads":[]}]}],
        "sprites":["test:block/goo_still","test:block/goo_flow","test:block/lamp"]}"#;

    #[test]
    fn parses_fluids_and_block_quads() {
        let m = JavaModels::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(m.packs, ["/g/java/resourcepacks/a.zip"]);
        let goo = &m.fluids["test:goo"];
        assert_eq!(&*goo.flow, "test:block/goo_flow");
        assert_eq!(goo.tint, JavaTint::Rgb(0x333333));
        assert!(goo.overlay.is_none() && goo.translucent);
        let lit = m
            .state("test:lamp", &[("lit", "true"), ("facing", "north")])
            .unwrap();
        assert_eq!(lit.tints, [JavaTint::Grass, JavaTint::Rgb(0x00ff00)]);
        assert!(!lit.ao);
        let q = &lit.quads[0];
        assert_eq!(
            (q.normal, q.cull, q.tint, q.emission),
            (Face::North, Some(Face::North), 0, 15)
        );
        assert_eq!(q.pos[2], [1.0, 1.0, 0.0]);
        assert_eq!(&*q.sprite, "test:block/lamp");
        assert!(
            m.state("test:lamp", &[("lit", "false"), ("facing", "north")])
                .unwrap()
                .quads
                .is_empty()
        );
        assert!(m.state("test:lamp", &[("lit", "maybe")]).is_none());
    }

    #[test]
    fn refuses_hostile_reports() {
        let swap = |from: &str, to: &str| SAMPLE.replacen(from, to, 1);
        for bad in [
            swap("\"format\":1", "\"format\":9"),
            swap("[0,0,0,0,0,", "[1e9,0,0,0,0,"),
            swap("[0,0,0,0,0,", "[0,0,0,0,"),
            swap("2,2,0,2,15,2,0]", "2,2,0,2,15,99,0]"),
            swap("2,2,0,2,15,2,0]", "7,2,0,2,15,2,0]"),
            swap("\"still\":0", "\"still\":7"),
            "{".to_owned(),
        ] {
            assert!(JavaModels::parse(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn merging_keeps_blocks_and_takes_new_fluids() {
        let mut m = JavaModels::parse(SAMPLE.as_bytes()).unwrap();
        let mut other = JavaModels::default();
        other.blocks.insert("test:other".into(), HashMap::new());
        m.merge(other);
        assert!(m.has_block("test:lamp") && m.has_block("test:other"));
        assert!(m.fluids.contains_key("test:goo"));
    }
}
