//! Entity model poses: vanilla bakes a model layer, builds its `EntityModel` and runs `setupAnim`
//! on a generated render state; ours plans the same entity through `Catalog::build` from an
//! `EntityView` carrying the equivalent inputs, then poses its first part the way the scene does
//! (rig, hidden slots, rotation overrides). Every model part compares by path.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::Arc;

use mcr_assets::ResourceManager;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_entity_model::{
    Binding, Catalog, EntityView, Model, PartPose, Plan, PlanCx, SceneAssets, TextureCache,
    animate, base_anim, idx,
};
use rustc_hash::FxHashMap;
use serde_json::{Map, Value, json};

use crate::{Case, Env, Harness, ParityError, Result, Rng};

const CASES: usize = 3000;
const STEP: f64 = 1.0 / 4096.0;

#[derive(Debug, Clone, Copy)]
enum Gen {
    Bool(u32),
    Float(f32, f32),
    Int(i64, i64),
}

struct Entry {
    entity: &'static str,
    layer: &'static str,
    model: &'static str,
    state: &'static str,
    extras: &'static [(&'static str, Gen)],
}

const fn e(
    entity: &'static str,
    layer: &'static str,
    model: &'static str,
    state: &'static str,
    extras: &'static [(&'static str, Gen)],
) -> Entry {
    Entry {
        entity,
        layer,
        model,
        state,
        extras,
    }
}

const LIVING: &str = "LivingEntityRenderState";

