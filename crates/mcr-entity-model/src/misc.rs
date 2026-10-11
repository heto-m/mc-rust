use glam::{Mat4, Quat, Vec3};
use mcr_buf::IdOr;
use mcr_protocol::types::entity_data::EntityDataValue;

use crate::catalog::{BuildFn, PlanCx};
use crate::idx;
use crate::items::clone_count;
use crate::mobs::short;
use crate::plan::{FreeGeo, FreeItem, ItemContext, ItemSource, Part, Plan};
use crate::rig::Rig;
use crate::view::EntityView;

const PAINTINGS: &[(&str, u8, u8)] = &[
    ("alban", 1, 1),
    ("aztec", 1, 1),
    ("aztec2", 1, 1),
    ("backyard", 3, 4),
    ("baroque", 2, 2),
    ("bomb", 1, 1),
    ("bouquet", 3, 3),
    ("burning_skull", 4, 4),
    ("bust", 2, 2),
    ("cavebird", 3, 3),
    ("changing", 4, 2),
    ("cotan", 3, 3),
    ("courbet", 2, 1),
    ("creebet", 2, 1),
    ("dennis", 3, 3),
    ("donkey_kong", 4, 3),
    ("earth", 2, 2),
    ("endboss", 3, 3),
    ("fern", 3, 3),
    ("fighters", 4, 2),
    ("finding", 4, 2),
    ("fire", 2, 2),
    ("graham", 1, 2),
    ("humble", 2, 2),
    ("kebab", 1, 1),
    ("lowmist", 4, 2),
    ("match", 2, 2),
    ("meditative", 1, 1),
    ("orb", 4, 4),
    ("owlemons", 3, 3),
    ("passage", 4, 2),
    ("pigscene", 4, 4),
    ("plant", 1, 1),
    ("pointer", 4, 4),
    ("pond", 3, 4),
    ("pool", 2, 1),
    ("prairie_ride", 1, 2),
    ("sea", 2, 1),
    ("skeleton", 4, 3),
    ("skull_and_roses", 2, 2),
    ("stage", 2, 2),
    ("sunflowers", 3, 3),
    ("sunset", 2, 1),
    ("tides", 3, 3),
    ("unpacked", 4, 4),
    ("void", 2, 2),
    ("wanderer", 1, 2),
    ("wasteland", 1, 1),
    ("water", 2, 2),
    ("wind", 2, 2),
    ("wither", 2, 2),
];

pub(crate) fn builder(name: &str) -> Option<BuildFn> {
    let n = short(name);
    if n.ends_with("_boat") || n.ends_with("_raft") {
        return Some(boat);
    }
    if n.ends_with("minecart") {
        return Some(minecart);
    }
    Some(match n {
        "item" => item_entity,
        "item_frame" | "glow_item_frame" => item_frame,
        "painting" => painting,
        "falling_block" => falling_block,
        "tnt" => tnt,
        "block_display" => block_display,
        "item_display" => item_display,
        "arrow" | "spectral_arrow" => arrow,
        "firework_rocket" => firework,
        "experience_orb" => xp_orb,
        "fishing_bobber" => bobber,
        "lightning_bolt" => lightning,
        "snowball" | "egg" | "ender_pearl" | "experience_bottle" | "splash_potion"
        | "lingering_potion" | "eye_of_ender" | "fireball" | "small_fireball" => thrown_item,
        "dragon_fireball" => dragon_fireball,
        _ => return None,
    })
}

/// `ThrowableProjectile`, `Fireball` and `EyeOfEnder` skip their first two ticks within 3.5 blocks of
/// the camera entity, so a fresh throw does not cover the screen.
pub fn hidden_while_fresh(type_name: &str) -> bool {
    matches!(
        short(type_name),
        "snowball"
            | "egg"
            | "ender_pearl"
            | "experience_bottle"
            | "splash_potion"
            | "lingering_potion"
            | "eye_of_ender"
            | "fireball"
            | "small_fireball"
    )
}

fn hash01(id: i32, salt: u32) -> f32 {
    let mut x = (id as u32)
        .wrapping_mul(2654435761)
        .wrapping_add(salt.wrapping_mul(40503));
    x ^= x >> 15;
    x = x.wrapping_mul(2246822519);
    x ^= x >> 13;
    (x & 0xFFFF) as f32 / 65536.0
}

