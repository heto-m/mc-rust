mod click;
mod kind;
mod menus;
mod screen;

pub use click::{
    ClickMachine, SlotGeometry, can_quick_replace, may_place, quick_craft_mask,
    quick_craft_place_count,
};
pub use kind::{ContainerLayout, FurnaceKind, MenuKind, SlotKind, SlotSpec};
pub use menus::BeaconState;
pub use screen::ContainerScreen;
