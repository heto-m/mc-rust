//! Synched entity data indices of 26.3: each class continues after its superclass' last index.

pub const FLAGS: u8 = 0;
pub const SILENT: u8 = 4;
pub const POSE: u8 = 6;

pub const LIVING_FLAGS: u8 = 8;
pub const MOB_FLAGS: u8 = 15;
/// `AgeableMob` and most animals; piglins and zombies define their own.
pub const BABY: u8 = 16;
pub const ZOMBIE_BABY: u8 = 16;
pub const PIGLIN_BABY: u8 = 17;

pub const TAMABLE_FLAGS: u8 = 18;
pub const TAMABLE_FLAG_SITTING: u8 = 0;
pub const TAMABLE_FLAG_TAME: u8 = 2;

pub const SHEEP_WOOL: u8 = 18;
pub const PIG_VARIANT: u8 = 19;
pub const COW_VARIANT: u8 = 18;
pub const CHICKEN_VARIANT: u8 = 18;
pub const FROG_VARIANT: u8 = 18;
pub const MOOSHROOM_TYPE: u8 = 18;
pub const RABBIT_TYPE: u8 = 18;
pub const AXOLOTL_VARIANT: u8 = 18;
pub const AXOLOTL_PLAYING_DEAD: u8 = 19;
pub const FOX_TYPE: u8 = 18;
pub const FOX_FLAGS: u8 = 19;
pub const PARROT_VARIANT: u8 = 20;
pub const HORSE_FLAGS: u8 = 18;
pub const HORSE_VARIANT: u8 = 19;
pub const LLAMA_VARIANT: u8 = 21;
pub const CAT_VARIANT: u8 = 20;
pub const CAT_LYING: u8 = 21;
pub const CAT_COLLAR: u8 = 23;
pub const WOLF_ANGER: u8 = 22;
pub const WOLF_COLLAR: u8 = 21;
pub const WOLF_VARIANT: u8 = 23;
pub const PANDA_MAIN_GENE: u8 = 21;
pub const PANDA_HIDDEN_GENE: u8 = 22;
pub const PANDA_FLAGS: u8 = 23;
pub const PANDA_UNHAPPY: u8 = 18;
pub const PANDA_SNEEZE: u8 = 19;
pub const PANDA_EAT: u8 = 20;
pub const VILLAGER_DATA: u8 = 19;
pub const ZOMBIE_VILLAGER_DATA: u8 = 20;
pub const ZOMBIE_VILLAGER_CONVERTING: u8 = 19;
pub const SLIME_SIZE: u8 = 18;
pub const CREEPER_SWELL: u8 = 16;
pub const CREEPER_POWERED: u8 = 17;
pub const ENDERMAN_CARRY: u8 = 16;
pub const ENDERMAN_CREEPY: u8 = 17;
pub const TROPICAL_VARIANT: u8 = 17;
pub const SALMON_TYPE: u8 = 17;
pub const PUFFER_STATE: u8 = 17;
pub const SHULKER_PEEK: u8 = 17;
pub const SHULKER_COLOR: u8 = 18;
pub const GOAT_SCREAMING: u8 = 18;
pub const GOAT_LEFT_HORN: u8 = 19;
pub const GOAT_RIGHT_HORN: u8 = 20;
pub const SNOW_GOLEM_PUMPKIN: u8 = 16;
pub const IRON_GOLEM_FLAGS: u8 = 16;
pub const SPIDER_FLAGS: u8 = 16;
pub const BLAZE_FLAGS: u8 = 16;
pub const VEX_FLAGS: u8 = 16;
pub const BAT_FLAGS: u8 = 16;
pub const BEE_FLAGS: u8 = 18;
pub const BEE_ANGER_END: u8 = 19;
pub const ARMADILLO_STATE: u8 = 18;
pub const SNIFFER_STATE: u8 = 18;
pub const CAMEL_DASH: u8 = 19;
pub const CAMEL_LAST_POSE: u8 = 20;
pub const POLAR_BEAR_STANDING: u8 = 18;
pub const STRIDER_SUFFOCATING: u8 = 19;
pub const TURTLE_EGG: u8 = 18;
pub const GHAST_CHARGING: u8 = 16;
pub const GUARDIAN_MOVING: u8 = 16;
pub const GUARDIAN_ATTACK_TARGET: u8 = 17;
pub const PHANTOM_SIZE: u8 = 16;
pub const WITCH_USING_ITEM: u8 = 17;
pub const PILLAGER_CHARGING: u8 = 17;
pub const PIGLIN_CHARGING: u8 = 18;
pub const PIGLIN_DANCING: u8 = 19;
pub const ALLAY_DANCING: u8 = 16;
pub const COPPER_GOLEM_WEATHER: u8 = 16;
pub const COPPER_GOLEM_STATE: u8 = 17;
pub const ZOMBIE_NAUTILUS_VARIANT: u8 = 21;
pub const HAPPY_GHAST_BABY: u8 = 16;
pub const BOGGED_SHEARED: u8 = 16;
pub const CREAKING_ACTIVE: u8 = 17;
pub const WITHER_INVULNERABLE: u8 = 19;
pub const WITHER_SKULL_DANGEROUS: u8 = 8;
pub const DOLPHIN_BABY: u8 = 16;

