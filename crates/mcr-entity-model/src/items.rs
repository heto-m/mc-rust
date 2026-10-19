use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use glam::{Mat4, Quat, Vec3};
use mcr_assets::{
    BakedItemModel, ChargeType, DisplayContext, Identifier, ItemBaker, ItemContext as AssetCtx,
    ItemLayer, ItemProps, LayerKind, ModelBaker,
};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, component_type_id};
use rustc_hash::FxHashMap;

use crate::assets::SceneAssets;
use crate::cube::Vertex;
use crate::gpu::{ModelId, Pipeline};
use crate::model::Model;
use crate::plan::{ItemContext, WHITE};

pub const MAX_CACHED: usize = 1024;
pub const MAX_BAKES_PER_FRAME: u32 = 32;

/// Static geometry of one sprite of an item model, in block units around the model centre.
#[derive(Debug)]
pub struct GeoPart {
    pub mesh: Model,
    pub model_id: OnceLock<ModelId>,
    pub texture: String,
    pub local: Mat4,
    pub tint: [u8; 4],
    pub pipeline: Pipeline,
}

#[derive(Debug, Default)]
pub struct ItemGeo {
    pub parts: Vec<GeoPart>,
    /// Lowest point of the model in block units after its ground transform, for dropped items.
    pub ground_offset: f32,
    /// Z extent after the display transform; 1/16 or less lays dropped copies out flat.
    pub depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    item: i32,
    dyed: i32,
    model_hash: u64,
    context: ItemContext,
    using: u8,
    variant: u64,
}

#[derive(Debug)]
pub struct ItemGeometry {
    assets: Arc<SceneAssets>,
    baker: ItemBaker,
    baked: HashMap<Identifier, Option<Arc<BakedItemModel>>>,
    cache: FxHashMap<Key, Option<Arc<ItemGeo>>>,
    models: HashMap<String, Option<Arc<ItemGeo>>>,
    blocks: HashMap<i32, Option<Arc<ItemGeo>>>,
    bakes: u32,
    evicted: Vec<ModelId>,
}

struct StackProps<'a> {
    stack: &'a ItemStack,
    max_damage: i32,
    max_stack: i32,
}

impl StackProps<'_> {
    fn component(&self, name: &str) -> Option<&DataComponent> {
        self.stack.components.get(component_type_id(name)?)
    }
}

impl ItemProps for StackProps<'_> {
    fn count(&self) -> i32 {
        self.stack.count
    }

    fn max_stack_size(&self) -> i32 {
        self.max_stack
    }

    fn damage(&self) -> i32 {
        match self.component("damage") {
            Some(DataComponent::Damage(v)) => v.0,
            _ => 0,
        }
    }

    fn max_damage(&self) -> i32 {
        match self.component("max_damage") {
            Some(DataComponent::MaxDamage(v)) => v.0,
            _ => self.max_damage,
        }
    }

    fn unbreakable(&self) -> bool {
        self.component("unbreakable").is_some()
    }

    fn item_model(&self) -> Option<&str> {
        match self.component("item_model") {
            Some(DataComponent::ItemModel(id)) => Some(id.as_str()),
            _ => None,
        }
    }

    fn has_component(&self, component: &str, _ignore_default: bool) -> bool {
        let name = component.strip_prefix("minecraft:").unwrap_or(component);
        self.component(name).is_some()
    }

    fn custom_model_float(&self, index: usize) -> Option<f32> {
        match self.component("custom_model_data") {
            Some(DataComponent::CustomModelData(d)) => d.floats.get(index).copied(),
            _ => None,
        }
    }

    fn custom_model_flag(&self, index: usize) -> Option<bool> {
        match self.component("custom_model_data") {
            Some(DataComponent::CustomModelData(d)) => d.flags.get(index).copied(),
            _ => None,
        }
    }

    fn custom_model_string(&self, index: usize) -> Option<&str> {
        match self.component("custom_model_data") {
            Some(DataComponent::CustomModelData(d)) => d.strings.get(index).map(String::as_str),
            _ => None,
        }
    }

    fn custom_model_color(&self, index: usize) -> Option<i32> {
        match self.component("custom_model_data") {
            Some(DataComponent::CustomModelData(d)) => d.colors.get(index).copied(),
            _ => None,
        }
    }

    fn dyed_color(&self) -> Option<i32> {
        match self.component("dyed_color") {
            Some(DataComponent::DyedColor(c)) => Some(*c),
            _ => None,
        }
    }

    fn potion_color(&self) -> Option<i32> {
        match self.component("potion_contents") {
            Some(DataComponent::PotionContents(p)) => p.custom_color,
            _ => None,
        }
    }

    fn block_state(&self, property: &str) -> Option<&str> {
        match self.component("block_state") {
            Some(DataComponent::BlockState(props)) => props
                .iter()
                .find(|p| p.name == property)
                .map(|p| p.value.as_str()),
            _ => None,
        }
    }

    fn charge_type(&self) -> ChargeType {
        match self.component("charged_projectiles") {
            Some(DataComponent::ChargedProjectiles(list)) if !list.0.is_empty() => {
                ChargeType::Arrow
            }
            _ => ChargeType::None,
        }
    }
}

