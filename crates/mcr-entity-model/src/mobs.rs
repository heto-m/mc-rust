use glam::Vec3;
use mcr_protocol::types::entity_data::EntityDataValue;

use crate::catalog::{Builder, PlanCx, Simple, is_baby, simple_entry};
use crate::equipment::add_layers;
use crate::gpu::Pipeline;
use crate::humanoids;
use crate::idx;
use crate::misc;
use crate::plan::{Part, Plan};
use crate::rig::Rig;
use crate::view::{EntityView, slot};

pub const DYE: [u32; 16] = [
    0xF9FFFE, 0xF9801D, 0xC74EBD, 0x3AB3DA, 0xFED83D, 0x80C71F, 0xF38BAA, 0x474F52, 0x9D9D97,
    0x169C9C, 0x8932B8, 0x3C44AA, 0x835432, 0x5E7C16, 0xB02E26, 0x1D1D21,
];

pub fn dye_rgb(i: i32) -> u32 {
    DYE[(i.rem_euclid(16)) as usize]
}

pub fn short(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

pub fn variant_name(
    cx: &PlanCx<'_>,
    v: &EntityView<'_>,
    index: u8,
    registry: &str,
) -> Option<String> {
    let id = v.variant_id(index)?;
    cx.reg.entry_name(registry, id).map(|n| short(n).to_owned())
}

fn pick<'a>(name: &'a Option<String>, known: &[&'a str], default: &'a str) -> &'a str {
    match name.as_deref() {
        Some(n) if known.contains(&n) => n,
        _ => default,
    }
}

