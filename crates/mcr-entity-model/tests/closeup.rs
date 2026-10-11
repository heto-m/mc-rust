mod support;

use mcr_entity_model::EntityView;
use mcr_entity_model::glam::{DVec3, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue as V;
use support::*;

fn env_f(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore]
fn closeup() {
    let Some(mut h) = Harness::new() else { return };
    let ty = std::env::var("CU_TYPE").unwrap_or("minecraft:zombie".into());
    let main = std::env::var("CU_ITEM").ok();
    let off = std::env::var("CU_OFF").ok();
    let mut data = Data::default();
    if let Ok(spec) = std::env::var("CU_DATA") {
        for pair in spec.split(',') {
            let Some((i, val)) = pair.split_once('=') else {
                continue;
            };
            let i: u8 = i.parse().unwrap();
            let value = if let Some(b) = val.strip_prefix('b') {
                V::Boolean(b == "1")
            } else if let Some(b) = val.strip_prefix('y') {
                V::Byte(b.parse().unwrap())
            } else if let Some(f) = val.strip_prefix('f') {
                V::Float(f.parse().unwrap())
            } else {
                V::Int(val.parse().unwrap())
            };
            data = data.with(i, value);
        }
    }
    let mut eq: [ItemStack; 8] = Default::default();
    if let Some(m) = main {
        eq[0] = item(&m);
    }
    if let Some(o) = off {
        eq[5] = item(&o);
    }
    if let Ok(c) = std::env::var("CU_CHEST") {
        eq[3] = item(&c);
    }
    if let Ok(d) = std::env::var("CU_ITEMDATA") {
        data = data.with(8, V::ItemStack(item(&d)));
    }
    let reg = TestRegistries::vanilla();
    let (w, hh) = (600u32, 600u32);
    let dist = env_f("CU_DIST", 3.5);
    let cy = env_f("CU_Y", 1.0);
    let mut strips = Vec::new();
    for az in [0.0f32, 90.0, 180.0] {
        let mut v = EntityView::named(&ty, &data.0);
        v.equipment = Some(&eq);
        v.limb_amount = env_f("CU_WALK", 0.0);
        v.limb_swing = env_f("CU_SWING", 2.0);
        v.attack_time = env_f("CU_ATTACK", 0.0);
        v.age_ticks = env_f("CU_AGE", 10.0);
        v.pitch = env_f("CU_PITCH", 0.0);
        v.head_yaw = env_f("CU_HEADYAW", 0.0);
        v.on_ground = env_f("CU_GROUND", 1.0) > 0.5;
        v.death_time = env_f("CU_DEATH", 0.0);
        v.glide.ticks = env_f("CU_GLIDE", 0.0);
        v.fuse = std::env::var("CU_FUSE").ok().and_then(|f| f.parse().ok());
        if let Ok(r) = std::env::var("CU_YROT") {
            v.glide.y_rot = r.parse().ok();
        }
        if let Ok(w) = std::env::var("CU_WINGS") {
            let w: Vec<f32> = w.split(',').filter_map(|x| x.parse().ok()).collect();
            if let [x, y, z] = w[..] {
                v.glide.wings = [x, y, z];
            }
        }
        let a = az.to_radians();
        let cam = Camera {
            position: DVec3::new(
                f64::from(dist * a.sin()),
                f64::from(cy + 0.2),
                f64::from(dist * a.cos()),
            ),
            target: Vec3::new(0.0, cy, 0.0),
            fov_deg: 35.0,
        };
        let d = cam.target.as_dvec3() - cam.position;
        let yaw = (-d.x).atan2(d.z).to_degrees() as f32;
        let pitch = (-d.y / d.length()).asin().to_degrees() as f32;
        h.scene.set_camera(cam.position.to_array(), yaw, pitch);
        h.scene.update(&reg, std::iter::once(v));
        strips.push(h.render(&cam.frame(w, hh), w, hh));
    }
    let mut all: Vec<u8> = Vec::new();
    for y in 0..hh as usize {
        for s in &strips {
            all.extend_from_slice(&s.pixels[y * w as usize * 4..(y + 1) * w as usize * 4]);
        }
    }
    let shot = Shot {
        pixels: all,
        width: w * 3,
        height: hh,
    };
    let name = std::env::var("CU_NAME").unwrap_or("closeup".into());
    println!("{}", shot.save(&name).display());
}