fn item_entity(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let Some(stack) = v.stack(idx::ITEM_STACK) else {
        return;
    };
    let bob_offset = hash01(v.id, 1) * std::f32::consts::TAU;
    let bob = (v.age_ticks / 10.0 + bob_offset).sin() * 0.1 + 0.1;
    let spin = v.age_ticks / 20.0 + bob_offset;
    for i in 0..clone_count(stack.count) {
        let off = if i == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(
                (hash01(v.id, i as u32 * 3) * 2.0 - 1.0) * 0.15,
                (hash01(v.id, i as u32 * 3 + 1) * 2.0 - 1.0) * 0.15,
                (hash01(v.id, i as u32 * 3 + 2) * 2.0 - 1.0) * 0.15,
            )
        };
        plan.free_items.push(FreeItem {
            source: ItemSource::Data(idx::ITEM_STACK),
            context: ItemContext::Ground,
            local: Mat4::from_translation(Vec3::new(0.0, bob, 0.0) + off)
                * Mat4::from_rotation_y(spin),
            ground_lift: true,
            billboard: false,
            post: Mat4::IDENTITY,
        });
    }
}

/// `ThrownItemRenderer`: the ground-context item turned to the camera, one copy, no bob or spin.
fn thrown_item(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    let (scale, bright) = match cx.type_name(v).map(short) {
        Some("fireball") => (3.0, true),
        Some("small_fireball") => (0.75, true),
        Some("eye_of_ender") => (1.0, true),
        _ => (1.0, false),
    };
    plan.skip_rotation = true;
    plan.bright = bright;
    plan.free_items.push(FreeItem {
        source: ItemSource::Data(idx::ITEM_STACK),
        context: ItemContext::Ground,
        local: Mat4::from_scale(Vec3::splat(scale)),
        ground_lift: false,
        billboard: true,
        post: Mat4::IDENTITY,
    });
}

/// `DragonFireballRenderer`: a camera-facing quad, twice block size, full block light.
fn dragon_fireball(_cx: &mut PlanCx<'_>, _v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    plan.bright = true;
    plan.free_geo.push(FreeGeo::Sprite {
        texture: "entity/enderdragon/dragon_fireball",
        uv: [0.0, 0.0, 1.0, 1.0],
        local: Mat4::IDENTITY,
        post: Mat4::from_scale(Vec3::splat(2.0))
            * Mat4::from_translation(Vec3::new(0.0, 0.25, 0.0)),
        tint: [255; 4],
        translucent: false,
        billboard: true,
    });
}

fn firework(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let angled = v.boolean(10);
    let post = if angled {
        Mat4::from_rotation_z(std::f32::consts::PI)
            * Mat4::from_rotation_y(std::f32::consts::PI)
            * Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2)
    } else {
        Mat4::IDENTITY
    };
    plan.free_items.push(FreeItem {
        source: ItemSource::Data(idx::ITEM_STACK),
        context: ItemContext::Ground,
        local: Mat4::from_translation(Vec3::new(0.0, 0.125, 0.0)),
        ground_lift: false,
        billboard: true,
        post,
    });
}

fn xp_orb(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let icon = match v.object_data {
        2477.. => 10,
        1237..=2476 => 9,
        617..=1236 => 8,
        307..=616 => 7,
        149..=306 => 6,
        73..=148 => 5,
        37..=72 => 4,
        17..=36 => 3,
        7..=16 => 2,
        3..=6 => 1,
        _ => 0,
    };
    let (u, w) = ((icon % 4) as f32 * 0.25, (icon / 4) as f32 * 0.25);
    let phase = v.age_ticks / 2.0;
    let r = ((phase.sin() + 1.0) * 0.5 * 255.0) as u8;
    let b = (((phase + std::f32::consts::PI * 4.0 / 3.0).sin() + 1.0) * 0.1 * 255.0) as u8;
    plan.free_geo.push(FreeGeo::Sprite {
        texture: "entity/experience/experience_orb",
        uv: [u, w, u + 0.25, w + 0.25],
        local: Mat4::from_translation(Vec3::new(0.0, 0.1, 0.0)),
        post: Mat4::from_scale(Vec3::splat(0.3))
            * Mat4::from_translation(Vec3::new(0.0, 0.25, 0.0)),
        tint: [r, 255, b, 128],
        translucent: true,
        billboard: true,
    });
}

