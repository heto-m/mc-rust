use mcr_buf::IdOr;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, EquipmentSlot, component_type_id};

use crate::assets::ImageRgba;
use crate::catalog::PlanCx;
use crate::gpu::Pipeline;
use crate::plan::{Part, Plan};
use crate::view::{EntityView, slot};

pub fn component<'a>(stack: &'a ItemStack, name: &str) -> Option<&'a DataComponent> {
    stack.components.get(component_type_id(name)?)
}

pub fn dyed_color(stack: &ItemStack) -> Option<i32> {
    match component(stack, "dyed_color") {
        Some(DataComponent::DyedColor(c)) => Some(*c),
        _ => None,
    }
}

/// Whether vanilla draws the enchantment glint over this stack.
pub fn has_glint(stack: &ItemStack) -> bool {
    if stack.components.is_empty() {
        return false;
    }
    match component(stack, "enchantment_glint_override") {
        Some(DataComponent::EnchantmentGlintOverride(v)) => *v,
        _ => matches!(
            component(stack, "enchantments"),
            Some(DataComponent::Enchantments(e)) if !e.0.is_empty()
        ),
    }
}

fn slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::MainHand => "mainhand",
        EquipmentSlot::OffHand => "offhand",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    }
}

/// The equipment asset a stack wears and the slot it belongs in: the stack's own component first,
/// then the item's defaults.
pub fn equippable(cx: &PlanCx<'_>, stack: &ItemStack) -> Option<(String, &'static str)> {
    if let Some(DataComponent::Equippable(e)) = component(stack, "equippable") {
        let asset = e.asset_id.as_ref()?;
        return Some((asset.to_string(), slot_name(e.slot)));
    }
    let name = mcr_data::v26_3().item_name(u32::try_from(stack.item).ok()?)?;
    let def = cx.assets.equippables.get(name)?;
    let slot = match def.slot.as_deref()? {
        "head" => "head",
        "chest" => "chest",
        "legs" => "legs",
        "feet" => "feet",
        "body" => "body",
        "saddle" => "saddle",
        "mainhand" => "mainhand",
        _ => "offhand",
    };
    Some((def.asset_id.clone()?, slot))
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn argb_rgb(c: i32) -> u32 {
    c as u32 & 0x00FF_FFFF
}

/// Adds the equipment layers of `stack` for `layer_type` drawn on `model`; false if nothing was added.
pub fn add_layers(
    cx: &mut PlanCx<'_>,
    plan: &mut Plan,
    stack: &ItemStack,
    wanted_slot: &str,
    layer_type: &str,
    model: &str,
) -> bool {
    let Some((asset, slot)) = equippable(cx, stack) else {
        return false;
    };
    if slot != wanted_slot {
        return false;
    }
    let Some(info) = cx.assets.equipment(&asset) else {
        return false;
    };
    let Some(layers) = info.layers.get(layer_type) else {
        return false;
    };
    let dye = dyed_color(stack).unwrap_or(0);
    let before = plan.parts.len();
    for layer in layers {
        let color = if layer.dyeable {
            if dye != 0 {
                Some(dye)
            } else {
                layer.undyed_color
            }
        } else {
            Some(-1)
        };
        let Some(color) = color else { continue };
        let tex = format!("entity/equipment/{layer_type}/{}", short(&layer.texture));
        let mut part = Part::new(model.to_owned(), tex);
        if layer.dyeable {
            part = part.tint(argb_rgb(color));
        }
        part.hurt = true;
        plan.push(part);
    }
    if plan.parts.len() == before {
        return false;
    }
    if has_glint(stack) {
        let mut glint = Part::new(model.to_owned(), "misc/enchanted_glint_armor")
            .pipeline(Pipeline::Additive)
            .no_hurt();
        glint.tint = [160, 100, 255, 150];
        glint.uv_offset = [
            (plan.anim.age * 0.01).fract(),
            (plan.anim.age * 0.007).fract(),
        ];
        plan.push(glint);
    }
    if matches!(layer_type, "humanoid" | "humanoid_leggings")
        && let Some(DataComponent::Trim(trim)) = component(stack, "trim")
        && let Some(key) = trim_texture(cx, &info, layer_type, trim)
    {
        let part = Part::new(model.to_owned(), key).pipeline(Pipeline::Decal);
        plan.push(part);
    }
    true
}

fn trim_texture(
    cx: &mut PlanCx<'_>,
    info: &crate::assets::EquipmentInfo,
    layer_type: &str,
    trim: &mcr_protocol::types::component::ArmorTrim,
) -> Option<String> {
    let (pattern, decal_unused) = match &trim.pattern {
        IdOr::Id(id) => (
            short(cx.reg.entry_name("minecraft:trim_pattern", *id)?).to_owned(),
            false,
        ),
        IdOr::Inline(p) => (short(p.asset_id.as_str()).to_owned(), p.decal),
    };
    let _ = decal_unused;
    let (material_name, mut palette) = match &trim.material {
        IdOr::Id(id) => {
            let name = cx.reg.entry_name("minecraft:trim_material", *id)?;
            (name.to_owned(), format!("trim/{}", short(name)))
        }
        IdOr::Inline(m) => (String::new(), short(m.palette_id.as_str()).to_owned()),
    };
    let mut pattern_key = pattern;
    for o in &info.trim_overrides {
        let matches = match (&o.material, material_name.is_empty()) {
            (Some(m), false) => short(m) == short(&material_name),
            (None, _) => true,
            _ => false,
        };
        if matches {
            if let Some(t) = &o.texture {
                pattern_key = short(t).to_owned();
            }
            if let Some(p) = &o.palette {
                palette = short(p).to_owned();
            }
            break;
        }
    }
    let key = format!("dyn:trim:{layer_type}:{pattern_key}:{palette}");
    if !cx.tex.is_registered(&key) {
        let image = compose_trim(
            cx,
            &format!("trims/entity/{layer_type}/{pattern_key}"),
            &palette,
        )?;
        cx.tex.register(&key, || image);
    }
    Some(key)
}

fn compose_trim(cx: &PlanCx<'_>, pattern: &str, palette: &str) -> Option<ImageRgba> {
    let mut image = cx.assets.read_texture(pattern)?;
    let base = cx.assets.read_texture("palettes/trim_base")?;
    let target = cx.assets.read_texture(&format!("palettes/{palette}"))?;
    let n = (base.width * base.height).min(target.width * target.height) as usize;
    let key =
        |img: &ImageRgba, i: usize| [img.rgba[i * 4], img.rgba[i * 4 + 1], img.rgba[i * 4 + 2]];
    for px in image.rgba.as_chunks_mut::<4>().0 {
        if px[3] == 0 {
            continue;
        }
        if let Some(i) = (0..n).find(|&i| key(&base, i) == [px[0], px[1], px[2]]) {
            let t = key(&target, i);
            px[..3].copy_from_slice(&t);
        }
    }
    Some(image)
}

/// Humanoid armor (head, chest, legs, feet) on a model family whose armor layers are
/// `<prefix>#helmet|chestplate|leggings|boots`.
pub fn humanoid_armor(
    cx: &mut PlanCx<'_>,
    v: &EntityView<'_>,
    plan: &mut Plan,
    prefix: &str,
    baby: bool,
) {
    let table = [
        (slot::CHEST, "chest", "chestplate"),
        (slot::LEGS, "legs", "leggings"),
        (slot::FEET, "feet", "boots"),
        (slot::HEAD, "head", "helmet"),
    ];
    for (index, slot_name, piece) in table {
        let Some(stack) = v.item(index) else { continue };
        let layer_type = if baby {
            "humanoid_baby"
        } else if slot_name == "legs" {
            "humanoid_leggings"
        } else {
            "humanoid"
        };
        let model = format!("minecraft:{prefix}#{piece}");
        add_layers(cx, plan, stack, slot_name, layer_type, &model);
    }
}
