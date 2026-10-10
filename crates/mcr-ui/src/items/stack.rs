use mcr_assets::{ChargeType, ItemProps};
use mcr_data::{ItemId, v26_3};
use mcr_protocol::types::component::DataComponent;
use mcr_protocol::types::item::ItemStack;

use super::icons::ItemView;

const GLINT_BY_DEFAULT: &[&str] = &[
    "minecraft:enchanted_golden_apple",
    "minecraft:experience_bottle",
    "minecraft:nether_star",
    "minecraft:end_crystal",
    "minecraft:debug_stick",
    "minecraft:enchanted_book",
];

/// A protocol stack seen through [`ItemProps`]; empty stacks and unknown item ids yield `None`.
#[derive(Debug)]
pub struct StackView<'a> {
    stack: &'a ItemStack,
    name: &'a str,
    max_damage: i32,
    max_stack: i32,
    trim_materials: &'a [String],
}

impl<'a> StackView<'a> {
    pub fn new(stack: &'a ItemStack) -> Option<Self> {
        if stack.is_empty() {
            return None;
        }
        let version = v26_3();
        let id = ItemId::try_from(stack.item).ok()?;
        let info = version.item_info(id)?;
        Some(Self {
            stack,
            name: version.item_name(id)?,
            max_damage: info.max_damage as i32,
            max_stack: i32::from(info.max_stack_size),
            trim_materials: &[],
        })
    }

    /// Names of the server's `trim_material` registry, indexed by registry id.
    pub fn with_trim_materials(mut self, names: &'a [String]) -> Self {
        self.trim_materials = names;
        self
    }

    pub fn name(&self) -> &str {
        self.name
    }

    fn component(&self, name: &str) -> Option<&DataComponent> {
        let id = mcr_protocol::types::component::component_type_id(name)?;
        self.stack.components.get(id)
    }

    fn removed(&self, name: &str) -> bool {
        mcr_protocol::types::component::component_type_id(name)
            .is_some_and(|id| self.stack.components.removed.contains(&id))
    }

    pub fn has_foil(&self) -> bool {
        match self.component("enchantment_glint_override") {
            Some(DataComponent::EnchantmentGlintOverride(v)) => *v,
            _ => {
                matches!(self.component("enchantments"), Some(DataComponent::Enchantments(e)) if !e.0.is_empty())
                    || (GLINT_BY_DEFAULT.contains(&self.name)
                        && !self.removed("enchantment_glint_override"))
            }
        }
    }

    pub fn view<'b>(&'b self, cooldown: f32) -> ItemView<'b> {
        ItemView {
            item: self.name,
            props: self,
            glint: self.has_foil(),
            cooldown,
        }
    }
}

impl ItemProps for StackView<'_> {
    fn count(&self) -> i32 {
        self.stack.count
    }

    fn max_stack_size(&self) -> i32 {
        match self.component("max_stack_size") {
            Some(DataComponent::MaxStackSize(v)) => v.0,
            _ => self.max_stack,
        }
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

    fn firework_colors(&self) -> &[i32] {
        match self.component("firework_explosion") {
            Some(DataComponent::FireworkExplosion(e)) => &e.colors,
            _ => &[],
        }
    }

    fn trim_material(&self) -> Option<&str> {
        use mcr_buf::IdOr;
        match self.component("trim") {
            Some(DataComponent::Trim(t)) => match &t.material {
                IdOr::Id(id) => self
                    .trim_materials
                    .get(usize::try_from(*id).ok()?)
                    .map(String::as_str),
                IdOr::Inline(m) => Some(m.palette_id.as_str()),
            },
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
            Some(DataComponent::ChargedProjectiles(list)) => match list.0.first() {
                None => ChargeType::None,
                Some(p) => {
                    let rocket = v26_3()
                        .item_by_name("minecraft:firework_rocket")
                        .is_some_and(|id| i32::try_from(id) == Ok(p.item));
                    if rocket {
                        ChargeType::Rocket
                    } else {
                        ChargeType::Arrow
                    }
                }
            },
            _ => ChargeType::None,
        }
    }
}
