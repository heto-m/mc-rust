use glam::{Mat4, Vec3};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, component_type_id};
use rustc_hash::FxHashMap;

use crate::assets::ImageRgba;
use crate::scene::EntityScene;

const MAX_MAPS: usize = 256;
const MAX_DECORATIONS: usize = 256;
const LAYER: f32 = 0.25 / 128.0;

/// A marker drawn over a map; `x`/`y` are the wire bytes (half-pixels from the centre).
#[derive(Debug, Clone, PartialEq)]
pub struct MapDecoration {
    pub sprite: String,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
}

pub fn stack_map_id(stack: &ItemStack) -> Option<i32> {
    let id = component_type_id("map_id")?;
    match stack.components.get(id)? {
        DataComponent::MapId(v) => Some(v.0),
        _ => None,
    }
}

pub(crate) type MapTable = FxHashMap<i32, Vec<MapDecoration>>;

impl EntityScene {
    /// Registers or replaces the 128x128 image and markers of map `id`.
    pub fn set_map(&mut self, id: i32, image: ImageRgba, mut decorations: Vec<MapDecoration>) {
        if !self.maps.contains_key(&id) && self.maps.len() >= MAX_MAPS && !self.evict_oldest_map() {
            return;
        }
        decorations.truncate(MAX_DECORATIONS);
        self.maps.insert(id, decorations);
        self.set_dynamic_texture(&format!("dyn:map/{id}"), image);
        self.flush_textures();
    }

    /// Frees the map least recently drawn; `false` when every map was drawn this frame.
    fn evict_oldest_map(&mut self) -> bool {
        let frame = self.gpu.frame();
        let oldest = self
            .maps
            .keys()
            .map(|&id| {
                (
                    self.textures
                        .last_used(&format!("dyn:map/{id}"))
                        .unwrap_or(0),
                    id,
                )
            })
            .min();
        let Some((used, id)) = oldest else {
            return false;
        };
        if used >= frame && used != 0 {
            return false;
        }
        self.remove_map(id);
        true
    }

    pub fn remove_map(&mut self, id: i32) {
        self.maps.remove(&id);
        self.textures
            .remove(&format!("dyn:map/{id}"), &mut self.gpu);
    }

    pub fn has_map(&self, id: i32) -> bool {
        self.maps.contains_key(&id)
    }

    /// Draws map `id` on a unit quad at `matrix` (facing -Z, the viewer on that side).
    pub fn emit_map(
        &mut self,
        id: i32,
        matrix: Mat4,
        light: [u8; 2],
        background: bool,
        origin: [f64; 3],
    ) {
        if background {
            self.emit_quad_at(
                "map/map_background",
                [142.0 / 128.0; 2],
                matrix,
                light,
                origin,
            );
        }
        let Some(decorations) = self.maps.get(&id).cloned() else {
            return;
        };
        self.emit_quad_at(
            &format!("dyn:map/{id}"),
            [1.0, 1.0],
            matrix * Mat4::from_translation(Vec3::new(0.0, 0.0, -LAYER)),
            light,
            origin,
        );
        for (i, d) in decorations.iter().enumerate() {
            let cx = f32::from(d.x) / 2.0 / 128.0;
            let cy = -f32::from(d.y) / 2.0 / 128.0;
            let m = matrix
                * Mat4::from_translation(Vec3::new(cx, cy, -LAYER * (2.0 + i as f32 * 0.1)))
                * Mat4::from_rotation_z((f32::from(d.rot) * 22.5).to_radians());
            self.emit_quad_at(
                &format!("map/decorations/{}", d.sprite),
                [8.0 / 128.0; 2],
                m,
                light,
                origin,
            );
        }
    }
}