fn bobber(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    plan.free_geo.push(FreeGeo::Sprite {
        texture: "entity/fishing/fishing_hook",
        uv: [0.0, 0.0, 1.0, 1.0],
        local: Mat4::IDENTITY,
        post: Mat4::from_scale(Vec3::splat(0.5)),
        tint: [255; 4],
        translucent: false,
        billboard: true,
    });
    if let Some(t) = v.tether {
        let d = Vec3::new(
            (t[0] - v.pos[0]) as f32,
            (t[1] - v.pos[1]) as f32 - 0.25,
            (t[2] - v.pos[2]) as f32,
        );
        let point = |a: f32| Vec3::new(d.x * a, d.y * (a * a + a) * 0.5 + 0.25, d.z * a);
        for i in 0..16 {
            plan.free_geo.push(FreeGeo::Line {
                a: point(i as f32 / 16.0),
                b: point((i + 1) as f32 / 16.0),
                width: 0.015,
            });
        }
    }
}

fn lightning(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    plan.free_geo.push(FreeGeo::Bolt {
        seed: (v.id as u32).wrapping_mul(2654435761),
    });
}

fn direction_of(v: &EntityView<'_>, index: u8) -> u8 {
    match v.raw(index) {
        Some(EntityDataValue::Direction(d)) => d.id() as u8,
        _ => 2,
    }
}

fn step(dir: u8) -> Vec3 {
    match dir {
        0 => Vec3::NEG_Y,
        1 => Vec3::Y,
        2 => Vec3::NEG_Z,
        3 => Vec3::Z,
        4 => Vec3::NEG_X,
        _ => Vec3::X,
    }
}

fn item_frame(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let dir = direction_of(v, idx::FRAME_DIRECTION);
    let glow = cx.type_name(v) == Some("minecraft:glow_item_frame");
    let has_item = v.stack(idx::FRAME_ITEM).is_some();
    let (xrot, yrot) = match dir {
        0 => (90.0f32, 180.0f32),
        1 => (-90.0, 180.0),
        2 => (0.0, 0.0),
        3 => (0.0, 180.0),
        4 => (0.0, 180.0 - 90.0),
        _ => (0.0, 180.0 - 270.0),
    };
    let yrot = match dir {
        2 => 0.0,
        3 => 180.0,
        4 => 90.0,
        5 => -90.0,
        _ => yrot,
    };
    let base = Mat4::from_translation(Vec3::splat(0.5))
        * Mat4::from_rotation_x(xrot.to_radians())
        * Mat4::from_rotation_y(yrot.to_radians());
    let map = v.stack(idx::FRAME_ITEM).and_then(crate::maps::stack_map_id);
    let model = match (glow, map.is_some()) {
        (true, true) => "minecraft:block/glow_item_frame_map",
        (true, false) => "minecraft:block/glow_item_frame",
        (false, true) => "minecraft:block/item_frame_map",
        (false, false) => "minecraft:block/item_frame",
    };
    let _ = has_item;
    plan.free_geo.push(FreeGeo::Model {
        id: model.to_owned(),
        local: base,
    });
    if let Some(id) = map {
        let rotation = (v.int(idx::FRAME_ROTATION).unwrap_or(0) % 4) as f32;
        plan.free_geo.push(FreeGeo::Map {
            id,
            local: base
                * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.4375 - 1.0 / 128.0))
                * Mat4::from_rotation_z((rotation * 90.0).to_radians())
                * Mat4::from_rotation_z(180f32.to_radians()),
        });
    } else if v.stack(idx::FRAME_ITEM).is_some() {
        let rotation = v.int(idx::FRAME_ROTATION).unwrap_or(0) as f32;
        plan.free_items.push(FreeItem {
            source: ItemSource::Data(idx::FRAME_ITEM),
            context: ItemContext::Fixed,
            local: base
                * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.4375))
                * Mat4::from_rotation_z((rotation * 45.0).to_radians())
                * Mat4::from_scale(Vec3::splat(0.5)),
            ground_lift: false,
            billboard: false,
            post: Mat4::IDENTITY,
        });
    }
}