static SIMPLE: &[(&str, Simple)] = &[
    simple_entry(
        "minecraft:allay",
        "minecraft:allay#main",
        None,
        "entity/allay/allay",
        None,
        Rig::Look,
    ),
    simple_entry(
        "minecraft:armadillo",
        "minecraft:armadillo#main",
        Some("minecraft:armadillo_baby#main"),
        "entity/armadillo/armadillo",
        Some("entity/armadillo/armadillo_baby"),
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:bat",
        "minecraft:bat#main",
        None,
        "entity/bat/bat",
        None,
        Rig::Bat,
    ),
    simple_entry(
        "minecraft:blaze",
        "minecraft:blaze#main",
        None,
        "entity/blaze/blaze",
        None,
        Rig::Blaze,
    ),
    simple_entry(
        "minecraft:cave_spider",
        "minecraft:cave_spider#main",
        None,
        "entity/spider/cave_spider",
        None,
        Rig::Spider,
    ),
    simple_entry(
        "minecraft:cod",
        "minecraft:cod#main",
        None,
        "entity/fish/cod",
        None,
        Rig::Fish,
    ),
    simple_entry(
        "minecraft:dolphin",
        "minecraft:dolphin#main",
        Some("minecraft:dolphin_baby#main"),
        "entity/dolphin/dolphin",
        Some("entity/dolphin/dolphin_baby"),
        Rig::Look,
    ),
    simple_entry(
        "minecraft:elder_guardian",
        "minecraft:elder_guardian#main",
        None,
        "entity/guardian/guardian_elder",
        None,
        Rig::Look,
    ),
    simple_entry(
        "minecraft:endermite",
        "minecraft:endermite#main",
        None,
        "entity/endermite/endermite",
        None,
        Rig::Endermite,
    ),
    simple_entry(
        "minecraft:glow_squid",
        "minecraft:glow_squid#main",
        Some("minecraft:glow_squid_baby#main"),
        "entity/squid/glow_squid",
        Some("entity/squid/glow_squid_baby"),
        Rig::Squid,
    ),
    simple_entry(
        "minecraft:goat",
        "minecraft:goat#main",
        Some("minecraft:goat_baby#main"),
        "entity/goat/goat",
        Some("entity/goat/goat_baby"),
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:guardian",
        "minecraft:guardian#main",
        None,
        "entity/guardian/guardian",
        None,
        Rig::Look,
    ),
    simple_entry(
        "minecraft:hoglin",
        "minecraft:hoglin#main",
        Some("minecraft:hoglin_baby#main"),
        "entity/hoglin/hoglin",
        Some("entity/hoglin/hoglin_baby"),
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:zoglin",
        "minecraft:zoglin#main",
        Some("minecraft:zoglin_baby#main"),
        "entity/hoglin/zoglin",
        Some("entity/hoglin/zoglin_baby"),
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:iron_golem",
        "minecraft:iron_golem#main",
        None,
        "entity/iron_golem/iron_golem",
        None,
        Rig::IronGolem,
    ),
    simple_entry(
        "minecraft:magma_cube",
        "minecraft:magma_cube#main",
        None,
        "entity/slime/magmacube",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:ocelot",
        "minecraft:ocelot#main",
        Some("minecraft:ocelot_baby#main"),
        "entity/cat/ocelot",
        Some("entity/cat/ocelot_baby"),
        Rig::Feline,
    ),
    simple_entry(
        "minecraft:phantom",
        "minecraft:phantom#main",
        None,
        "entity/phantom/phantom",
        None,
        Rig::Phantom,
    ),
    simple_entry(
        "minecraft:polar_bear",
        "minecraft:polar_bear#main",
        Some("minecraft:polar_bear_baby#main"),
        "entity/bear/polarbear",
        Some("entity/bear/polarbear_baby"),
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:ravager",
        "minecraft:ravager#main",
        None,
        "entity/illager/ravager",
        None,
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:silverfish",
        "minecraft:silverfish#main",
        None,
        "entity/silverfish/silverfish",
        None,
        Rig::Endermite,
    ),
    simple_entry(
        "minecraft:snow_golem",
        "minecraft:snow_golem#main",
        None,
        "entity/snow_golem/snow_golem",
        None,
        Rig::SnowGolem,
    ),
    simple_entry(
        "minecraft:spider",
        "minecraft:spider#main",
        None,
        "entity/spider/spider",
        None,
        Rig::Spider,
    ),
    simple_entry(
        "minecraft:squid",
        "minecraft:squid#main",
        Some("minecraft:squid_baby#main"),
        "entity/squid/squid",
        Some("entity/squid/squid_baby"),
        Rig::Squid,
    ),
    simple_entry(
        "minecraft:tadpole",
        "minecraft:tadpole#main",
        None,
        "entity/tadpole/tadpole",
        None,
        Rig::Fish,
    ),
    simple_entry(
        "minecraft:turtle",
        "minecraft:turtle#main",
        Some("minecraft:turtle_baby#main"),
        "entity/turtle/turtle",
        Some("entity/turtle/turtle_baby"),
        Rig::Quadruped,
    ),
    simple_entry(
        "minecraft:vex",
        "minecraft:vex#main",
        None,
        "entity/illager/vex",
        None,
        Rig::Vex,
    ),
    simple_entry(
        "minecraft:strider",
        "minecraft:strider#main",
        Some("minecraft:strider_baby#main"),
        "entity/strider/strider",
        Some("entity/strider/strider_baby"),
        Rig::Strider,
    ),
    simple_entry(
        "minecraft:sniffer",
        "minecraft:sniffer#main",
        Some("minecraft:sniffer_baby#main"),
        "entity/sniffer/sniffer",
        Some("entity/sniffer/snifflet"),
        Rig::Look,
    ),
    simple_entry(
        "minecraft:warden",
        "minecraft:warden#main",
        None,
        "entity/warden/warden",
        None,
        Rig::Look,
    ),
    simple_entry(
        "minecraft:nautilus",
        "minecraft:nautilus#main",
        Some("minecraft:nautilus_baby#main"),
        "entity/nautilus/nautilus",
        Some("entity/nautilus/nautilus_baby"),
        Rig::Static,
    ),
    simple_entry(
        "minecraft:end_crystal",
        "minecraft:end_crystal#main",
        None,
        "entity/end_crystal/end_crystal",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:leash_knot",
        "minecraft:leash_knot#main",
        None,
        "entity/lead_knot/lead_knot",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:evoker_fangs",
        "minecraft:evoker_fangs#main",
        None,
        "entity/illager/evoker_fangs",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:llama_spit",
        "minecraft:llama_spit#main",
        None,
        "entity/llama/llama_spit",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:shulker_bullet",
        "minecraft:shulker_bullet#main",
        None,
        "entity/shulker/spark",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:wind_charge",
        "minecraft:wind_charge#main",
        None,
        "entity/projectiles/wind_charge",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:breeze_wind_charge",
        "minecraft:wind_charge#main",
        None,
        "entity/projectiles/wind_charge",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:trident",
        "minecraft:trident#main",
        None,
        "entity/trident/trident",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:wither_skull",
        "minecraft:wither_skull#main",
        None,
        "entity/wither/wither",
        None,
        Rig::Static,
    ),
    simple_entry(
        "minecraft:copper_golem",
        "minecraft:copper_golem#main",
        None,
        "entity/copper_golem/copper_golem",
        None,
        Rig::Look,
    ),
];

