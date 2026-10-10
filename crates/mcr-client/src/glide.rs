//! Elytra, firework boosts and riptide: the client side of `LivingEntity#canGlide` and friends.

use mcr_physics::RIPTIDE_SPIN_TICKS;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, EquipmentSlot};
use mcr_protocol::types::entity_data::EntityDataValue;

use crate::client::Client;
use crate::inventory::{HOTBAR_START, OFFHAND_MENU_SLOT};
use crate::menu::Rules;

const FIREWORK_ATTACHED_INDEX: u8 = 9;
const FLAGS_INDEX: u8 = 0;
const LIVING_FLAGS_INDEX: u8 = 8;
const FALL_FLYING_BIT: i8 = 0x80u8 as i8;
const SPIN_ATTACK_BIT: i8 = 4;

/// Server-supplied riptide strength is bounded so a hostile enchantment cannot fling the player.
fn clamp_strength(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 16.0)
    } else {
        0.0
    }
}

fn next_damage_will_break(rules: &Rules<'_>, s: &ItemStack) -> bool {
    if !rules.is_damageable(s) {
        return false;
    }
    let damage = s
        .components
        .added
        .iter()
        .find_map(|c| match c {
            DataComponent::Damage(v) => Some(v.0),
            _ => None,
        })
        .unwrap_or(0);
    let max = s
        .components
        .added
        .iter()
        .find_map(|c| match c {
            DataComponent::MaxDamage(v) => Some(v.0),
            _ => None,
        })
        .or_else(|| {
            u32::try_from(s.item)
                .ok()
                .and_then(|i| rules.data.item_info(i))
                .map(|i| i32::try_from(i.max_damage).unwrap_or(i32::MAX))
        })
        .unwrap_or(0);
    damage >= max.saturating_sub(1)
}

impl Client {
    fn rules(&self) -> Rules<'_> {
        Rules {
            data: self.state.data,
            registries: &self.state.registries,
            creative: false,
        }
    }

    /// `canGlideUsing` over every equipment slot of the local player.
    pub fn glider_equipped(&self) -> bool {
        let rules = self.rules();
        let inv = &self.state.inventory.player.slots;
        let held = HOTBAR_START + usize::from(self.state.player.held_slot);
        [
            (5, EquipmentSlot::Head),
            (6, EquipmentSlot::Chest),
            (7, EquipmentSlot::Legs),
            (8, EquipmentSlot::Feet),
            (OFFHAND_MENU_SLOT, EquipmentSlot::OffHand),
            (held, EquipmentSlot::MainHand),
        ]
        .into_iter()
        .any(|(slot, equip)| {
            inv.get(slot).is_some_and(|s| {
                !s.is_empty()
                    && rules.has(s, "minecraft:glider")
                    && rules.equip(s).is_some_and(|e| e.slot == equip)
                    && !next_damage_will_break(&rules, s)
            })
        })
    }

    /// Rockets attached to the local player push along the view while gliding.
    pub(crate) fn apply_firework_boosts(&mut self) {
        let me = self.state.player.entity_id;
        let Some(rocket) = self
            .state
            .data
            .entity_type_by_name("minecraft:firework_rocket")
        else {
            return;
        };
        let attached = self
            .state
            .entities
            .iter()
            .filter(|e| e.kind == rocket as i32)
            .filter(|e| {
                matches!(
                    e.data.get(&FIREWORK_ATTACHED_INDEX),
                    Some(EntityDataValue::OptionalUnsignedInt(Some(id))) if i32::try_from(*id).is_ok_and(|id| id == me)
                )
            })
            .count();
        for _ in 0..attached.min(8) {
            self.state.player.physics.apply_firework_boost();
        }
    }

    pub(crate) fn in_water_or_rain(&self) -> bool {
        let p = &self.state.player.physics;
        if p.in_water() {
            return true;
        }
        if self.state.world.rain_level <= 0.2 {
            return false;
        }
        let (x, z) = (p.pos[0].floor() as i32, p.pos[2].floor() as i32);
        let y = p.pos[1].floor() as i32;
        (y + 1..y + 320).all(|yy| {
            self.state.chunks.block(x, yy, z).is_none_or(|b| {
                self.state
                    .data
                    .block_state(u32::from(b.0))
                    .is_none_or(|i| self.state.data.block_name(i.block) == Some("minecraft:air"))
            })
        })
    }

    /// Strength of the riptide enchantment on `stack`, from the synced enchantment definition.
    pub(crate) fn riptide_strength(&self, stack: &ItemStack) -> f32 {
        let Some(enchants) = stack.components.added.iter().find_map(|c| match c {
            DataComponent::Enchantments(e) => Some(e),
            _ => None,
        }) else {
            return 0.0;
        };
        for level in &enchants.0 {
            let Some(item) = self
                .state
                .registries
                .get("minecraft:enchantment")
                .and_then(|r| r.get(level.enchantment))
            else {
                continue;
            };
            let Some(effects) = item.data.as_ref().and_then(|d| d.get_compound("effects")) else {
                if item.id.as_str() == "minecraft:riptide" {
                    return clamp_strength(0.75 + 0.75 * level.level.clamp(0, 255) as f32);
                }
                continue;
            };
            if let Some(strength) = effects.get_compound("minecraft:trident_spin_attack_strength") {
                let strength = strength.get_compound("value").unwrap_or(strength);
                let base = strength.get_f64("base").unwrap_or(0.0);
                let per = strength.get_f64("per_level_above_first").unwrap_or(0.0);
                let above_first = f64::from(level.level.clamp(1, 255) - 1);
                return clamp_strength((base + per * above_first) as f32);
            }
        }
        0.0
    }

    /// Releasing a riptide trident: push, spin and hop, as `TridentItem#releaseUsing` does locally.
    pub(crate) fn start_riptide(&mut self, strength: f32) {
        self.state
            .player
            .physics
            .start_riptide(&self.state.chunks, strength);
        let _ = RIPTIDE_SPIN_TICKS;
    }
}

