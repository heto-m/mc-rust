use serde_json::Value;

use crate::error::{AssetError, Result};
use crate::identifier::Identifier;

pub type Mat4 = [[f32; 4]; 4];

pub const IDENTITY4: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub fn mat4_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub fn mat4_apply(m: &Mat4, v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2] + m[i][3])
}

#[derive(Clone, Debug, PartialEq)]
pub enum TintSource {
    Constant(i32),
    Dye(i32),
    Grass { temperature: f32, downfall: f32 },
    Firework(i32),
    Potion(i32),
    MapColor(i32),
    Team(i32),
    CustomModelData { index: usize, default: i32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConditionProp {
    CustomModelData(usize),
    UsingItem,
    Broken,
    Damaged,
    FishingRodCast,
    HasComponent {
        component: String,
        ignore_default: bool,
    },
    BundleHasSelectedItem,
    Selected,
    Carried,
    ExtendedView,
    KeybindDown(String),
    ViewEntity,
    Component {
        component: String,
        value: Value,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum SelectProp {
    CustomModelData(usize),
    MainHand,
    ChargeType,
    TrimMaterial,
    BlockState(String),
    DisplayContext,
    LocalTime { pattern: String },
    ContextEntityType,
    ContextDimension,
    Component(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeSource {
    Random,
    Daytime,
    MoonPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompassTarget {
    None,
    Spawn,
    Lodestone,
    Recovery,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RangeProp {
    CustomModelData(usize),
    BundleFullness,
    Damage { normalize: bool },
    Cooldown,
    Time(TimeSource),
    Compass(CompassTarget),
    CrossbowPull,
    UseCycle { period: f32 },
    UseDuration { remaining: bool },
    Count { normalize: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SpecialKind {
    Banner { color: String },
    Bed { texture: String },
    Chest { texture: String },
    Conduit,
    CopperGolemStatue { pose: String, texture: String },
    DecoratedPot,
    Head { kind: String },
    PlayerHead,
    Shield,
    ShulkerBox { texture: String },
    Trident,
    Other(String),
}

impl SpecialKind {
    pub fn name(&self) -> &str {
        match self {
            SpecialKind::Banner { .. } => "banner",
            SpecialKind::Bed { .. } => "bed",
            SpecialKind::Chest { .. } => "chest",
            SpecialKind::Conduit => "conduit",
            SpecialKind::CopperGolemStatue { .. } => "copper_golem_statue",
            SpecialKind::DecoratedPot => "decorated_pot",
            SpecialKind::Head { .. } => "head",
            SpecialKind::PlayerHead => "player_head",
            SpecialKind::Shield => "shield",
            SpecialKind::ShulkerBox { .. } => "shulker_box",
            SpecialKind::Trident => "trident",
            SpecialKind::Other(name) => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Empty,
    Model {
        model: Identifier,
        tints: Vec<TintSource>,
    },
    Composite(Vec<ItemNode>),
    Condition {
        prop: ConditionProp,
        on_true: Box<ItemNode>,
        on_false: Box<ItemNode>,
    },
    Select {
        prop: SelectProp,
        cases: Vec<(Vec<String>, ItemNode)>,
        fallback: Option<Box<ItemNode>>,
    },
    Range {
        prop: RangeProp,
        scale: f32,
        entries: Vec<(f32, ItemNode)>,
        fallback: Option<Box<ItemNode>>,
    },
    Special {
        base: Identifier,
        kind: SpecialKind,
    },
    BundleSelectedItem,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemNode {
    pub transform: Option<Mat4>,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemModelDef {
    pub root: ItemNode,
    pub oversized_in_gui: bool,
}

struct Parser<'a> {
    path: &'a str,
}

fn name_of(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

impl Parser<'_> {
    fn err(&self, message: impl Into<String>) -> AssetError {
        AssetError::invalid(self.path, message)
    }

    fn string(&self, obj: &Value, key: &str) -> Result<String> {
        obj.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| self.err(format!("missing string \"{key}\"")))
    }

    fn opt_string(obj: &Value, key: &str) -> Option<String> {
        obj.get(key).and_then(Value::as_str).map(str::to_owned)
    }

    fn float(obj: &Value, key: &str, default: f32) -> f32 {
        obj.get(key)
            .and_then(Value::as_f64)
            .map_or(default, |v| v as f32)
    }

    fn int(obj: &Value, key: &str, default: i32) -> i32 {
        obj.get(key)
            .and_then(Value::as_i64)
            .map_or(default, |v| v as i32)
    }

    fn flag(obj: &Value, key: &str, default: bool) -> bool {
        obj.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    fn index(obj: &Value) -> usize {
        Self::int(obj, "index", 0).max(0) as usize
    }

    fn node(&self, value: &Value, depth: usize) -> Result<ItemNode> {
        if depth > 64 {
            return Err(self.err("item model nesting too deep"));
        }
        let ty = self.string(value, "type")?;
        let ty = ty.strip_prefix("minecraft:").unwrap_or(&ty);
        let child = |key: &str| -> Result<ItemNode> {
            let v = value
                .get(key)
                .ok_or_else(|| self.err(format!("missing \"{key}\"")))?;
            self.node(v, depth + 1)
        };
        let optional_child = |key: &str| -> Result<Option<Box<ItemNode>>> {
            value
                .get(key)
                .map(|v| self.node(v, depth + 1).map(Box::new))
                .transpose()
        };
        let kind = match ty {
            "empty" => NodeKind::Empty,
            "model" => NodeKind::Model {
                model: Identifier::parse(&self.string(value, "model")?),
                tints: value
                    .get("tints")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(|t| self.tint(t)).collect::<Result<Vec<_>>>())
                    .transpose()?
                    .unwrap_or_default(),
            },
            "composite" => NodeKind::Composite(
                value
                    .get("models")
                    .and_then(Value::as_array)
                    .ok_or_else(|| self.err("composite without models"))?
                    .iter()
                    .map(|m| self.node(m, depth + 1))
                    .collect::<Result<_>>()?,
            ),
            "condition" => NodeKind::Condition {
                prop: self.condition_prop(value)?,
                on_true: Box::new(child("on_true")?),
                on_false: Box::new(child("on_false")?),
            },
            "select" => NodeKind::Select {
                prop: self.select_prop(value)?,
                cases: value
                    .get("cases")
                    .and_then(Value::as_array)
                    .ok_or_else(|| self.err("select without cases"))?
                    .iter()
                    .map(|case| {
                        let when = match case.get("when") {
                            Some(Value::Array(a)) => a.iter().map(value_text).collect(),
                            Some(v) => vec![value_text(v)],
                            None => return Err(self.err("select case without when")),
                        };
                        let model = case
                            .get("model")
                            .ok_or_else(|| self.err("select case without model"))?;
                        Ok((when, self.node(model, depth + 1)?))
                    })
                    .collect::<Result<_>>()?,
                fallback: optional_child("fallback")?,
            },
            "range_dispatch" => {
                let mut entries = value
                    .get("entries")
                    .and_then(Value::as_array)
                    .ok_or_else(|| self.err("range_dispatch without entries"))?
                    .iter()
                    .map(|e| {
                        let model = e
                            .get("model")
                            .ok_or_else(|| self.err("range entry without model"))?;
                        Ok((
                            Self::float(e, "threshold", 0.0),
                            self.node(model, depth + 1)?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                entries.sort_by(|a, b| a.0.total_cmp(&b.0));
                NodeKind::Range {
                    prop: self.range_prop(value)?,
                    scale: Self::float(value, "scale", 1.0),
                    entries,
                    fallback: optional_child("fallback")?,
                }
            }
            "special" => {
                let model = value
                    .get("model")
                    .ok_or_else(|| self.err("special without model"))?;
                NodeKind::Special {
                    base: Identifier::parse(&self.string(value, "base")?),
                    kind: self.special(model)?,
                }
            }
            "bundle/selected_item" => NodeKind::BundleSelectedItem,
            other => return Err(self.err(format!("unknown item model type {other}"))),
        };
        Ok(ItemNode {
            transform: value
                .get("transformation")
                .map(|t| self.transformation(t))
                .transpose()?,
            kind,
        })
    }

    fn tint(&self, value: &Value) -> Result<TintSource> {
        let ty = self.string(value, "type")?;
        let default = |fallback: i32| Self::int(value, "default", fallback);
        Ok(match ty.strip_prefix("minecraft:").unwrap_or(&ty) {
            "constant" => TintSource::Constant(Self::int(value, "value", -1)),
            "dye" => TintSource::Dye(default(0)),
            "grass" => TintSource::Grass {
                temperature: Self::float(value, "temperature", 0.5),
                downfall: Self::float(value, "downfall", 1.0),
            },
            "firework" => TintSource::Firework(default(-7697782)),
            "potion" => TintSource::Potion(default(-13083194)),
            "map_color" => TintSource::MapColor(default(0)),
            "team" => TintSource::Team(default(-1)),
            "custom_model_data" => TintSource::CustomModelData {
                index: Self::index(value),
                default: default(-1),
            },
            other => return Err(self.err(format!("unknown tint source {other}"))),
        })
    }

    fn property_name(&self, value: &Value) -> Result<String> {
        let name = self.string(value, "property")?;
        Ok(name.strip_prefix("minecraft:").unwrap_or(&name).to_owned())
    }

    fn condition_prop(&self, v: &Value) -> Result<ConditionProp> {
        Ok(match self.property_name(v)?.as_str() {
            "custom_model_data" => ConditionProp::CustomModelData(Self::index(v)),
            "using_item" => ConditionProp::UsingItem,
            "broken" => ConditionProp::Broken,
            "damaged" => ConditionProp::Damaged,
            "fishing_rod/cast" => ConditionProp::FishingRodCast,
            "has_component" => ConditionProp::HasComponent {
                component: self.string(v, "component")?,
                ignore_default: Self::flag(v, "ignore_default", false),
            },
            "bundle/has_selected_item" => ConditionProp::BundleHasSelectedItem,
            "selected" => ConditionProp::Selected,
            "carried" => ConditionProp::Carried,
            "extended_view" => ConditionProp::ExtendedView,
            "keybind_down" => ConditionProp::KeybindDown(self.string(v, "keybind")?),
            "view_entity" => ConditionProp::ViewEntity,
            "component" => ConditionProp::Component {
                component: Self::opt_string(v, "predicate").unwrap_or_default(),
                value: v.get("value").cloned().unwrap_or(Value::Null),
            },
            other => return Err(self.err(format!("unknown condition property {other}"))),
        })
    }

    fn select_prop(&self, v: &Value) -> Result<SelectProp> {
        Ok(match self.property_name(v)?.as_str() {
            "custom_model_data" => SelectProp::CustomModelData(Self::index(v)),
            "main_hand" => SelectProp::MainHand,
            "charge_type" => SelectProp::ChargeType,
            "trim_material" => SelectProp::TrimMaterial,
            "block_state" => SelectProp::BlockState(self.string(v, "block_state_property")?),
            "display_context" => SelectProp::DisplayContext,
            "local_time" => SelectProp::LocalTime {
                pattern: self.string(v, "pattern")?,
            },
            "context_entity_type" => SelectProp::ContextEntityType,
            "context_dimension" => SelectProp::ContextDimension,
            "component" => SelectProp::Component(self.string(v, "component")?),
            other => return Err(self.err(format!("unknown select property {other}"))),
        })
    }

    fn range_prop(&self, v: &Value) -> Result<RangeProp> {
        Ok(match self.property_name(v)?.as_str() {
            "custom_model_data" => RangeProp::CustomModelData(Self::index(v)),
            "bundle/fullness" => RangeProp::BundleFullness,
            "damage" => RangeProp::Damage {
                normalize: Self::flag(v, "normalize", true),
            },
            "cooldown" => RangeProp::Cooldown,
            "time" => RangeProp::Time(match name_of(&v["source"]) {
                "random" => TimeSource::Random,
                "daytime" => TimeSource::Daytime,
                "moon_phase" => TimeSource::MoonPhase,
                other => return Err(self.err(format!("unknown time source {other}"))),
            }),
            "compass" => RangeProp::Compass(match name_of(&v["target"]) {
                "none" => CompassTarget::None,
                "spawn" => CompassTarget::Spawn,
                "lodestone" => CompassTarget::Lodestone,
                "recovery" => CompassTarget::Recovery,
                other => return Err(self.err(format!("unknown compass target {other}"))),
            }),
            "crossbow/pull" => RangeProp::CrossbowPull,
            "use_cycle" => RangeProp::UseCycle {
                period: Self::float(v, "period", 1.0),
            },
            "use_duration" => RangeProp::UseDuration {
                remaining: Self::flag(v, "remaining", false),
            },
            "count" => RangeProp::Count {
                normalize: Self::flag(v, "normalize", true),
            },
            other => return Err(self.err(format!("unknown range property {other}"))),
        })
    }

    fn special(&self, v: &Value) -> Result<SpecialKind> {
        let ty = self.string(v, "type")?;
        let ty = ty.strip_prefix("minecraft:").unwrap_or(&ty);
        let text = |key: &str| Self::opt_string(v, key).unwrap_or_default();
        Ok(match ty {
            "banner" => SpecialKind::Banner {
                color: text("color"),
            },
            "bed" => SpecialKind::Bed {
                texture: text("texture"),
            },
            "chest" => SpecialKind::Chest {
                texture: text("texture"),
            },
            "conduit" => SpecialKind::Conduit,
            "copper_golem_statue" => SpecialKind::CopperGolemStatue {
                pose: text("pose"),
                texture: text("texture"),
            },
            "decorated_pot" => SpecialKind::DecoratedPot,
            "head" => SpecialKind::Head { kind: text("kind") },
            "player_head" => SpecialKind::PlayerHead,
            "shield" => SpecialKind::Shield,
            "shulker_box" => SpecialKind::ShulkerBox {
                texture: text("texture"),
            },
            "trident" => SpecialKind::Trident,
            other => SpecialKind::Other(other.to_owned()),
        })
    }

    fn transformation(&self, v: &Value) -> Result<Mat4> {
        let floats = |v: &Value| -> Option<Vec<f32>> {
            v.as_array()?
                .iter()
                .map(|x| x.as_f64().map(|f| f as f32))
                .collect()
        };
        if let Some(m) = floats(v).filter(|m| m.len() == 16) {
            return Ok(std::array::from_fn(|r| {
                std::array::from_fn(|c| m[r * 4 + c])
            }));
        }
        let vec3 = |key: &str, default: [f32; 3]| -> Result<[f32; 3]> {
            match v.get(key) {
                None => Ok(default),
                Some(x) => floats(x)
                    .and_then(|f| <[f32; 3]>::try_from(f).ok())
                    .ok_or_else(|| self.err(format!("bad \"{key}\""))),
            }
        };
        let quat = |key: &str| -> Result<[f32; 4]> {
            match v.get(key) {
                None => Ok([0.0, 0.0, 0.0, 1.0]),
                Some(x) => floats(x)
                    .and_then(|f| <[f32; 4]>::try_from(f).ok())
                    .ok_or_else(|| self.err(format!("bad \"{key}\""))),
            }
        };
        let translation = vec3("translation", [0.0; 3])?;
        let scale = vec3("scale", [1.0; 3])?;
        let left = quat_matrix(quat("left_rotation")?);
        let right = quat_matrix(quat("right_rotation")?);
        let mut scale_m = IDENTITY4;
        for i in 0..3 {
            scale_m[i][i] = scale[i];
        }
        let mut t = IDENTITY4;
        for i in 0..3 {
            t[i][3] = translation[i];
        }
        Ok(mat4_mul(&t, &mat4_mul(&left, &mat4_mul(&scale_m, &right))))
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn quat_matrix([x, y, z, w]: [f32; 4]) -> Mat4 {
    let len = (x * x + y * y + z * z + w * w).sqrt();
    if len == 0.0 {
        return IDENTITY4;
    }
    let (x, y, z, w) = (x / len, y / len, z / len, w / len);
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - z * w),
            2.0 * (x * z + y * w),
            0.0,
        ],
        [
            2.0 * (x * y + z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - x * w),
            0.0,
        ],
        [
            2.0 * (x * z - y * w),
            2.0 * (y * z + x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

impl ItemModelDef {
    pub fn parse(json: &[u8], path: &str) -> Result<Self> {
        let value: Value = serde_json::from_slice(json).map_err(|source| AssetError::Json {
            path: path.to_owned(),
            source,
        })?;
        let parser = Parser { path };
        let root = value
            .get("model")
            .ok_or_else(|| parser.err("missing \"model\""))?;
        Ok(Self {
            root: parser.node(root, 0)?,
            oversized_in_gui: Parser::flag(&value, "oversized_in_gui", false),
        })
    }

    /// Every special model kind this definition can select, with its base model.
    pub fn specials(&self) -> Vec<(Identifier, SpecialKind)> {
        fn walk(node: &ItemNode, out: &mut Vec<(Identifier, SpecialKind)>) {
            match &node.kind {
                NodeKind::Special { base, kind } => out.push((base.clone(), kind.clone())),
                NodeKind::Composite(nodes) => nodes.iter().for_each(|n| walk(n, out)),
                NodeKind::Condition {
                    on_true, on_false, ..
                } => {
                    walk(on_true, out);
                    walk(on_false, out);
                }
                NodeKind::Select {
                    cases, fallback, ..
                } => {
                    cases.iter().for_each(|(_, n)| walk(n, out));
                    fallback.iter().for_each(|f| walk(f, out));
                }
                NodeKind::Range {
                    entries, fallback, ..
                } => {
                    entries.iter().for_each(|(_, n)| walk(n, out));
                    fallback.iter().for_each(|f| walk(f, out));
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    /// Every model this definition can select, including special base models.
    pub fn models(&self) -> Vec<Identifier> {
        let mut out = Vec::new();
        self.root.collect_models(&mut out);
        out
    }
}

impl ItemNode {
    fn collect_models(&self, out: &mut Vec<Identifier>) {
        match &self.kind {
            NodeKind::Model { model, .. } => out.push(model.clone()),
            NodeKind::Special { base, .. } => out.push(base.clone()),
            NodeKind::Composite(nodes) => nodes.iter().for_each(|n| n.collect_models(out)),
            NodeKind::Condition {
                on_true, on_false, ..
            } => {
                on_true.collect_models(out);
                on_false.collect_models(out);
            }
            NodeKind::Select {
                cases, fallback, ..
            } => {
                cases.iter().for_each(|(_, n)| n.collect_models(out));
                if let Some(f) = fallback {
                    f.collect_models(out);
                }
            }
            NodeKind::Range {
                entries, fallback, ..
            } => {
                entries.iter().for_each(|(_, n)| n.collect_models(out));
                if let Some(f) = fallback {
                    f.collect_models(out);
                }
            }
            NodeKind::Empty | NodeKind::BundleSelectedItem => {}
        }
    }
}