pub(crate) fn builder_for(name: &str) -> Builder {
    if let Some((_, spec)) = SIMPLE.iter().find(|(n, _)| *n == name) {
        return Builder::Simple(spec);
    }
    if let Some(f) = humanoids::builder(name) {
        return Builder::Func(f);
    }
    if let Some(f) = misc::builder(name) {
        return Builder::Func(f);
    }
    match name {
        "minecraft:area_effect_cloud"
        | "minecraft:interaction"
        | "minecraft:marker"
        | "minecraft:lightning_bolt"
        | "minecraft:text_display"
        | "minecraft:ominous_item_spawner"
        | "minecraft:experience_orb"
        | "minecraft:fishing_bobber"
        | "minecraft:firework_rocket" => Builder::Skip,
        "minecraft:pig" => Builder::Func(pig),
        "minecraft:cow" => Builder::Func(cow),
        "minecraft:mooshroom" => Builder::Func(mooshroom),
        "minecraft:chicken" => Builder::Func(chicken),
        "minecraft:sheep" => Builder::Func(sheep),
        "minecraft:wolf" => Builder::Func(wolf),
        "minecraft:cat" => Builder::Func(cat),
        "minecraft:creeper" => Builder::Func(creeper),
        "minecraft:fox" => Builder::Func(fox),
        "minecraft:rabbit" => Builder::Func(rabbit),
        "minecraft:parrot" => Builder::Func(parrot),
        "minecraft:axolotl" => Builder::Func(axolotl),
        "minecraft:frog" => Builder::Func(frog),
        "minecraft:panda" => Builder::Func(panda),
        "minecraft:horse" | "minecraft:skeleton_horse" | "minecraft:zombie_horse" => {
            Builder::Func(horse)
        }
        "minecraft:donkey" | "minecraft:mule" => Builder::Func(donkey),
        "minecraft:llama" | "minecraft:trader_llama" => Builder::Func(llama),
        "minecraft:camel" | "minecraft:camel_husk" => Builder::Func(camel),
        "minecraft:bee" => Builder::Func(bee),
        "minecraft:salmon" => Builder::Func(salmon),
        "minecraft:pufferfish" => Builder::Func(pufferfish),
        "minecraft:tropical_fish" => Builder::Func(tropical_fish),
        "minecraft:slime" => Builder::Func(slime),
        "minecraft:ghast" => Builder::Func(ghast),
        "minecraft:happy_ghast" => Builder::Func(happy_ghast),
        "minecraft:shulker" => Builder::Func(shulker),
        "minecraft:wither" => Builder::Func(wither),
        "minecraft:breeze" => Builder::Func(breeze),
        "minecraft:creaking" => Builder::Func(creaking),
        "minecraft:ender_dragon" => Builder::Func(dragon),
        "minecraft:zombie_nautilus" => Builder::Func(zombie_nautilus),
        "minecraft:sulfur_cube" => Builder::Func(sulfur_cube),
        "minecraft:cushion" => Builder::Skip,
        _ => Builder::Hitbox,
    }
}

fn baby_of(plan: &mut Plan, v: &EntityView<'_>) -> bool {
    let baby = is_baby(v);
    plan.anim.baby = baby;
    baby
}

fn pig(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let name = variant_name(cx, v, idx::PIG_VARIANT, "minecraft:pig_variant");
    let variant = pick(&name, &["cold", "temperate", "warm"], "temperate");
    let layer = match (baby, variant) {
        (true, _) => "minecraft:pig_baby#main",
        (_, "cold") => "minecraft:cold_pig#main",
        _ => "minecraft:pig#main",
    };
    let tex = if baby {
        format!("entity/pig/pig_{variant}_baby")
    } else {
        format!("entity/pig/pig_{variant}")
    };
    plan.push(Part::new(layer, tex));
    if let Some(s) = v.item(slot::SADDLE) {
        add_layers(cx, plan, s, "saddle", "pig_saddle", "minecraft:pig#saddle");
    }
}