fn display_context(c: ItemContext) -> DisplayContext {
    match c {
        ItemContext::RightHand => DisplayContext::ThirdPersonRightHand,
        ItemContext::LeftHand => DisplayContext::ThirdPersonLeftHand,
        ItemContext::FirstPersonRight => DisplayContext::FirstPersonRightHand,
        ItemContext::FirstPersonLeft => DisplayContext::FirstPersonLeftHand,
        ItemContext::Head => DisplayContext::Head,
        ItemContext::Ground => DisplayContext::Ground,
        ItemContext::Fixed => DisplayContext::Fixed,
    }
}

/// `CrossbowPull`: ticks used over the charge time, which is the use duration less 3 ticks; a
/// holder whose duration is unknown charges for vanilla's default 25 ticks.
fn crossbow_pull(ticks: f32, remaining: f32) -> f32 {
    let charge = ticks + remaining - 3.0;
    let charge = if charge > 0.0 && charge < 1000.0 {
        charge
    } else {
        25.0
    };
    (ticks / charge).clamp(0.0, 1.0)
}

/// The display transform of an entity-model special: like a baked model's, ending in the
/// block-centring `T(-0.5)` that the shield and trident display offsets are authored against.
fn special_base(display: Option<&mcr_assets::ItemTransform>, left: bool) -> Mat4 {
    display.map_or(Mat4::from_translation(Vec3::splat(-0.5)), |t| {
        display_matrix(t, left)
    })
}

fn mat_from_rows(m: &[[f32; 4]; 4]) -> Mat4 {
    Mat4::from_cols_array_2d(&std::array::from_fn(|c| std::array::from_fn(|r| m[r][c])))
}

/// `T(t) * R(rot) * S(scale) * T(-0.5)` with the left-hand mirror vanilla applies.
fn display_matrix(t: &mcr_assets::ItemTransform, left: bool) -> Mat4 {
    let (tx, ry, rz) = if left {
        (-t.translation[0], -t.rotation[1], -t.rotation[2])
    } else {
        (t.translation[0], t.rotation[1], t.rotation[2])
    };
    let d = std::f32::consts::PI / 180.0;
    Mat4::from_translation(Vec3::new(tx, t.translation[1], t.translation[2]))
        * Mat4::from_quat(Quat::from_euler(
            glam::EulerRot::XYZ,
            t.rotation[0] * d,
            ry * d,
            rz * d,
        ))
        * Mat4::from_scale(Vec3::from(t.scale))
        * Mat4::from_translation(Vec3::splat(-0.5))
}

impl ItemGeometry {
    /// A block or item model by id (item frames), centred, with no display transform.
    pub fn model(&mut self, id: &str) -> Option<Arc<ItemGeo>> {
        if let Some(hit) = self.models.get(id) {
            return hit.clone();
        }
        if !self.admit() {
            return None;
        }
        let ident = Identifier::parse(id);
        let geo = self.baked(&ident).and_then(|m| {
            self.geo_from_quads(
                &m.quads,
                &[],
                Mat4::from_translation(Vec3::splat(-0.5)),
                &|id| self.baker.sprites().get(id).path().to_owned(),
            )
        });
        self.models.insert(id.to_owned(), geo.clone());
        geo
    }

    /// Geometry of a block state id with its default model variant.
    pub fn block(&mut self, state: i32) -> Option<Arc<ItemGeo>> {
        if let Some(hit) = self.blocks.get(&state) {
            return hit.clone();
        }
        if !self.admit() {
            return None;
        }
        let geo = self.bake_block(state);
        self.blocks.insert(state, geo.clone());
        geo
    }

