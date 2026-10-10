use std::collections::HashMap;

use serde_json::Value;

use crate::error::{AssetError, Result};
use crate::identifier::Identifier;
use crate::item_def::{
    ConditionProp, IDENTITY4, ItemModelDef, ItemNode, Mat4, NodeKind, RangeProp, SelectProp,
    SpecialKind, TimeSource, TintSource, mat4_mul,
};
use crate::model::DisplayContext;
use crate::resources::ResourceManager;
use crate::texture::decode_sprite;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChargeType {
    #[default]
    None,
    Arrow,
    Rocket,
}

/// Item stack data the definitions can look at; every method defaults to "absent".
pub trait ItemProps {
    fn count(&self) -> i32 {
        1
    }
    fn max_stack_size(&self) -> i32 {
        64
    }
    fn damage(&self) -> i32 {
        0
    }
    fn max_damage(&self) -> i32 {
        0
    }
    fn unbreakable(&self) -> bool {
        false
    }
    fn item_model(&self) -> Option<&str> {
        None
    }
    fn has_component(&self, _component: &str, _ignore_default: bool) -> bool {
        false
    }
    fn component_string(&self, _component: &str) -> Option<String> {
        None
    }
    fn component_matches(&self, _component: &str, _value: &Value) -> bool {
        false
    }
    fn custom_model_float(&self, _index: usize) -> Option<f32> {
        None
    }
    fn custom_model_flag(&self, _index: usize) -> Option<bool> {
        None
    }
    fn custom_model_string(&self, _index: usize) -> Option<&str> {
        None
    }
    fn custom_model_color(&self, _index: usize) -> Option<i32> {
        None
    }
    fn dyed_color(&self) -> Option<i32> {
        None
    }
    fn potion_color(&self) -> Option<i32> {
        None
    }
    fn map_color(&self) -> Option<i32> {
        None
    }
    fn firework_colors(&self) -> &[i32] {
        &[]
    }
    fn team_color(&self) -> Option<i32> {
        None
    }
    fn trim_material(&self) -> Option<&str> {
        None
    }
    fn block_state(&self, _property: &str) -> Option<&str> {
        None
    }
    fn charge_type(&self) -> ChargeType {
        ChargeType::None
    }
    fn bundle_fullness(&self) -> f32 {
        0.0
    }
}

/// An item with no components.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainItem;