fn cow(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let name = variant_name(cx, v, idx::COW_VARIANT, "minecraft:cow_variant");
    let variant = pick(&name, &["cold", "temperate", "warm"], "temperate");
    let prefix = match variant {
        "cold" => "cold_",
        "warm" => "warm_",
        _ => "",
    };
    let layer = format!(
        "minecraft:{prefix}cow{}#main",
        if baby { "_baby" } else { "" }
    );
    let tex = format!(
        "entity/cow/cow_{variant}{}",
        if baby { "_baby" } else { "" }
    );
    plan.push(Part::new(layer, tex));
}

fn mooshroom(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let brown = v.int(idx::MOOSHROOM_TYPE) == Some(1);
    let colour = if brown { "brown" } else { "red" };
    let b = if baby { "_baby" } else { "" };
    plan.push(Part::new(
        format!("minecraft:mooshroom{b}#main"),
        format!("entity/cow/mooshroom_{colour}{b}"),
    ));
}

fn chicken(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Chicken;
    let baby = baby_of(plan, v);
    let name = variant_name(cx, v, idx::CHICKEN_VARIANT, "minecraft:chicken_variant");
    let variant = pick(&name, &["cold", "temperate", "warm"], "temperate");
    let layer = if baby {
        "minecraft:chicken_baby#main"
    } else if variant == "cold" {
        "minecraft:cold_chicken#main"
    } else {
        "minecraft:chicken#main"
    };
    let tex = format!(
        "entity/chicken/chicken_{variant}{}",
        if baby { "_baby" } else { "" }
    );
    plan.push(Part::new(layer, tex));
}

fn sheep(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let b = if baby { "_baby" } else { "" };
    plan.push(Part::new(
        format!("minecraft:sheep{b}#main"),
        format!("entity/sheep/sheep{b}"),
    ));
    let raw = v.byte(idx::SHEEP_WOOL).unwrap_or(0) as u8;
    let sheared = raw & 0x10 != 0;
    let colour = i32::from(raw & 15);
    if !sheared {
        let rgb = dye_rgb(colour);
        if !baby && colour != 0 {
            plan.push(
                Part::new(
                    "minecraft:sheep#wool_undercoat",
                    "entity/sheep/sheep_wool_undercoat",
                )
                .tint(rgb),
            );
        }
        plan.push(
            Part::new(
                format!("minecraft:sheep{b}#wool"),
                format!("entity/sheep/sheep_wool{b}"),
            )
            .tint(rgb),
        );
    }
}

fn tamable(v: &EntityView<'_>) -> (bool, bool) {
    let flags = v.byte(idx::TAMABLE_FLAGS).unwrap_or(0) as u8;
    (flags & 1 != 0, flags & 4 != 0)
}

fn wolf(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Wolf;
    let baby = baby_of(plan, v);
    let (sitting, tame) = tamable(v);
    plan.anim.sitting = sitting;
    let name = variant_name(cx, v, idx::WOLF_VARIANT, "minecraft:wolf_variant");
    let known = [
        "ashen", "black", "chestnut", "pale", "rusty", "snowy", "spotted", "striped", "woods",
    ];
    let variant = pick(&name, &known, "pale");
    let angry = matches!(v.raw(idx::WOLF_ANGER), Some(EntityDataValue::Long(t)) if *t > 0);
    let state = if tame {
        "_tame"
    } else if angry {
        "_angry"
    } else {
        ""
    };
    let b = if baby { "_baby" } else { "" };
    let stem = if variant == "pale" {
        "wolf".to_owned()
    } else {
        format!("wolf_{variant}")
    };
    let layer = if baby {
        "minecraft:wolf_baby#main"
    } else {
        "minecraft:wolf#main"
    };
    plan.push(Part::new(layer, format!("entity/wolf/{stem}{state}{b}")));
    if tame && let Some(DyeValue(c)) = dye_int(v, idx::WOLF_COLLAR) {
        plan.push(
            Part::new(layer, format!("entity/wolf/wolf_collar{b}"))
                .tint(dye_rgb(c))
                .overlay()
                .no_hurt(),
        );
    }
    if !baby && let Some(s) = v.item(slot::BODY) {
        add_layers(
            cx,
            plan,
            s,
            "body",
            "wolf_body",
            "minecraft:wolf_armor#main",
        );
    }
}