    fn bake_block(&self, state: i32) -> Option<Arc<ItemGeo>> {
        let version = mcr_data::v26_3();
        let id = u32::try_from(state).ok()?;
        let name = version.state_block_name(id)?;
        let props: Vec<(&str, &str)> = version.state_properties(id).collect();
        let resources = &self.assets.resources;
        let mut baker = ModelBaker::new(resources);
        let model = baker.bake_block_state(name, &props).ok()?;
        let mut quads = Vec::new();
        for part in &model.parts {
            if let Some(v) = part.variants.first() {
                quads.extend(v.quads.iter().copied());
            }
        }
        let sprites = baker.sprites();
        self.geo_from_quads(&quads, &[], Mat4::IDENTITY, &|id| {
            sprites.get(id).path().to_owned()
        })
    }

    fn geo_from_quads(
        &self,
        quads: &[mcr_assets::BakedQuad],
        tints: &[u32],
        local: Mat4,
        path: &dyn Fn(mcr_assets::SpriteId) -> String,
    ) -> Option<Arc<ItemGeo>> {
        let mut groups: Vec<((mcr_assets::SpriteId, i32), Vec<Vertex>)> = Vec::new();
        for quad in quads {
            let slot = match groups
                .iter()
                .position(|(k, _)| *k == (quad.sprite, quad.tint))
            {
                Some(i) => i,
                None => {
                    groups.push(((quad.sprite, quad.tint), Vec::new()));
                    groups.len() - 1
                }
            };
            let normal = mcr_assets::Face::normal(quad.normal).map(f32::from);
            for v in &quad.vertices {
                groups[slot].1.push(Vertex {
                    pos: v.pos,
                    bone: 0,
                    normal,
                    uv: v.uv,
                });
            }
        }
        let mut geo = ItemGeo::default();
        for ((sprite, tint_index), vertices) in groups {
            let indices = (0..vertices.len() as u32 / 4)
                .flat_map(|q| [q * 4, q * 4 + 1, q * 4 + 2, q * 4, q * 4 + 2, q * 4 + 3])
                .collect();
            let tint = if tint_index < 0 {
                WHITE
            } else {
                tints
                    .get(tint_index as usize)
                    .map_or([0x77, 0xAB, 0x2F, 255], |c| {
                        [(c >> 16) as u8, (c >> 8) as u8, *c as u8, 255]
                    })
            };
            geo.parts.push(GeoPart {
                mesh: Model::from_geometry(vertices, indices),
                model_id: OnceLock::new(),
                texture: path(sprite),
                local,
                tint,
                pipeline: Pipeline::Cutout,
            });
        }
        (!geo.parts.is_empty()).then(|| Arc::new(geo))
    }

    pub fn new(assets: Arc<SceneAssets>) -> Self {
        ItemGeometry {
            assets,
            baker: ItemBaker::new(),
            baked: HashMap::new(),
            cache: FxHashMap::default(),
            models: HashMap::new(),
            blocks: HashMap::new(),
            bakes: 0,
            evicted: Vec::new(),
        }
    }

    pub fn item(&mut self, stack: &ItemStack, context: ItemContext) -> Option<Arc<ItemGeo>> {
        self.item_using(stack, context, None)
    }

    /// Like [`Self::item`] while the holder is using the item: `(ticks used, ticks remaining)`.
    pub fn item_using(
        &mut self,
        stack: &ItemStack,
        context: ItemContext,
        using: Option<(f32, f32)>,
    ) -> Option<Arc<ItemGeo>> {
        if stack.components.is_empty() && using.is_none() {
            let key = Key {
                item: stack.item,
                dyed: 0,
                model_hash: 0,
                context,
                using: 0,
                variant: 0,
            };
            if let Some(hit) = self.cache.get(&key) {
                return hit.clone();
            }
        }
        let version = mcr_data::v26_3();
        let name = version.item_name(u32::try_from(stack.item).ok()?)?;
        let props = StackProps {
            stack,
            max_damage: version
                .item_info(u32::try_from(stack.item).ok()?)
                .map_or(0, |i| i.max_damage as i32),
            max_stack: version
                .item_info(u32::try_from(stack.item).ok()?)
                .map_or(64, |i| i32::from(i.max_stack_size)),
        };
        let known_model = props
            .item_model()
            .filter(|m| self.assets.items.get(&Identifier::parse(m)).is_some());
        let broken = props.max_damage() > 0 && props.damage() >= props.max_damage() - 1;
        let model_hash = known_model
            .map_or(0, |m| m.bytes().fold(7u64, |h, b| h * 31 + u64::from(b)))
            ^ u64::from(broken) << 63;
        let key = Key {
            item: stack.item,
            dyed: props.dyed_color().map_or(0, |c| c & 0x00FF_FFFF),
            model_hash,
            context,
            using: using.map_or(0, |(ticks, _)| 1 + ticks.clamp(0.0, 120.0) as u8),
            variant: crate::special::variant_hash(stack),
        };
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        if !self.admit() {
            return None;
        }
        let ctx = AssetCtx {
            display: display_context(context),
            using_item: using.is_some(),
            ticks_using_item: using.map_or(0.0, |u| u.0),
            use_remaining_ticks: using.map_or(0.0, |u| u.1),
            crossbow_pull: using.map_or(0.0, |(ticks, remaining)| crossbow_pull(ticks, remaining)),
            ..AssetCtx::default()
        };
        let layers = self
            .assets
            .items
            .evaluate(&Identifier::parse(name), &props, &ctx);
        let geo = self.build(&layers, context, Some(stack)).map(Arc::new);
        self.cache.insert(key, geo.clone());
        geo
    }