fn painting(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let (name, w, h) = match v.raw(idx::PAINTING_VARIANT) {
        Some(EntityDataValue::PaintingVariant(IdOr::Inline(p))) => (
            short(p.asset_id.as_str()).to_owned(),
            p.width.clamp(1, 16) as u8,
            p.height.clamp(1, 16) as u8,
        ),
        Some(EntityDataValue::PaintingVariant(IdOr::Id(id))) => {
            let n = cx
                .reg
                .entry_name("minecraft:painting_variant", *id)
                .map(|n| short(n).to_owned())
                .unwrap_or_else(|| "kebab".to_owned());
            let (w, h) = PAINTINGS
                .iter()
                .find(|p| p.0 == n)
                .map_or((1, 1), |p| (p.1, p.2));
            (n, w, h)
        }
        _ => ("kebab".to_owned(), 1, 1),
    };
    let dir = direction_of(v, idx::PAINTING_DIRECTION);
    let data2d = match dir {
        3 => 0,
        4 => 1,
        2 => 2,
        _ => 3,
    };
    let toward = step(dir);
    let left = Vec3::new(toward.z, 0.0, -toward.x);
    let shift = |n: u8| if n.is_multiple_of(2) { 0.5 } else { 0.0 };
    let centre =
        Vec3::splat(0.5) - toward * 0.46875 + left * shift(w) + Vec3::new(0.0, shift(h), 0.0);
    let rot = Mat4::from_translation(centre)
        * Mat4::from_rotation_y((180.0 - data2d as f32 * 90.0).to_radians());
    let size = [f32::from(w), f32::from(h)];
    plan.free_geo.push(FreeGeo::Quad {
        texture: format!("painting/{name}"),
        size,
        local: rot * Mat4::from_translation(Vec3::new(0.0, 0.0, -0.03125)),
        tint: [255; 4],
    });
    plan.free_geo.push(FreeGeo::Quad {
        texture: "painting/back".to_owned(),
        size,
        local: rot
            * Mat4::from_translation(Vec3::new(0.0, 0.0, 0.03125))
            * Mat4::from_rotation_y(std::f32::consts::PI),
        tint: [255; 4],
    });
}

pub(crate) fn state_by_name(name: &str) -> i32 {
    let v = mcr_data::v26_3();
    v.block_by_name(name)
        .and_then(|b| v.block_info(b))
        .map_or(0, |i| i.default_state as i32)
}

fn falling_block(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    plan.free_geo.push(FreeGeo::Block {
        state: v.object_data,
        local: Mat4::from_translation(Vec3::new(-0.5, 0.0, -0.5)),
    });
}

/// `TntRenderer.getSwellAmount`: the last ten fuse ticks grow the block by up to 30%.
pub fn tnt_swell(fuse: f32) -> f32 {
    let g = (1.0 - fuse / 10.0).clamp(0.0, 1.0);
    let g = g * g;
    g * g * 0.3
}

/// `TntRenderer.isLit`: white in alternating five-tick windows.
pub fn tnt_lit(fuse: f32) -> bool {
    fuse >= 0.0 && (fuse / 5.0) as i32 % 2 == 0
}

fn tnt(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let fuse = v
        .fuse
        .unwrap_or_else(|| v.int(idx::TNT_FUSE).unwrap_or(80) as f32 + 1.0);
    let scale = if fuse < 10.0 {
        1.0 + tnt_swell(fuse)
    } else {
        1.0
    };
    plan.flash = tnt_lit(fuse);
    let state = match v.raw(idx::TNT_BLOCK) {
        Some(EntityDataValue::BlockState(s)) => *s,
        _ => state_by_name("minecraft:tnt"),
    };
    plan.free_geo.push(FreeGeo::Block {
        state,
        local: Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0))
            * Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_translation(Vec3::new(-0.5, -0.5, -0.5)),
    });
}