pub struct DyeValue(pub i32);

fn dye_int(v: &EntityView<'_>, index: u8) -> Option<DyeValue> {
    v.int(index).map(DyeValue)
}

fn cat(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Feline;
    let baby = baby_of(plan, v);
    let (sitting, tame) = tamable(v);
    plan.anim.sitting = sitting;
    if v.boolean(idx::CAT_LYING) {
        plan.anim.lie_down = 1.0;
    }
    let name = variant_name(cx, v, idx::CAT_VARIANT, "minecraft:cat_variant");
    let known = [
        "all_black",
        "black",
        "british_shorthair",
        "calico",
        "jellie",
        "persian",
        "ragdoll",
        "red",
        "siamese",
        "tabby",
        "white",
    ];
    let variant = pick(&name, &known, "tabby");
    let b = if baby { "_baby" } else { "" };
    let layer = format!("minecraft:cat{b}#main");
    plan.push(Part::new(layer, format!("entity/cat/cat_{variant}{b}")));
    if tame && let Some(DyeValue(c)) = dye_int(v, idx::CAT_COLLAR) {
        plan.push(
            Part::new(
                format!("minecraft:cat{b}#collar"),
                format!("entity/cat/cat_collar{b}"),
            )
            .tint(dye_rgb(c))
            .overlay()
            .no_hurt(),
        );
    }
}

fn creeper(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Creeper;
    plan.push(Part::new(
        "minecraft:creeper#main",
        "entity/creeper/creeper",
    ));
    let swell = v.int(idx::CREEPER_SWELL).unwrap_or(-1).max(0) as f32 / 28.0;
    if swell > 0.0 {
        let wobble = 1.0 + (swell * 100.0).sin() * swell * 0.01;
        let g = swell.clamp(0.0, 1.0).powi(4);
        plan.extra_scale = Vec3::new(
            (1.0 + g * 0.4) * wobble,
            (1.0 + g * 0.1) / wobble,
            (1.0 + g * 0.4) * wobble,
        );
    }
    if matches!(
        v.raw(idx::CREEPER_POWERED),
        Some(EntityDataValue::Boolean(true))
    ) {
        let mut p = Part::new("minecraft:creeper#armor", "entity/creeper/creeper_armor")
            .pipeline(Pipeline::Additive)
            .fullbright()
            .no_hurt();
        let t = v.age_ticks * 0.01;
        p.uv_offset = [t, t];
        p.tint = [128, 128, 128, 255];
        plan.push(p);
    }
}

fn fox(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Fox;
    let baby = baby_of(plan, v);
    let snow = v.int(idx::FOX_TYPE) == Some(1);
    let sleeping = v.byte(idx::FOX_FLAGS).unwrap_or(0) & 32 != 0;
    let b = if baby { "_baby" } else { "" };
    let tex = format!(
        "entity/fox/fox{}{}{b}",
        if snow { "_snow" } else { "" },
        if sleeping { "_sleep" } else { "" }
    );
    plan.push(Part::new(format!("minecraft:fox{b}#main"), tex));
}

fn rabbit(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Rabbit;
    let baby = baby_of(plan, v);
    let name = match v.int(idx::RABBIT_TYPE).unwrap_or(0) {
        1 => "white",
        2 => "black",
        3 => "white_splotched",
        4 => "gold",
        5 => "salt",
        99 => "caerbannog",
        _ => "brown",
    };
    let b = if baby { "_baby" } else { "" };
    plan.push(Part::new(
        format!("minecraft:rabbit{b}#main"),
        format!("entity/rabbit/rabbit_{name}{b}"),
    ));
}

fn parrot(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Parrot;
    let name = match v.int(idx::PARROT_VARIANT).unwrap_or(0) {
        1 => "blue",
        2 => "green",
        3 => "yellow_blue",
        4 => "grey",
        _ => "red_blue",
    };
    plan.push(Part::new(
        "minecraft:parrot#main",
        format!("entity/parrot/parrot_{name}"),
    ));
}