/// Adult models without keyframe animations; class names are relative to `net.minecraft.client.model`
/// and `net.minecraft.client.renderer.entity.state`.
const ENTRIES: &[Entry] = &[
    e(
        "cow",
        "minecraft:cow#main",
        "animal.cow.CowModel",
        LIVING,
        &[],
    ),
    e(
        "pig",
        "minecraft:pig#main",
        "animal.pig.PigModel",
        LIVING,
        &[],
    ),
    e(
        "sheep",
        "minecraft:sheep#main",
        "animal.sheep.SheepModel",
        "SheepRenderState",
        &[],
    ),
    e(
        "polar_bear",
        "minecraft:polar_bear#main",
        "animal.polarbear.PolarBearModel",
        "PolarBearRenderState",
        &[("standScale", Gen::Bool(30))],
    ),
    e(
        "goat",
        "minecraft:goat#main",
        "animal.goat.GoatModel",
        "GoatRenderState",
        &[],
    ),
    e(
        "hoglin",
        "minecraft:hoglin#main",
        "monster.hoglin.HoglinModel",
        "HoglinRenderState",
        &[("attackAnimationRemainingTicks", Gen::Int(0, 10))],
    ),
    e(
        "ravager",
        "minecraft:ravager#main",
        "monster.ravager.RavagerModel",
        "RavagerRenderState",
        &[
            ("attackTicksRemaining", Gen::Float(0.0, 10.0)),
            ("stunnedTicksRemaining", Gen::Float(0.0, 40.0)),
            ("roarAnimation", Gen::Float(0.0, 1.0)),
        ],
    ),
    e(
        "turtle",
        "minecraft:turtle#main",
        "animal.turtle.AdultTurtleModel",
        "TurtleRenderState",
        &[("isOnLand", Gen::Bool(50))],
    ),
    e(
        "creeper",
        "minecraft:creeper#main",
        "monster.creeper.CreeperModel",
        "CreeperRenderState",
        &[],
    ),
    e(
        "spider",
        "minecraft:spider#main",
        "monster.spider.SpiderModel",
        LIVING,
        &[],
    ),
    e(
        "cave_spider",
        "minecraft:cave_spider#main",
        "monster.spider.SpiderModel",
        LIVING,
        &[],
    ),
    e(
        "chicken",
        "minecraft:chicken#main",
        "animal.chicken.AdultChickenModel",
        "ChickenRenderState",
        &[],
    ),
    e(
        "iron_golem",
        "minecraft:iron_golem#main",
        "animal.golem.IronGolemModel",
        "IronGolemRenderState",
        &[
            ("attackTicksRemaining", Gen::Float(0.0, 10.0)),
            ("offerFlowerTick", Gen::Int(0, 400)),
        ],
    ),
    e(
        "snow_golem",
        "minecraft:snow_golem#main",
        "animal.golem.SnowGolemModel",
        LIVING,
        &[],
    ),
    e(
        "endermite",
        "minecraft:endermite#main",
        "monster.endermite.EndermiteModel",
        LIVING,
        &[],
    ),
    e(
        "silverfish",
        "minecraft:silverfish#main",
        "monster.silverfish.SilverfishModel",
        LIVING,
        &[],
    ),
    e(
        "blaze",
        "minecraft:blaze#main",
        "monster.blaze.BlazeModel",
        LIVING,
        &[],
    ),
    e(
        "bat",
        "minecraft:bat#main",
        "ambient.BatModel",
        "BatRenderState",
        &[],
    ),
    e(
        "squid",
        "minecraft:squid#main",
        "animal.squid.SquidModel",
        "SquidRenderState",
        &[("tentacleSync", Gen::Float(0.0, 200.0))],
    ),
    e(
        "vex",
        "minecraft:vex#main",
        "monster.vex.VexModel",
        "VexRenderState",
        &[],
    ),
    e(
        "allay",
        "minecraft:allay#main",
        "animal.allay.AllayModel",
        "AllayRenderState",
        &[],
    ),
    e(
        "phantom",
        "minecraft:phantom#main",
        "monster.phantom.PhantomModel",
        "PhantomRenderState",
        &[],
    ),
    e(
        "guardian",
        "minecraft:guardian#main",
        "monster.guardian.GuardianModel",
        "GuardianRenderState",
        &[],
    ),
    e(
        "dolphin",
        "minecraft:dolphin#main",
        "animal.dolphin.DolphinModel",
        "DolphinRenderState",
        &[],
    ),
    e(
        "cod",
        "minecraft:cod#main",
        "animal.fish.CodModel",
        LIVING,
        &[("isInWater", Gen::Bool(70))],
    ),
    e(
        "tadpole",
        "minecraft:tadpole#main",
        "animal.frog.TadpoleModel",
        LIVING,
        &[("isInWater", Gen::Bool(70))],
    ),
    e(
        "wolf",
        "minecraft:wolf#main",
        "animal.wolf.AdultWolfModel",
        "WolfRenderState",
        &[],
    ),
    e(
        "ocelot",
        "minecraft:ocelot#main",
        "animal.feline.AdultOcelotModel",
        "FelineRenderState",
        &[],
    ),
    e(
        "ghast",
        "minecraft:ghast#main",
        "monster.ghast.GhastModel",
        "GhastRenderState",
        &[],
    ),
    e(
        "zombie",
        "minecraft:zombie#main",
        "monster.zombie.ZombieModel",
        "ZombieRenderState",
        &[
            ("swingAnimation", Gen::Float(0.0, 1.0)),
            ("isAggressive", Gen::Bool(40)),
        ],
    ),
    e(
        "skeleton",
        "minecraft:skeleton#main",
        "monster.skeleton.SkeletonModel",
        "SkeletonRenderState",
        &[
            ("swingAnimation", Gen::Float(0.0, 1.0)),
            ("isAggressive", Gen::Bool(40)),
        ],
    ),
    e(
        "enderman",
        "minecraft:enderman#main",
        "monster.enderman.EndermanModel",
        "EndermanRenderState",
        &[
            ("swingAnimation", Gen::Float(0.0, 1.0)),
            ("isCreepy", Gen::Bool(40)),
        ],
    ),
    e(
        "strider",
        "minecraft:strider#main",
        "monster.strider.AdultStriderModel",
        "StriderRenderState",
        &[],
    ),
    e(
        "magma_cube",
        "minecraft:magma_cube#main",
        "monster.slime.MagmaCubeModel",
        "SlimeRenderState",
        &[("squish", Gen::Float(-1.0, 1.0))],
    ),
    e(
        "end_crystal",
        "minecraft:end_crystal#main",
        "object.crystal.EndCrystalModel",
        "EndCrystalRenderState",
        &[],
    ),
];

#[derive(Debug)]
pub struct EntityPoseHarness;

fn float(rng: &mut Rng, lo: f32, hi: f32) -> f32 {
    let t = rng.range(0, 1 << 20) as f32 / (1 << 20) as f32;
    lo + t * (hi - lo)
}

fn value(rng: &mut Rng, g: Gen) -> Value {
    match g {
        Gen::Bool(p) => Value::from(rng.chance(p)),
        Gen::Float(lo, hi) => Value::from(if rng.chance(25) {
            lo
        } else {
            float(rng, lo, hi)
        }),
        Gen::Int(lo, hi) => Value::from(if rng.chance(25) {
            lo
        } else {
            rng.range(lo, hi)
        }),
    }
}

/// Squid tentacle angle from the synced movement clock, as `Squid.aiStep` advances it.
fn tentacle_angle(sync: f32) -> f32 {
    let t = (sync * 0.1) % (PI * 2.0);
    if t < PI {
        let k = t / PI;
        (k * k * PI).sin() * PI * 0.25
    } else {
        0.0
    }
}