fn state_with(name: &str, props: &[(&str, &str)]) -> i32 {
    let v = mcr_data::v26_3();
    let Some(info) = v.block_by_name(name).and_then(|b| v.block_info(b)) else {
        return 0;
    };
    (info.states.0..info.states.1)
        .find(|&s| {
            props
                .iter()
                .all(|p| v.state_properties(s).any(|(k, val)| (k, val) == *p))
        })
        .unwrap_or(info.default_state) as i32
}

/// `getDisplayBlockState` and `getDisplayOffset` of each minecart type.
fn minecart_contents(name: &str, v: &EntityView<'_>) -> Option<(i32, i32)> {
    let (block, offset) = match name {
        "tnt_minecart" => (state_by_name("minecraft:tnt"), 6),
        "chest_minecart" => (state_by_name("minecraft:chest"), 8),
        "furnace_minecart" => {
            let lit = if v.boolean(idx::MINECART_FURNACE_FUEL) {
                "true"
            } else {
                "false"
            };
            (
                state_with("minecraft:furnace", &[("facing", "north"), ("lit", lit)]),
                6,
            )
        }
        "hopper_minecart" => (state_by_name("minecraft:hopper"), 1),
        "spawner_minecart" => (state_by_name("minecraft:spawner"), 6),
        "command_block_minecart" => (state_by_name("minecraft:command_block"), 6),
        _ => (0, 6),
    };
    let block = match v.raw(idx::MINECART_DISPLAY_BLOCK) {
        Some(EntityDataValue::OptionalBlockState(Some(s))) => *s,
        _ => block,
    };
    let offset = v.int(idx::MINECART_DISPLAY_OFFSET).unwrap_or(offset);
    (block != 0).then_some((block, offset.clamp(-64, 64)))
}

fn display_transform(v: &EntityView<'_>) -> Mat4 {
    let vec = |i: u8, d: Vec3| match v.raw(i) {
        Some(EntityDataValue::Vector3(x)) => Vec3::new(x.x, x.y, x.z),
        _ => d,
    };
    let quat = |i: u8| match v.raw(i) {
        Some(EntityDataValue::Quaternion(q)) => Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize(),
        _ => Quat::IDENTITY,
    };
    let t = vec(11, Vec3::ZERO).clamp(Vec3::splat(-64.0), Vec3::splat(64.0));
    let s = vec(12, Vec3::ONE).clamp(Vec3::splat(-64.0), Vec3::splat(64.0));
    Mat4::from_translation(t)
        * Mat4::from_quat(quat(13))
        * Mat4::from_scale(s)
        * Mat4::from_quat(quat(14))
}

fn block_display(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    if let Some(EntityDataValue::BlockState(s)) = v.raw(23) {
        plan.free_geo.push(FreeGeo::Block {
            state: *s,
            local: display_transform(v),
        });
    }
}

fn item_display(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.skip_rotation = true;
    let context = match v.byte(24).unwrap_or(0) {
        5 => ItemContext::Head,
        7 => ItemContext::Ground,
        _ => ItemContext::Fixed,
    };
    plan.free_items.push(FreeItem {
        source: ItemSource::Data(23),
        context,
        local: display_transform(v),
        ground_lift: false,
        billboard: false,
        post: Mat4::IDENTITY,
    });
}

fn boat(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    let name = short(cx.type_name(v).unwrap_or("minecraft:oak_boat")).to_owned();
    let chest = name.contains("chest");
    let wood = name
        .trim_end_matches("_chest_boat")
        .trim_end_matches("_chest_raft")
        .trim_end_matches("_boat")
        .trim_end_matches("_raft")
        .to_owned();
    let dir = if chest { "chest_boat" } else { "boat" };
    plan.rig = Rig::Static;
    plan.model_y = 0.0;
    plan.extra_offset = Vec3::new(0.0, 0.375, 0.0);
    plan.spin = std::f32::consts::FRAC_PI_2;
    plan.body_yaw_offset = 0.0;
    plan.push(Part::new(
        format!("minecraft:{dir}/{wood}#main"),
        format!("entity/{dir}/{wood}"),
    ));
}