fn axolotl(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Look;
    let baby = baby_of(plan, v);
    let name = match v.int(idx::AXOLOTL_VARIANT).unwrap_or(0) {
        1 => "wild",
        2 => "gold",
        3 => "cyan",
        4 => "blue",
        _ => "lucy",
    };
    let b = if baby { "_baby" } else { "" };
    plan.push(Part::new(
        format!("minecraft:axolotl{b}#main"),
        format!("entity/axolotl/axolotl_{name}{b}"),
    ));
}

fn frog(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Look;
    let name = variant_name(cx, v, idx::FROG_VARIANT, "minecraft:frog_variant");
    let variant = pick(&name, &["cold", "temperate", "warm"], "temperate");
    plan.push(Part::new(
        "minecraft:frog#main",
        format!("entity/frog/frog_{variant}"),
    ));
}

fn panda(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let gene = |i: u8| v.byte(i).unwrap_or(0);
    let (main, hidden) = (gene(idx::PANDA_MAIN_GENE), gene(idx::PANDA_HIDDEN_GENE));
    let name = |g: i8| match g {
        1 => "lazy",
        2 => "worried",
        3 => "playful",
        4 => "brown",
        5 => "weak",
        6 => "aggressive",
        _ => "normal",
    };
    let effective = if main <= 3 || main == hidden { main } else { 0 };
    let n = name(effective);
    let b = if baby { "_baby" } else { "" };
    let tex = match (n, baby) {
        ("normal", false) => "entity/panda/panda".to_owned(),
        ("normal", true) => "entity/panda/panda_baby".to_owned(),
        (n, false) => format!("entity/panda/panda_{n}"),
        (n, true) => format!("entity/panda/{n}_panda_baby"),
    };
    plan.push(Part::new(format!("minecraft:panda{b}#main"), tex));
}

fn equine_common(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan, kind: &str, baby: bool) {
    let saddle_type = format!("{kind}_saddle");
    if let Some(s) = v.item(slot::SADDLE) {
        let model = format!("minecraft:{kind}#saddle");
        let m = if baby { None } else { Some(model) };
        if let Some(m) = m {
            add_layers(cx, plan, s, "saddle", &saddle_type, &m);
        }
    }
}

fn horse(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let b = if baby { "_baby" } else { "" };
    let name = cx.type_name(v).unwrap_or("minecraft:horse").to_owned();
    match short(&name) {
        "skeleton_horse" | "zombie_horse" => {
            let kind = short(&name).to_owned();
            let tex = if kind == "skeleton_horse" {
                "horse_skeleton"
            } else {
                "horse_zombie"
            };
            plan.push(Part::new(
                format!("minecraft:{kind}{b}#main"),
                format!("entity/horse/{tex}{b}"),
            ));
            equine_common(cx, v, plan, &kind, baby);
        }
        _ => {
            let variant = v.int(idx::HORSE_VARIANT).unwrap_or(0);
            let colour = [
                "white",
                "creamy",
                "chestnut",
                "brown",
                "black",
                "gray",
                "darkbrown",
            ][(variant & 255).rem_euclid(7) as usize];
            let layer = format!("minecraft:horse{b}#main");
            plan.push(Part::new(
                layer.clone(),
                format!("entity/horse/horse_{colour}{b}"),
            ));
            let marking = match (variant >> 8) & 255 {
                1 => Some("white"),
                2 => Some("whitefield"),
                3 => Some("whitedots"),
                4 => Some("blackdots"),
                _ => None,
            };
            if let Some(m) = marking {
                plan.push(
                    Part::new(layer, format!("entity/horse/horse_markings_{m}{b}"))
                        .overlay()
                        .no_hurt(),
                );
            }
            equine_common(cx, v, plan, "horse", baby);
            if !baby && let Some(a) = v.item(slot::BODY) {
                add_layers(
                    cx,
                    plan,
                    a,
                    "body",
                    "horse_body",
                    "minecraft:horse_armor#main",
                );
            }
        }
    }
}

fn donkey(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let mule = cx.type_name(v) == Some("minecraft:mule");
    let kind = if mule { "mule" } else { "donkey" };
    let b = if baby { "_baby" } else { "" };
    plan.push(Part::new(
        format!("minecraft:{kind}{b}#main"),
        format!("entity/horse/{kind}{b}"),
    ));
    if !v.boolean(19) {
        plan.hide.push(crate::rig::Slot::LeftChest);
        plan.hide.push(crate::rig::Slot::RightChest);
    }
    equine_common(cx, v, plan, kind, baby);
}

