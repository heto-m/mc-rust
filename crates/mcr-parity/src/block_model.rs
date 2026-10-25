//! Block model baking: every block state's baked quads, by multipart part, weighted variant and cull
//! face. Vanilla's `ModelBakery` (on sprites whose atlas UVs map back to sprite-local UVs) against
//! `mcr-assets`'s `ModelBaker`. Quads keep their order inside a cull bucket, the order the renderer
//! draws them in; the buckets themselves are a map, since the renderer visits them in a fixed order.

use std::collections::HashMap;

use mcr_assets::{BakedQuad, BakedVariant, Face, ModelBaker, ResourceManager, SpriteId};
use serde_json::{Map, Value, json};

use crate::{Case, Env, Harness, Result, Rng, round};

pub(crate) const STEP: f64 = 1.0 / 1024.0;
const MISSING_SPRITE: &str = "minecraft:missingno";

#[derive(Debug)]
pub struct BlockModelHarness;

pub(crate) fn face_name(face: Face) -> &'static str {
    match face {
        Face::Down => "down",
        Face::Up => "up",
        Face::North => "north",
        Face::South => "south",
        Face::West => "west",
        Face::East => "east",
    }
}

/// Our sprite names, with sprites that have no texture shown as the missing sprite like the renderer.
pub(crate) struct SpriteNames<'a> {
    resources: &'a ResourceManager,
    known: HashMap<SpriteId, String>,
}

impl<'a> SpriteNames<'a> {
    pub(crate) fn new(resources: &'a ResourceManager) -> Self {
        Self {
            resources,
            known: HashMap::new(),
        }
    }

    pub(crate) fn name(&mut self, baker: &ModelBaker, id: SpriteId) -> String {
        if let Some(n) = self.known.get(&id) {
            return n.clone();
        }
        let sprite = baker.sprites().get(id);
        let name = if self
            .resources
            .exists(&sprite.resource_path("textures", ".png"))
        {
            sprite.to_string()
        } else {
            MISSING_SPRITE.to_owned()
        };
        self.known.insert(id, name.clone());
        name
    }
}

fn quad(q: &BakedQuad, sprite: String) -> Value {
    let vertices: Vec<Value> = q
        .vertices
        .iter()
        .map(|v| {
            let mut out: Vec<f64> = v.pos.iter().map(|&c| round(f64::from(c), STEP)).collect();
            out.extend(v.uv.iter().map(|&c| round(f64::from(c), STEP)));
            json!(out)
        })
        .collect();
    json!({
        "face": face_name(q.normal),
        "sprite": sprite,
        "tint": q.tint,
        "shade": face_name(q.shade_dir),
        "emission": q.light_emission,
        "v": vertices,
    })
}

fn variant(v: &BakedVariant, baker: &ModelBaker, names: &mut SpriteNames) -> Value {
    let mut buckets: Map<String, Value> = Map::new();
    for q in v.quads.iter() {
        let key = q.cull.map_or("none", face_name);
        let entry = buckets
            .entry(key.to_owned())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(list) = entry {
            list.push(quad(q, names.name(baker, q.sprite)));
        }
    }
    let particle = v
        .particle
        .map_or_else(|| MISSING_SPRITE.to_owned(), |p| names.name(baker, p));
    json!({
        "weight": v.weight,
        "ao": v.ambient_occlusion,
        "particle": particle,
        "quads": buckets,
    })
}

impl Harness for BlockModelHarness {
    fn name(&self) -> &'static str {
        "block_model"
    }

    fn scope(&self) -> &'static [&'static str] {
        &[
            "crates/mcr-assets/src/bake.rs",
            "crates/mcr-assets/src/blockstate.rs",
            "crates/mcr-assets/src/geometry.rs",
            "crates/mcr-assets/src/model.rs",
            "crates/mcr-assets/src/resources.rs",
            "crates/mcr-data/src",
            "crates/mcr-parity/src/block_model.rs",
        ]
    }

    fn count(&self) -> usize {
        mcr_data::v26_3().state_count()
    }

    fn case(&self, id: usize, _rng: &mut Rng) -> Value {
        let version = mcr_data::v26_3();
        let state = id as u32;
        let props: Map<String, Value> = version
            .state_properties(state)
            .map(|(k, v)| (k.to_owned(), json!(v)))
            .collect();
        json!({
            "state": state,
            "block": version.state_block_name(state).unwrap_or("minecraft:air"),
            "props": props,
        })
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let resources = ResourceManager::vanilla(&env.client_jar)?;
        let mut baker = ModelBaker::new(&resources);
        let mut names = SpriteNames::new(&resources);
        let mut out = Vec::with_capacity(cases.len());
        for case in cases {
            let block = case.case["block"].as_str().unwrap_or("minecraft:air");
            let props: Vec<(&str, &str)> = case.case["props"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.as_str(), v.as_str()?)))
                        .collect()
                })
                .unwrap_or_default();
            out.push(match baker.bake_block_state(block, &props) {
                Err(_) => json!({"missing": true}),
                Ok(model) => {
                    let parts: Vec<Value> = model
                        .parts
                        .iter()
                        .map(|p| {
                            Value::Array(
                                p.variants
                                    .iter()
                                    .map(|v| variant(v, &baker, &mut names))
                                    .collect(),
                            )
                        })
                        .collect();
                    json!({"parts": parts})
                }
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_case_per_block_state() {
        let h = BlockModelHarness;
        let mut rng = Rng::new(0);
        let stone = mcr_data::v26_3().block_by_name("minecraft:stone").unwrap();
        let first = mcr_data::v26_3().block_info(stone).unwrap().states.0;
        let case = h.case(first as usize, &mut rng);
        assert_eq!(case["block"], "minecraft:stone");
        assert_eq!(case["props"], json!({}));
        assert_eq!(h.count(), mcr_data::v26_3().state_count());
    }
}