    /// Resets the per-frame bake budget.
    pub fn begin_frame(&mut self) {
        self.bakes = 0;
    }

    /// GPU models of evicted geometry, to be freed by the caller.
    pub fn take_evicted(&mut self) -> Vec<ModelId> {
        std::mem::take(&mut self.evicted)
    }

    /// Charges one bake against the frame budget; evicts everything when the caches are full.
    fn admit(&mut self) -> bool {
        if self.bakes >= MAX_BAKES_PER_FRAME {
            return false;
        }
        self.bakes += 1;
        if self.cache.len() + self.models.len() + self.blocks.len() >= MAX_CACHED {
            let geos = self
                .cache
                .drain()
                .filter_map(|(_, g)| g)
                .chain(self.models.drain().filter_map(|(_, g)| g))
                .chain(self.blocks.drain().filter_map(|(_, g)| g));
            for geo in geos {
                self.evicted
                    .extend(geo.parts.iter().filter_map(|p| p.model_id.get().copied()));
            }
        }
        true
    }

    fn baked(&mut self, id: &Identifier) -> Option<Arc<BakedItemModel>> {
        if let Some(hit) = self.baked.get(id) {
            return hit.clone();
        }
        let model = match self.baker.bake(&self.assets.resources, id) {
            Ok(m) => Some(m),
            Err(e) => {
                tracing_free_warn(id, &e);
                None
            }
        };
        self.baked.insert(id.clone(), model.clone());
        model
    }

    fn special_model(
        &mut self,
        base: &Identifier,
        kind: &mcr_assets::SpecialKind,
    ) -> Option<Arc<BakedItemModel>> {
        let base = self.baked(base)?;
        self.baker
            .bake_special(&self.assets.resources, &base, kind)
            .ok()
    }

    /// Shield and trident draw their entity models; returns whether `kind` was one of them.
    fn push_entity_special(
        &mut self,
        geo: &mut ItemGeo,
        layer: &ItemLayer,
        base: &Identifier,
        kind: &mcr_assets::SpecialKind,
        context: ItemContext,
        stack: Option<&ItemStack>,
    ) -> bool {
        if !matches!(
            kind,
            mcr_assets::SpecialKind::Shield | mcr_assets::SpecialKind::Trident
        ) {
            return false;
        }
        let left = matches!(
            context,
            ItemContext::LeftHand | ItemContext::FirstPersonLeft
        );
        let display = self
            .baked(base)
            .and_then(|m| m.display(display_context(context)));
        let local = special_base(display.as_ref(), left) * mat_from_rows(&layer.transform);
        let layers = &self.assets.layers;
        let parts =
            crate::special::parts(kind, |name| layers.get(name).map(Model::from_layer), stack);
        for p in parts {
            geo.parts.push(GeoPart {
                mesh: p.mesh,
                model_id: OnceLock::new(),
                texture: p.texture,
                local,
                tint: p.tint,
                pipeline: p.pipeline,
            });
        }
        true
    }