impl ItemProps for PlainItem {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDateTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Default for LocalDateTime {
    fn default() -> Self {
        Self {
            year: 2026,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }
}

impl LocalDateTime {
    fn format(&self, pattern: &str) -> String {
        let mut out = String::new();
        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let run = chars[i..].iter().take_while(|&&x| x == c).count();
            let number = |v: i64| format!("{v:0width$}", width = run);
            match c {
                'y' if run == 2 => out.push_str(&format!("{:02}", self.year.rem_euclid(100))),
                'y' | 'u' => out.push_str(&number(i64::from(self.year))),
                'M' => out.push_str(&number(i64::from(self.month))),
                'd' => out.push_str(&number(i64::from(self.day))),
                'H' => out.push_str(&number(i64::from(self.hour))),
                'm' => out.push_str(&number(i64::from(self.minute))),
                's' => out.push_str(&number(i64::from(self.second))),
                _ => out.extend(std::iter::repeat_n(c, run)),
            }
            i += run;
        }
        out
    }
}

/// Everything about the viewing situation that is not the stack; defaults describe a plain GUI slot.
#[derive(Clone, Debug)]
pub struct ItemContext {
    pub display: DisplayContext,
    pub using_item: bool,
    pub ticks_using_item: f32,
    pub use_remaining_ticks: f32,
    pub cooldown: f32,
    pub day_fraction: f32,
    pub moon_phase_fraction: f32,
    pub random: f32,
    pub compass_angle: f32,
    pub crossbow_pull: f32,
    pub fishing_rod_cast: bool,
    pub selected: bool,
    pub carried: bool,
    pub extended_view: bool,
    pub view_entity: bool,
    pub left_handed: bool,
    pub bundle_has_selected: bool,
    pub dimension: String,
    pub entity_type: Option<String>,
    pub local_time: LocalDateTime,
    pub keys_down: Vec<String>,
}

impl Default for ItemContext {
    fn default() -> Self {
        Self {
            display: DisplayContext::Gui,
            using_item: false,
            ticks_using_item: 0.0,
            use_remaining_ticks: 0.0,
            cooldown: 0.0,
            day_fraction: 0.0,
            moon_phase_fraction: 0.0,
            random: 0.0,
            compass_angle: 0.0,
            crossbow_pull: 0.0,
            fishing_rod_cast: false,
            selected: false,
            carried: false,
            extended_view: false,
            view_entity: false,
            left_handed: false,
            bundle_has_selected: false,
            dimension: "minecraft:overworld".into(),
            entity_type: None,
            local_time: LocalDateTime::default(),
            keys_down: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerKind {
    Model(Identifier),
    Special { base: Identifier, kind: SpecialKind },
    BundleSelectedItem,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemLayer {
    pub kind: LayerKind,
    /// ARGB per tint index.
    pub tints: Vec<u32>,
    pub transform: Mat4,
}

#[derive(Clone, Debug)]
struct Colormap {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

impl Colormap {
    fn sample(&self, temperature: f32, downfall: f32) -> u32 {
        let t = temperature.clamp(0.0, 1.0);
        let d = downfall.clamp(0.0, 1.0) * t;
        let x = (((1.0 - t) * 255.0) as usize).min(self.width.saturating_sub(1));
        let y = (((1.0 - d) * 255.0) as usize).min(self.height.saturating_sub(1));
        let i = (y * self.width + x) * 4;
        let p = &self.pixels[i..i + 3];
        0xFF00_0000 | u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2])
    }
}

/// All item model definitions of a resource stack.
#[derive(Debug, Default)]
pub struct ItemModels {
    defs: HashMap<Identifier, ItemModelDef>,
    grass: Option<Colormap>,
}

fn opaque(color: i32) -> u32 {
    0xFF00_0000 | color as u32
}

fn same_id(a: &str, b: &str) -> bool {
    Identifier::parse(a) == Identifier::parse(b)
}

fn average(colors: &[i32]) -> i32 {
    let n = colors.len() as i64;
    let channel = |shift: u32| {
        colors
            .iter()
            .map(|&c| i64::from((c as u32 >> shift) & 0xFF))
            .sum::<i64>()
            / n
    };
    ((channel(16) << 16) | (channel(8) << 8) | channel(0)) as i32
}

impl ItemModels {
    pub fn load(resources: &ResourceManager) -> Result<Self> {
        let mut defs = HashMap::new();
        for path in resources.list_prefix("assets/") {
            let Some((namespace, rest)) =
                path.strip_prefix("assets/").and_then(|p| p.split_once('/'))
            else {
                continue;
            };
            let Some(name) = rest
                .strip_prefix("items/")
                .and_then(|n| n.strip_suffix(".json"))
            else {
                continue;
            };
            let Some(bytes) = resources.read(path)? else {
                continue;
            };
            defs.insert(
                Identifier::new(namespace, name),
                ItemModelDef::parse(&bytes, path)?,
            );
        }
        let grass = match resources.read("assets/minecraft/textures/colormap/grass.png")? {
            Some(png) => {
                let sprite = decode_sprite("colormap/grass", &png, None)?;
                Some(Colormap {
                    width: sprite.width as usize,
                    height: sprite.height as usize,
                    pixels: sprite.frames.into_iter().next().unwrap_or_default(),
                })
            }
            None => None,
        };
        Ok(Self { defs, grass })
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn get(&self, id: &Identifier) -> Option<&ItemModelDef> {
        self.defs.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &Identifier> {
        self.defs.keys()
    }

    /// Layers drawn for `item`; empty for unknown items and the `empty` model.
    pub fn evaluate(
        &self,
        item: &Identifier,
        props: &dyn ItemProps,
        ctx: &ItemContext,
    ) -> Vec<ItemLayer> {
        let id = props.item_model().map_or_else(
            || item.clone(),
            |m| {
                let model = Identifier::parse(m);
                if self.defs.contains_key(&model) {
                    model
                } else {
                    item.clone()
                }
            },
        );
        let mut out = Vec::new();
        if let Some(def) = self.defs.get(&id) {
            Evaluator {
                models: self,
                props,
                ctx,
            }
            .node(&def.root, &IDENTITY4, &mut out);
        }
        out
    }

    pub fn require(&self, item: &Identifier) -> Result<&ItemModelDef> {
        self.defs
            .get(item)
            .ok_or_else(|| AssetError::Missing(item.resource_path("items", ".json")))
    }
}

struct Evaluator<'a> {
    models: &'a ItemModels,
    props: &'a dyn ItemProps,
    ctx: &'a ItemContext,
}

impl Evaluator<'_> {
    fn node(&self, node: &ItemNode, parent: &Mat4, out: &mut Vec<ItemLayer>) {
        let transform = match &node.transform {
            Some(t) => mat4_mul(parent, t),
            None => *parent,
        };
        match &node.kind {
            NodeKind::Empty => {}
            NodeKind::Model { model, tints } => out.push(ItemLayer {
                kind: LayerKind::Model(model.clone()),
                tints: tints.iter().map(|t| self.tint(t)).collect(),
                transform,
            }),
            NodeKind::Composite(models) => {
                for m in models {
                    self.node(m, &transform, out);
                }
            }
            NodeKind::Condition {
                prop,
                on_true,
                on_false,
            } => {
                let branch = if self.condition(prop) {
                    on_true
                } else {
                    on_false
                };
                self.node(branch, &transform, out);
            }
            NodeKind::Select {
                prop,
                cases,
                fallback,
            } => {
                let value = self.select_value(prop);
                let hit = value.and_then(|v| {
                    cases
                        .iter()
                        .find(|(when, _)| when.iter().any(|w| same_id(w, &v)))
                        .map(|(_, node)| node)
                });
                if let Some(node) = hit.or(fallback.as_deref()) {
                    self.node(node, &transform, out);
                }
            }
            NodeKind::Range {
                prop,
                scale,
                entries,
                fallback,
            } => {
                let value = self.range_value(prop) * scale;
                let picked = if value.is_nan() {
                    None
                } else {
                    entries.iter().rev().find(|(t, _)| *t <= value)
                };
                match picked {
                    Some((_, node)) => self.node(node, &transform, out),
                    None => {
                        if let Some(f) = fallback {
                            self.node(f, &transform, out);
                        }
                    }
                }
            }
            NodeKind::Special { base, kind } => out.push(ItemLayer {
                kind: LayerKind::Special {
                    base: base.clone(),
                    kind: kind.clone(),
                },
                tints: Vec::new(),
                transform,
            }),
            NodeKind::BundleSelectedItem => out.push(ItemLayer {
                kind: LayerKind::BundleSelectedItem,
                tints: Vec::new(),
                transform,
            }),
        }
    }

    fn tint(&self, source: &TintSource) -> u32 {
        match source {
            TintSource::Constant(c) => *c as u32,
            TintSource::Dye(default) => opaque(self.props.dyed_color().unwrap_or(*default)),
            TintSource::Grass {
                temperature,
                downfall,
            } => self
                .models
                .grass
                .as_ref()
                .map_or(0xFF7F_B238, |c| c.sample(*temperature, *downfall)),
            TintSource::Firework(default) => {
                let colors = self.props.firework_colors();
                opaque(if colors.is_empty() {
                    *default
                } else {
                    average(colors)
                })
            }
            TintSource::Potion(default) => opaque(self.props.potion_color().unwrap_or(*default)),
            TintSource::MapColor(default) => opaque(self.props.map_color().unwrap_or(*default)),
            TintSource::Team(default) => opaque(self.props.team_color().unwrap_or(*default)),
            TintSource::CustomModelData { index, default } => {
                opaque(self.props.custom_model_color(*index).unwrap_or(*default))
            }
        }
    }

    fn condition(&self, prop: &ConditionProp) -> bool {
        let (p, c) = (self.props, self.ctx);
        match prop {
            ConditionProp::CustomModelData(i) => p.custom_model_flag(*i).unwrap_or(false),
            ConditionProp::UsingItem => c.using_item,
            ConditionProp::Broken => p.max_damage() > 0 && p.damage() >= p.max_damage() - 1,
            ConditionProp::Damaged => p.max_damage() > 0 && p.damage() > 0,
            ConditionProp::FishingRodCast => c.fishing_rod_cast,
            ConditionProp::HasComponent {
                component,
                ignore_default,
            } => p.has_component(component, *ignore_default),
            ConditionProp::BundleHasSelectedItem => c.bundle_has_selected,
            ConditionProp::Selected => c.selected,
            ConditionProp::Carried => c.carried,
            ConditionProp::ExtendedView => c.extended_view,
            ConditionProp::KeybindDown(key) => c.keys_down.iter().any(|k| k == key),
            ConditionProp::ViewEntity => c.view_entity,
            ConditionProp::Component { component, value } => p.component_matches(component, value),
        }
    }

    fn select_value(&self, prop: &SelectProp) -> Option<String> {
        let (p, c) = (self.props, self.ctx);
        match prop {
            SelectProp::CustomModelData(i) => p.custom_model_string(*i).map(str::to_owned),
            SelectProp::MainHand => Some(if c.left_handed { "left" } else { "right" }.into()),
            SelectProp::ChargeType => Some(
                match p.charge_type() {
                    ChargeType::None => "none",
                    ChargeType::Arrow => "arrow",
                    ChargeType::Rocket => "rocket",
                }
                .into(),
            ),
            SelectProp::TrimMaterial => p.trim_material().map(str::to_owned),
            SelectProp::BlockState(name) => p.block_state(name).map(str::to_owned),
            SelectProp::DisplayContext => Some(c.display.name().into()),
            SelectProp::LocalTime { pattern } => Some(c.local_time.format(pattern)),
            SelectProp::ContextEntityType => c.entity_type.clone(),
            SelectProp::ContextDimension => Some(c.dimension.clone()),
            SelectProp::Component(name) => p.component_string(name),
        }
    }

    fn range_value(&self, prop: &RangeProp) -> f32 {
        let (p, c) = (self.props, self.ctx);
        match prop {
            RangeProp::CustomModelData(i) => p.custom_model_float(*i).unwrap_or(0.0),
            RangeProp::BundleFullness => p.bundle_fullness(),
            RangeProp::Damage { normalize } => {
                let (damage, max) = (p.damage() as f32, p.max_damage() as f32);
                if *normalize {
                    if max > 0.0 {
                        (damage / max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                } else {
                    damage.clamp(0.0, max.max(0.0))
                }
            }
            RangeProp::Cooldown => c.cooldown,
            RangeProp::Time(source) => match source {
                TimeSource::Random => c.random,
                TimeSource::Daytime => c.day_fraction,
                TimeSource::MoonPhase => c.moon_phase_fraction,
            },
            RangeProp::Compass(_) => c.compass_angle,
            RangeProp::CrossbowPull => c.crossbow_pull,
            RangeProp::UseCycle { period } => {
                if c.using_item && *period > 0.0 {
                    c.use_remaining_ticks % period
                } else {
                    0.0
                }
            }
            RangeProp::UseDuration { remaining } => {
                if !c.using_item {
                    0.0
                } else if *remaining {
                    c.use_remaining_ticks
                } else {
                    c.ticks_using_item
                }
            }
            RangeProp::Count { normalize } => {
                let (count, max) = (p.count() as f32, p.max_stack_size() as f32);
                if *normalize {
                    (count / max).clamp(0.0, 1.0)
                } else {
                    count.clamp(0.0, max)
                }
            }
        }
    }
}
