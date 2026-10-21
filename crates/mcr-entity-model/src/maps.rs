use glam::{Mat4, Vec3};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, component_type_id};
use rustc_hash::FxHashMap;

use crate::assets::ImageRgba;
use crate::scene::EntityScene;

const MAX_MAPS: usize = 256;
const MAX_DECORATIONS: usize = 256;
const MAX_LABELED_MAPS: usize = 16;
const LAYER: f32 = 0.25 / 128.0;
/// `MapRenderer#render` puts every name at one depth, level with the 36th icon; later icons cover names.
const LABEL_DEPTH_INDEX: f32 = 35.5;

/// A marker drawn over a map; `x`/`y` are the wire bytes (half-pixels from the centre).
#[derive(Debug, Clone, PartialEq)]
pub struct MapDecoration {
    pub sprite: String,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    /// `MapDecorationType.showOnItemFrame`.
    pub on_frame: bool,
}

/// Decoration names baked over a map (`LABEL_SCALE` texels per map pixel): for held maps, and for
/// item frames when some named decoration is hidden there.
#[derive(Debug, Clone, Default)]
pub struct MapLabels {
    pub held: Option<ImageRgba>,
    pub frame: Option<ImageRgba>,
    /// Item frames show every named decoration, so they reuse `held`.
    pub frame_same: bool,
}

pub const LABEL_SCALE: u32 = 4;

#[derive(Debug, Clone, Default)]
pub(crate) struct MapEntry {
    decorations: Vec<MapDecoration>,
    held_labels: bool,
    frame_labels: Option<&'static str>,
}

pub fn stack_map_id(stack: &ItemStack) -> Option<i32> {
    let id = component_type_id("map_id")?;
    match stack.components.get(id)? {
        DataComponent::MapId(v) => Some(v.0),
        _ => None,
    }
}

pub(crate) type MapTable = FxHashMap<i32, MapEntry>;

impl EntityScene {
    /// Registers or replaces the 128x128 image, markers and name labels of map `id`; `None` labels
    /// keep the ones already uploaded.
    pub fn set_map(
        &mut self,
        id: i32,
        image: ImageRgba,
        mut decorations: Vec<MapDecoration>,
        labels: Option<MapLabels>,
    ) {
        if !self.maps.contains_key(&id) && self.maps.len() >= MAX_MAPS && !self.evict_oldest_map() {
            return;
        }
        decorations.truncate(MAX_DECORATIONS);
        let previous = self.maps.get(&id).map(|e| (e.held_labels, e.frame_labels));
        let (held_labels, frame_labels) = match labels {
            None => previous.unwrap_or((false, None)),
            Some(mut labels) => {
                let labeled = self
                    .maps
                    .iter()
                    .filter(|&(&other, e)| other != id && e.held_labels)
                    .count();
                if labeled >= MAX_LABELED_MAPS {
                    labels = MapLabels::default();
                }
                let held_labels = labels.held.is_some();
                let frame_labels = if labels.frame_same {
                    held_labels.then_some("held")
                } else {
                    labels.frame.is_some().then_some("frame")
                };
                for (kind, image) in [("held", labels.held), ("frame", labels.frame)] {
                    let key = format!("dyn:map_labels/{kind}/{id}");
                    match image {
                        Some(image) => self.set_dynamic_texture(&key, image),
                        None => self.textures.remove(&key, &mut self.gpu),
                    }
                }
                (held_labels, frame_labels)
            }
        };
        self.maps.insert(
            id,
            MapEntry {
                decorations,
                held_labels,
                frame_labels,
            },
        );
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
        for kind in ["held", "frame"] {
            self.textures
                .remove(&format!("dyn:map_labels/{kind}/{id}"), &mut self.gpu);
        }
    }

    pub fn has_map(&self, id: i32) -> bool {
        self.maps.contains_key(&id)
    }

    /// Draws map `id` on a unit quad at `matrix` (facing -Z, the viewer on that side); a map in an
    /// item frame (`held` false) has no paper background and skips `on_frame == false` markers.
    pub fn emit_map(
        &mut self,
        id: i32,
        matrix: Mat4,
        light: [u8; 2],
        held: bool,
        origin: [f64; 3],
    ) {
        if held {
            self.emit_quad_at(
                "map/map_background",
                [142.0 / 128.0; 2],
                matrix,
                light,
                origin,
            );
        }
        let Some(entry) = self.maps.get(&id) else {
            return;
        };
        let labels = if held {
            entry.held_labels.then_some("held")
        } else {
            entry.frame_labels
        };
        let decorations: Vec<(String, Mat4)> = entry
            .decorations
            .iter()
            .filter(|d| held || d.on_frame)
            .enumerate()
            .map(|(i, d)| {
                let cx = -f32::from(d.x) / 2.0 / 128.0;
                let cy = -f32::from(d.y) / 2.0 / 128.0;
                let m = matrix
                    * Mat4::from_translation(Vec3::new(cx, cy, -LAYER * (2.0 + i as f32 * 0.1)))
                    * Mat4::from_rotation_z((f32::from(d.rot) * 22.5).to_radians());
                (format!("map/decorations/{}", d.sprite), m)
            })
            .collect();
        self.emit_quad_at(
            &format!("dyn:map/{id}"),
            [1.0, 1.0],
            matrix * Mat4::from_translation(Vec3::new(0.0, 0.0, -LAYER)),
            light,
            origin,
        );
        for (sprite, m) in &decorations {
            self.emit_quad_at(sprite, [8.0 / 128.0; 2], *m, light, origin);
        }
        let top = -LAYER * (2.0 + LABEL_DEPTH_INDEX * 0.1);
        if let Some(kind) = labels {
            self.emit_quad_with(
                &format!("dyn:map_labels/{kind}/{id}"),
                [1.0, 1.0],
                matrix * Mat4::from_translation(Vec3::new(0.0, 0.0, top)),
                light,
                origin,
                crate::gpu::Pipeline::Translucent,
            );
        }
    }
}