fn llama(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let trader = cx.type_name(v) == Some("minecraft:trader_llama");
    let name = ["creamy", "white", "brown", "gray"]
        [v.int(idx::LLAMA_VARIANT).unwrap_or(0).rem_euclid(4) as usize];
    let b = if baby { "_baby" } else { "" };
    let kind = if trader { "trader_llama" } else { "llama" };
    plan.push(Part::new(
        format!("minecraft:{kind}{b}#main"),
        format!("entity/llama/llama_{name}{b}"),
    ));
    if !v.boolean(19) {
        plan.hide.push(crate::rig::Slot::LeftChest);
        plan.hide.push(crate::rig::Slot::RightChest);
    }
    if let Some(s) = v.item(slot::BODY) {
        let model = format!("minecraft:llama{b}#decor");
        add_layers(cx, plan, s, "body", "llama_body", &model);
    }
}

fn camel(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Quadruped;
    let baby = baby_of(plan, v);
    let husk = cx.type_name(v) == Some("minecraft:camel_husk");
    let tex = if husk {
        "entity/camel/camel_husk"
    } else if baby {
        "entity/camel/camel_baby"
    } else {
        "entity/camel/camel"
    };
    let layer = if baby && !husk {
        "minecraft:camel_baby#main"
    } else {
        "minecraft:camel#main"
    };
    plan.push(Part::new(layer, tex));
    if let Some(s) = v.item(slot::SADDLE) {
        let (m, t) = if husk {
            ("minecraft:camel_husk#saddle", "camel_husk_saddle")
        } else {
            ("minecraft:camel#saddle", "camel_saddle")
        };
        add_layers(cx, plan, s, "saddle", t, m);
    }
}

fn bee(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Bee;
    let baby = baby_of(plan, v);
    let nectar = v.byte(idx::BEE_FLAGS).unwrap_or(0) & 8 != 0;
    let angry = matches!(v.raw(19), Some(EntityDataValue::Long(t)) if *t > 0);
    let b = if baby { "_baby" } else { "" };
    let tex = format!(
        "entity/bee/bee{}{}{b}",
        if angry { "_angry" } else { "" },
        if nectar { "_nectar" } else { "" }
    );
    plan.push(Part::new(format!("minecraft:bee{b}#main"), tex));
}

fn salmon(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Fish;
    let layer = match v.int(idx::SALMON_TYPE).unwrap_or(1) {
        0 => "minecraft:salmon_small#main",
        2 => "minecraft:salmon_large#main",
        _ => "minecraft:salmon#main",
    };
    plan.push(Part::new(layer, "entity/fish/salmon"));
}

fn pufferfish(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Static;
    let layer = match v.int(idx::PUFFER_STATE).unwrap_or(0) {
        1 => "minecraft:pufferfish_medium#main",
        2 => "minecraft:pufferfish_big#main",
        _ => "minecraft:pufferfish_small#main",
    };
    plan.push(Part::new(layer, "entity/fish/pufferfish"));
}

fn tropical_fish(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Fish;
    let packed = v.int(idx::TROPICAL_VARIANT).unwrap_or(0);
    let large = (packed & 0xFFFF) >= 256;
    let pattern = (packed & 0xFF) % 6 + 1;
    let base = dye_rgb((packed >> 16) & 0xFF);
    let pat = dye_rgb((packed >> 24) & 0xFF);
    let (shape, letter) = if large {
        ("large", "b")
    } else {
        ("small", "a")
    };
    let layer = format!("minecraft:tropical_fish_{shape}#main");
    plan.push(Part::new(layer, format!("entity/fish/tropical_{letter}")).tint(base));
    plan.push(
        Part::new(
            format!("minecraft:tropical_fish_{shape}#pattern"),
            format!("entity/fish/tropical_{letter}_pattern_{pattern}"),
        )
        .tint(pat)
        .overlay(),
    );
}

fn slime(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Static;
    let size = v.int(idx::SLIME_SIZE).unwrap_or(1).clamp(1, 127) as f32;
    plan.extra_scale = Vec3::splat(size * 0.999);
    plan.extra_offset = Vec3::ZERO;
    plan.push(Part::new("minecraft:slime#main", "entity/slime/slime"));
    plan.push(
        Part::new("minecraft:slime#outer", "entity/slime/slime")
            .pipeline(Pipeline::Translucent)
            .no_hurt(),
    );
}