/// The render state fields vanilla reads; derived ones are filled the way the entity would.
fn vanilla_fields(entry: &Entry, fields: &Map<String, Value>) -> Map<String, Value> {
    let mut out = fields.clone();
    let get = |k: &str| fields.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let age = get("ageInTicks");
    if let Some(sync) = out.remove("tentacleSync") {
        out.insert(
            "tentacleAngle".into(),
            json!(tentacle_angle(sync.as_f64().unwrap_or(0.0) as f32)),
        );
    }
    if let Some(Value::Bool(stand)) = out.get("standScale") {
        let scale = if *stand { 1.0 } else { 0.0 };
        out.insert("standScale".into(), json!(scale));
    }
    match entry.entity {
        "phantom" => {
            out.insert("flapTime".into(), json!(age));
        }
        "guardian" => {
            out.insert("spikesAnimation".into(), json!(1.0));
            out.insert("tailAnimation".into(), json!(age * 0.45));
        }
        "sheep" => {
            out.insert("headEatAngleScale".into(), json!(get("xRot").to_radians()));
        }
        "dolphin" => {
            out.insert("isMoving".into(), json!(get("walkAnimationSpeed") > 0.01));
        }
        _ => {}
    }
    out
}

impl Harness for EntityPoseHarness {
    fn name(&self) -> &'static str {
        "entity_pose"
    }

    fn scope(&self) -> &'static [&'static str] {
        &["crates/mcr-entity-model/src"]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn tolerance(&self, field: &str) -> f64 {
        // Mth.sin is a 65536-entry table: ~1e-4 off, times part radii up to ~10 sixteenths.
        if matches!(field, "pos" | "rot" | "scale") {
            1.0 / 512.0
        } else {
            0.0
        }
    }

    fn case(&self, id: usize, rng: &mut Rng) -> Value {
        let entry = &ENTRIES[id % ENTRIES.len()];
        let mut fields = Map::new();
        let still = rng.chance(10);
        fields.insert("ageInTicks".into(), json!(float(rng, 0.0, 2000.0)));
        if entry.state != "EndCrystalRenderState" {
            fields.insert(
                "walkAnimationPos".into(),
                json!(if still { 0.0 } else { float(rng, 0.0, 100.0) }),
            );
            fields.insert(
                "walkAnimationSpeed".into(),
                json!(if still { 0.0 } else { float(rng, 0.0, 1.0) }),
            );
            fields.insert("yRot".into(), json!(float(rng, -75.0, 75.0)));
            fields.insert("xRot".into(), json!(float(rng, -90.0, 90.0)));
        }
        for &(name, g) in entry.extras {
            fields.insert(name.into(), value(rng, g));
        }
        json!({
            "entity": entry.entity,
            "layer": entry.layer,
            "model": entry.model,
            "state": entry.state,
            "fields": Value::Object(fields.clone()),
            "vanilla": Value::Object(vanilla_fields(entry, &fields)),
        })
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let extracted = env
            .client_jar
            .parent()
            .map(|d| d.join("extracted"))
            .ok_or_else(|| ParityError::Invalid("client jar has no parent directory".into()))?;
        let resources = Arc::new(ResourceManager::vanilla(&env.client_jar)?);
        let assets = SceneAssets::load(resources, &extracted)
            .map_err(|e| ParityError::Invalid(format!("scene assets: {e}")))?;
        let catalog = Catalog::new();
        let mut tex = TextureCache::default();
        let mut models: HashMap<String, Option<(Model, Binding)>> = HashMap::new();
        let mut out = Vec::with_capacity(cases.len());
        for case in cases {
            let mut cx = PlanCx {
                reg: &(),
                assets: &assets,
                tex: &mut tex,
                camera_angles: (0.0, 0.0),
            };
            out.push(crate::guarded(|| {
                probe_case(&catalog, &mut cx, &assets, &mut models, case)
            }));
        }
        Ok(out)
    }
}