fn minecart(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    let name = short(cx.type_name(v).unwrap_or("minecraft:minecart")).to_owned();
    plan.rig = Rig::Static;
    plan.model_y = 0.0;
    plan.extra_offset = Vec3::new(0.0, 0.375, 0.0);
    plan.spin = std::f32::consts::FRAC_PI_2;
    plan.push(Part::new(
        format!("minecraft:{name}#main"),
        "entity/minecart/minecart",
    ));
    let Some((state, offset)) = minecart_contents(&name, v) else {
        return;
    };
    let mut local = Mat4::from_rotation_y((180.0 - v.body_yaw).to_radians() + plan.spin)
        * Mat4::from_translation(Vec3::new(0.0, 0.375, 0.0))
        * Mat4::from_scale(Vec3::splat(0.75))
        * Mat4::from_translation(Vec3::new(-0.5, (offset - 8) as f32 / 16.0, 0.5))
        * Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
    if let Some(fuse) = v.fuse {
        if fuse > -1.0 && fuse < 10.0 {
            let swell = tnt_swell(fuse);
            local *= Mat4::from_translation(Vec3::new(-swell * 0.5, 0.0, -swell * 0.5))
                * Mat4::from_scale(Vec3::splat(1.0 + swell));
        }
        plan.flash = tnt_lit(fuse);
    }
    plan.free_geo.push(FreeGeo::Block { state, local });
}

fn arrow(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    let spectral = cx.type_name(v) == Some("minecraft:spectral_arrow");
    let tex = if spectral {
        "entity/projectiles/arrow_spectral"
    } else if v.int(11).is_some_and(|c| c != -1) {
        "entity/projectiles/arrow_tipped"
    } else {
        "entity/projectiles/arrow"
    };
    plan.rig = Rig::Static;
    plan.model_y = 0.0;
    plan.skip_rotation = true;
    plan.spin = -(v.yaw - 90.0).to_radians();
    plan.tilt = v.pitch;
    plan.extra_scale = Vec3::new(-1.0, 1.0, 1.0) * 0.05625 * 16.0;
    plan.push(Part::new("minecraft:arrow#main", tex).no_hurt());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::camera_orientation;

    #[test]
    fn billboards_face_the_camera_upright() {
        for (yaw, pitch) in [(0.0f32, 0.0f32), (90.0, 0.0), (-135.0, 40.0), (30.0, -70.0)] {
            let (a, b) = (yaw.to_radians(), pitch.to_radians());
            let forward = Vec3::new(-a.sin() * b.cos(), -b.sin(), a.cos() * b.cos());
            let m = camera_orientation(yaw, pitch);
            let front = m.transform_vector3(Vec3::Z);
            assert!((front + forward).length() < 1e-5, "{yaw} {pitch}: {front}");
            let up = m.transform_vector3(Vec3::Y);
            assert!(up.dot(forward).abs() < 1e-5 && up.y >= 0.0, "{up}");
        }
    }

    #[test]
    fn tnt_flashes_in_five_tick_windows() {
        let lit: Vec<bool> = (0..=20).map(|f| tnt_lit(f as f32)).collect();
        for (f, l) in lit.iter().enumerate() {
            assert_eq!(*l, (f / 5) % 2 == 0, "fuse {f}");
        }
        assert!(tnt_lit(4.99) && !tnt_lit(5.0) && tnt_lit(10.0));
        assert!(!tnt_lit(-0.5), "an unlit minecart never flashes");
    }

    #[test]
    fn tnt_swells_over_the_last_ten_ticks() {
        assert_eq!(tnt_swell(10.0), 0.0);
        assert_eq!(tnt_swell(80.0), 0.0);
        assert!((tnt_swell(5.0) - 0.5f32.powi(4) * 0.3).abs() < 1e-7);
        assert!((tnt_swell(0.0) - 0.3).abs() < 1e-7);
        assert!((tnt_swell(-3.0) - 0.3).abs() < 1e-7);
        assert!(tnt_swell(2.0) > tnt_swell(3.0));
    }

    #[test]
    fn fresh_throws_hide_like_vanilla_projectiles() {
        assert!(hidden_while_fresh("minecraft:snowball"));
        assert!(hidden_while_fresh("minecraft:small_fireball"));
        assert!(!hidden_while_fresh("minecraft:item"));
        assert!(!hidden_while_fresh("minecraft:dragon_fireball"));
    }
}