    fn build(
        &mut self,
        layers: &[ItemLayer],
        context: ItemContext,
        stack: Option<&ItemStack>,
    ) -> Option<ItemGeo> {
        let mut geo = ItemGeo::default();
        let left = matches!(
            context,
            ItemContext::LeftHand | ItemContext::FirstPersonLeft
        );
        let mut min_y = f32::MAX;
        let (mut min_z, mut max_z) = (f32::MAX, f32::MIN);
        let mut special = false;
        let mut ground_scale = 1.0f32;
        for layer in layers {
            let model = match &layer.kind {
                LayerKind::Model(id) => self.baked(id),
                LayerKind::Special { base, kind } => {
                    if self.push_entity_special(&mut geo, layer, base, kind, context, stack) {
                        special = true;
                        continue;
                    }
                    self.special_model(base, kind)
                }
                _ => None,
            };
            let Some(model) = model else {
                continue;
            };
            let display = model.display(display_context(context));
            if let Some(d) = &display {
                ground_scale = d.scale[1];
            }
            let base = match &display {
                Some(t) => display_matrix(t, left),
                None => Mat4::from_translation(Vec3::splat(-0.5)),
            };
            let local = base * mat_from_rows(&layer.transform);
            let mut groups: Vec<((mcr_assets::SpriteId, i32), Vec<Vertex>)> = Vec::new();
            for quad in model.quads.iter() {
                let slot = match groups
                    .iter()
                    .position(|(k, _)| *k == (quad.sprite, quad.tint))
                {
                    Some(i) => i,
                    None => {
                        groups.push(((quad.sprite, quad.tint), Vec::new()));
                        groups.len() - 1
                    }
                };
                let normal = mcr_assets::Face::normal(quad.normal).map(f32::from);
                for v in &quad.vertices {
                    let p = local.transform_point3(Vec3::from(v.pos));
                    min_y = min_y.min(p.y);
                    min_z = min_z.min(p.z);
                    max_z = max_z.max(p.z);
                    groups[slot].1.push(Vertex {
                        pos: v.pos,
                        bone: 0,
                        normal,
                        uv: v.uv,
                    });
                }
            }
            for ((sprite_id, tint_index), vertices) in groups {
                let name = self.baker.sprites().get(sprite_id).clone();
                let indices = (0..vertices.len() as u32 / 4)
                    .flat_map(|q| [q * 4, q * 4 + 1, q * 4 + 2, q * 4, q * 4 + 2, q * 4 + 3])
                    .collect();
                let tint = usize::try_from(tint_index)
                    .ok()
                    .and_then(|i| layer.tints.get(i).copied())
                    .map_or(WHITE, |c| [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
                geo.parts.push(GeoPart {
                    mesh: Model::from_geometry(vertices, indices),
                    model_id: OnceLock::new(),
                    texture: name.path().to_owned(),
                    local: Mat4::IDENTITY,
                    tint,
                    pipeline: Pipeline::Cutout,
                });
                let last = geo.parts.last_mut()?;
                last.local = local;
            }
        }
        let _ = min_y;
        geo.ground_offset = ground_scale * 0.25;
        geo.depth = if max_z >= min_z { max_z - min_z } else { 0.0 };
        if special {
            geo.depth = geo.depth.max(1.0);
        }
        (!geo.parts.is_empty()).then_some(geo)
    }
}

fn tracing_free_warn(_id: &Identifier, _e: &mcr_assets::AssetError) {}

/// Visual copies of a dropped stack, as vanilla: at most five whatever the server claims.
pub fn clone_count(count: i32) -> usize {
    match count {
        i32::MIN..=1 => 1,
        2..=16 => 2,
        17..=32 => 3,
        33..=48 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::clone_count;

    #[test]
    fn clone_counts_are_clamped() {
        assert_eq!(clone_count(1), 1);
        assert_eq!(clone_count(40), 4);
        assert_eq!(clone_count(i32::MAX), 5);
        assert_eq!(clone_count(i32::MIN), 1);
    }
}

#[cfg(test)]
mod special_tests {
    use super::*;

    fn shield(translation: [f32; 3]) -> mcr_assets::ItemTransform {
        mcr_assets::ItemTransform {
            rotation: [0.0, 180.0, 5.0],
            translation: translation.map(|c| c / 16.0),
            scale: [1.25; 3],
        }
    }

    #[test]
    fn first_person_shields_sit_on_their_arm_and_mirror() {
        let right = special_base(Some(&shield([-10.0, 1.75, -10.0])), false);
        let left = special_base(Some(&shield([10.0, 0.0, -10.0])), true);
        let r = right.transform_point3(Vec3::ZERO);
        let l = left.transform_point3(Vec3::ZERO);
        assert!(r.x.abs() < 0.1 && (r.x + l.x).abs() < 0.01, "{r} {l}");
        assert!((r.z - l.z).abs() < 1e-5 && r.y < -0.4 && l.y < -0.4);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn hostile_variants_collapse_into_few_keys() {
        let key = |dyed: i32| Key {
            using: 0,
            item: 1,
            dyed: dyed & 0x00FF_FFFF,
            model_hash: 0,
            context: ItemContext::Ground,
            variant: 0,
        };
        assert_eq!(key(0x0100_0000), key(0));
        assert_eq!(key(i32::MIN), key(0));
    }
}