fn probe_case(
    catalog: &Catalog,
    cx: &mut PlanCx<'_>,
    assets: &SceneAssets,
    models: &mut HashMap<String, Option<(Model, Binding)>>,
    case: &Case,
) -> Value {
    let c = &case.case;
    let entity = c["entity"].as_str().unwrap_or("");
    let fields = c["fields"].as_object().cloned().unwrap_or_default();
    let f = |k: &str| fields.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let b = |k: &str| fields.get(k).and_then(Value::as_bool).unwrap_or(false);
    let Some(type_id) = mcr_data::v26_3().entity_type_by_name(&format!("minecraft:{entity}"))
    else {
        return json!({"error": "unknown entity"});
    };
    let mut data: FxHashMap<u8, EntityDataValue> = FxHashMap::default();
    if b("isAggressive") {
        data.insert(idx::MOB_FLAGS, EntityDataValue::Byte(4));
    }
    if b("isCreepy") {
        data.insert(idx::ENDERMAN_CREEPY, EntityDataValue::Boolean(true));
    }
    if b("standScale") {
        data.insert(idx::POLAR_BEAR_STANDING, EntityDataValue::Boolean(true));
    }
    let mut view = EntityView::new(type_id as i32, &data);
    view.age_ticks = f("ageInTicks");
    view.limb_swing = f("walkAnimationPos");
    view.limb_amount = f("walkAnimationSpeed");
    view.head_yaw = f("yRot");
    view.pitch = f("xRot");
    view.attack_time = f("swingAnimation");
    view.attack_ticks = f("attackTicksRemaining") + f("attackAnimationRemainingTicks");
    view.offer_ticks = f("offerFlowerTick");
    view.squish = f("squish");
    view.events.stunned = f("stunnedTicksRemaining");
    view.events.roar = f("roarAnimation");
    if let Some(sync) = fields.get("tentacleSync").and_then(Value::as_f64) {
        view.events.tentacle_sync = Some(sync as f32);
    }
    if fields.contains_key("isInWater") {
        view.on_ground = !b("isInWater");
    }
    if fields.contains_key("isOnLand") {
        view.on_ground = b("isOnLand");
    }
    let mut plan = Plan {
        anim: base_anim(&view),
        ..Plan::default()
    };
    catalog.build(cx, &view, &mut plan);
    let Some(part) = plan.parts.first() else {
        return json!({"error": "no parts"});
    };
    let layer = part.layer.to_string();
    let Some((model, binding)) = models
        .entry(layer.clone())
        .or_insert_with(|| {
            let model = Model::from_layer(assets.layers.get(&layer)?);
            let binding = Binding::new(&model);
            Some((model, binding))
        })
        .as_ref()
    else {
        return json!({"layer": layer, "error": "unknown layer"});
    };
    let mut pose = model.rest_pose();
    let rigged = match part.pose {
        PartPose::Rig | PartPose::Keyframes => {
            animate(plan.rig, &plan.anim, binding, &mut pose);
            true
        }
        PartPose::Own(rig) => {
            animate(rig, &plan.anim, binding, &mut pose);
            false
        }
        PartPose::Rest => false,
    };
    if rigged {
        for slot in &plan.hide {
            if let Some(i) = binding.get(*slot) {
                pose[i].scale = mcr_entity_model::glam::Vec3::ZERO;
            }
        }
        for (slot, rot) in &plan.rot_overrides {
            if let Some(i) = binding.get(*slot) {
                pose[i].rot = mcr_entity_model::glam::Vec3::from(*rot) * (PI / 180.0);
            }
        }
    }
    let mut paths: Vec<String> = Vec::with_capacity(model.bones.len());
    let mut parts = Map::new();
    for (i, bone) in model.bones.iter().enumerate() {
        let path = match bone.parent {
            None => String::new(),
            Some(p) if paths[p as usize].is_empty() => bone.name.clone(),
            Some(p) => format!("{}/{}", paths[p as usize], bone.name),
        };
        paths.push(path.clone());
        let p = &pose[i];
        let value = if p.scale.x == 0.0 && p.scale.y == 0.0 && p.scale.z == 0.0 {
            json!({"hidden": true})
        } else {
            let r = |v: f32| crate::round(f64::from(v), STEP);
            json!({
                "pos": [r(p.pos.x), r(p.pos.y), r(p.pos.z)],
                "rot": [r(p.rot.x), r(p.rot.y), r(p.rot.z)],
                "scale": [r(p.scale.x), r(p.scale.y), r(p.scale.z)],
            })
        };
        parts.insert(path, value);
    }
    json!({"layer": layer, "parts": parts})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_cycle_every_entry() {
        let cases = crate::generate(&EntityPoseHarness);
        for entry in ENTRIES {
            assert!(cases.iter().any(|c| c.case["entity"] == entry.entity));
        }
        let squid = cases.iter().find(|c| c.case["entity"] == "squid").unwrap();
        assert!(squid.case["vanilla"].get("tentacleAngle").is_some());
        assert!(squid.case["vanilla"].get("tentacleSync").is_none());
    }

    #[test]
    fn tentacles_rest_through_the_second_half_of_a_stroke() {
        assert_eq!(tentacle_angle(0.0), 0.0);
        assert!(tentacle_angle(20.0) > 0.0);
        assert_eq!(tentacle_angle(40.0), 0.0);
    }
}