pub const PLAYER_MAIN_HAND: u8 = 15;
pub const PLAYER_MODEL_PARTS: u8 = 16;

pub const ARMOR_STAND_FLAGS: u8 = 15;
pub const ARMOR_STAND_HEAD_POSE: u8 = 16;
pub const ARMOR_STAND_BODY_POSE: u8 = 17;
pub const ARMOR_STAND_LEFT_ARM_POSE: u8 = 18;
pub const ARMOR_STAND_RIGHT_ARM_POSE: u8 = 19;
pub const ARMOR_STAND_LEFT_LEG_POSE: u8 = 20;
pub const ARMOR_STAND_RIGHT_LEG_POSE: u8 = 21;

pub const ITEM_STACK: u8 = 8;
pub const FRAME_DIRECTION: u8 = 8;
pub const FRAME_ITEM: u8 = 9;
pub const FRAME_ROTATION: u8 = 10;
pub const PAINTING_DIRECTION: u8 = 8;
pub const PAINTING_VARIANT: u8 = 9;
pub const FALLING_BLOCK_POS: u8 = 8;
pub const END_CRYSTAL_SHOW_BOTTOM: u8 = 9;
pub const MINECART_DISPLAY_BLOCK: u8 = 11;
pub const MINECART_DISPLAY_OFFSET: u8 = 12;
pub const TNT_FUSE: u8 = 8;
pub const TNT_BLOCK: u8 = 9;
pub const MINECART_FURNACE_FUEL: u8 = 13;
pub const BOAT_HURT: u8 = 8;

/// `ThrownTrident.ID_FOIL`, after `AbstractArrow`'s three fields and the loyalty byte.
pub const THROWN_TRIDENT_FOIL: u8 = 12;

/// `Cushion.DATA_COLOR`, the first field after `Entity`'s.
pub const CUSHION_COLOR: u8 = 8;

pub const DISPLAY_INTERPOLATION_DELAY: u8 = 8;
pub const DISPLAY_INTERPOLATION_DURATION: u8 = 9;
pub const DISPLAY_TRANSLATION: u8 = 11;
pub const DISPLAY_SCALE: u8 = 12;
pub const DISPLAY_LEFT_ROTATION: u8 = 13;
pub const DISPLAY_RIGHT_ROTATION: u8 = 14;
pub const DISPLAY_BILLBOARD: u8 = 15;
pub const DISPLAY_BRIGHTNESS: u8 = 16;
pub const DISPLAY_VIEW_RANGE: u8 = 17;
pub const DISPLAY_WIDTH: u8 = 20;
pub const DISPLAY_HEIGHT: u8 = 21;
pub const TEXT_DISPLAY_TEXT: u8 = 23;
pub const TEXT_DISPLAY_LINE_WIDTH: u8 = 24;
pub const TEXT_DISPLAY_BACKGROUND: u8 = 25;
pub const TEXT_DISPLAY_OPACITY: u8 = 26;
pub const TEXT_DISPLAY_FLAGS: u8 = 27;