fn ghast(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Ghast;
    let tex = if v.boolean(idx::GHAST_CHARGING) {
        "entity/ghast/ghast_shooting"
    } else {
        "entity/ghast/ghast"
    };
    plan.extra_scale = Vec3::splat(4.5);
    plan.push(Part::new("minecraft:ghast#main", tex));
}

fn happy_ghast(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Ghast;
    let baby = baby_of(plan, v);
    let b = if baby { "_baby" } else { "" };
    plan.push(Part::new(
        format!("minecraft:happy_ghast{b}#main"),
        format!("entity/ghast/happy_ghast{b}"),
    ));
    if let Some(s) = v.item(slot::BODY) {
        let model = format!("minecraft:happy_ghast{b}_harness#main");
        add_layers(cx, plan, s, "body", "happy_ghast_body", &model);
    }
}

fn shulker(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Static;
    plan.push(Part::new("minecraft:shulker#main", {
        let c = v.byte(idx::SHULKER_COLOR).unwrap_or(16);
        if (0..16).contains(&c) {
            format!("entity/shulker/shulker_{}", COLOR_NAMES[c as usize])
        } else {
            "entity/shulker/shulker".to_owned()
        }
    }));
}

pub const COLOR_NAMES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

fn wither(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Look;
    plan.extra_scale = Vec3::splat(2.0);
    let inv = v.int(idx::WITHER_INVULNERABLE).unwrap_or(0);
    let tex = if inv > 0 {
        "entity/wither/wither_invulnerable"
    } else {
        "entity/wither/wither"
    };
    plan.push(Part::new("minecraft:wither#main", tex));
}

fn breeze(_cx: &mut PlanCx<'_>, _v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Look;
    plan.push(Part::new("minecraft:breeze#main", "entity/breeze/breeze"));
    plan.push(Part::new("minecraft:breeze#eyes", "entity/breeze/breeze_eyes").eyes());
}

fn creaking(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Look;
    plan.push(Part::new(
        "minecraft:creaking#main",
        "entity/creaking/creaking",
    ));
    if v.boolean(idx::CREAKING_ACTIVE) {
        plan.push(Part::new("minecraft:creaking#eyes", "entity/creaking/creaking_eyes").eyes());
    }
}

fn dragon(_cx: &mut PlanCx<'_>, _v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Static;
    plan.push(Part::new(
        "minecraft:ender_dragon#main",
        "entity/enderdragon/dragon",
    ));
    plan.push(
        Part::new(
            "minecraft:ender_dragon#main",
            "entity/enderdragon/dragon_eyes",
        )
        .eyes(),
    );
}

fn zombie_nautilus(cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Static;
    baby_of(plan, v);
    let name = variant_name(
        cx,
        v,
        idx::ZOMBIE_NAUTILUS_VARIANT,
        "minecraft:zombie_nautilus_variant",
    );
    let coral = name.as_deref() == Some("warm");
    let (layer, tex) = if coral {
        (
            "minecraft:zombie_nautilus_coral#main",
            "entity/nautilus/zombie_nautilus_coral",
        )
    } else {
        (
            "minecraft:zombie_nautilus#main",
            "entity/nautilus/zombie_nautilus",
        )
    };
    plan.push(Part::new(layer, tex));
}

fn sulfur_cube(_cx: &mut PlanCx<'_>, v: &EntityView<'_>, plan: &mut Plan) {
    plan.rig = Rig::Static;
    let small = v.int(idx::SLIME_SIZE).unwrap_or(1) <= 1;
    let (m, i, t) = if small {
        (
            "minecraft:sulfur_cube_small",
            "sulfur_cube_inner_small",
            "sulfur_cube_outer_small",
        )
    } else {
        (
            "minecraft:sulfur_cube",
            "sulfur_cube_inner",
            "sulfur_cube_outer",
        )
    };
    plan.push(
        Part::new(format!("{m}#main"), format!("entity/sulfur_cube/{t}"))
            .pipeline(Pipeline::Translucent),
    );
    plan.push(Part::new(
        format!("{m}#inner"),
        format!("entity/sulfur_cube/{i}"),
    ));
}