/// Entity-data driven flags the server owns: gliding and the riptide spin pose.
pub(crate) fn apply_player_flags(player: &mut crate::player::LocalPlayer) {
    let byte = |index: u8| match player.data.get(&index) {
        Some(EntityDataValue::Byte(b)) => Some(*b),
        _ => None,
    };
    let (flags, living) = (byte(FLAGS_INDEX), byte(LIVING_FLAGS_INDEX));
    if let Some(f) = flags {
        player.physics.fall_flying = f & FALL_FLYING_BIT != 0;
    }
    if let Some(l) = living {
        player.physics.spin_attack_flag = l & SPIN_ATTACK_BIT != 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_protocol::types::component::DataComponent;

    fn rules() -> (&'static mcr_data::Version, crate::registry::Registries) {
        (mcr_data::v26_3(), crate::registry::Registries::default())
    }

    #[test]
    fn hostile_damage_values_do_not_overflow() {
        let (data, regs) = rules();
        let rules = Rules {
            data,
            registries: &regs,
            creative: false,
        };
        let item = data.item_by_name("minecraft:elytra").unwrap() as i32;
        for (damage, max) in [
            (i32::MAX, i32::MIN),
            (i32::MIN, i32::MIN),
            (i32::MAX, i32::MAX),
            (-1, 0),
        ] {
            let mut s = ItemStack::new(item, 1);
            s.components
                .added
                .push(DataComponent::Damage(mcr_buf::VarInt(damage)));
            s.components
                .added
                .push(DataComponent::MaxDamage(mcr_buf::VarInt(max)));
            let _ = next_damage_will_break(&rules, &s);
        }
    }

    #[test]
    fn strength_is_clamped() {
        assert_eq!(clamp_strength(f32::NAN), 0.0);
        assert_eq!(clamp_strength(f32::INFINITY), 0.0);
        assert_eq!(clamp_strength(1.0e9), 16.0);
        assert_eq!(clamp_strength(-5.0), 0.0);
    }
}
