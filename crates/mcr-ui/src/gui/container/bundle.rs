//! Vanilla's `BundleMouseActions`: scrolling over a bundle selects one of its items for the server.

use mcr_protocol::types::item::ItemStack;

use super::screen::ContainerScreen;
use crate::gui::env::GameView;
use crate::gui::screen::{ClickType, ContainerClick, ScreenAction};
use crate::items::{bundle_items, items_to_show};

/// `ScrollWheelHandler#onMouseScroll` and `getNextScrollWheelSelection`.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ScrollWheel {
    accumulated: f64,
}

impl ScrollWheel {
    pub(super) fn wheel(&mut self, dy: f64) -> i32 {
        if self.accumulated != 0.0 && dy.signum() != self.accumulated.signum() {
            self.accumulated = 0.0;
        }
        self.accumulated += dy;
        let wheel = self.accumulated as i32;
        if wheel != 0 {
            self.accumulated -= f64::from(wheel);
        }
        wheel
    }
}

pub(super) fn next_selection(wheel: i32, current: i32, limit: i32) -> i32 {
    if limit <= 0 {
        return -1;
    }
    let next = (current - wheel.signum()).max(-1);
    next.rem_euclid(limit)
}

fn bundle_at(view: &dyn GameView, slot: usize) -> Option<&ItemStack> {
    view.slot(slot).filter(|s| bundle_items(s).is_some())
}

impl ContainerScreen {
    pub(super) fn bundle_selected(&self, view: &dyn GameView, slot: usize) -> Option<usize> {
        let (s, index) = self.bundle_selection?;
        let len = bundle_at(view, slot)
            .and_then(bundle_items)
            .map_or(0, <[_]>::len);
        (s == slot && index < len).then_some(index)
    }

    /// True when the scroll went to a bundle under the mouse.
    pub(super) fn bundle_scroll(
        &mut self,
        view: &dyn GameView,
        dy: f32,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        let Some(slot) = self.hovered_slot() else {
            return false;
        };
        let Some(items) = bundle_at(view, slot).and_then(bundle_items) else {
            return false;
        };
        let shown = items_to_show(items.len()) as i32;
        if shown == 0 {
            return false;
        }
        let wheel = self.bundle_wheel.wheel(f64::from(dy));
        if wheel != 0 {
            let current = self.bundle_selected(view, slot).map_or(-1, |i| i as i32);
            let next = next_selection(wheel, current, shown);
            if next != current {
                self.select_bundle(slot, next, out);
            }
        }
        true
    }

    fn select_bundle(&mut self, slot: usize, index: i32, out: &mut Vec<ScreenAction>) {
        self.bundle_selection = usize::try_from(index).ok().map(|i| (slot, i));
        out.push(ScreenAction::SelectBundleItem {
            slot: slot as i32,
            index,
        });
    }

    /// `onStopHovering`: leaving a bundle slot clears its selection on the server.
    pub(super) fn bundle_hover_changed(
        &mut self,
        view: &dyn GameView,
        out: &mut Vec<ScreenAction>,
    ) {
        let hovered = self.hovered_slot();
        let previous = std::mem::replace(&mut self.bundle_hovered, hovered);
        if let Some(prev) = previous
            && previous != hovered
            && bundle_at(view, prev).is_some()
        {
            self.select_bundle(prev, -1, out);
        }
    }

    /// `onSlotClicked` for shift-clicks and swaps, and the local reset a left click on the bundle does.
    pub(super) fn bundle_clicked(
        &mut self,
        view: &dyn GameView,
        click: &ContainerClick,
        out: &mut Vec<ScreenAction>,
    ) {
        let Ok(slot) = usize::try_from(click.slot) else {
            return;
        };
        if bundle_at(view, slot).is_none() {
            return;
        }
        match click.mode {
            ClickType::QuickMove | ClickType::Swap => self.select_bundle(slot, -1, out),
            _ => {
                if self.bundle_selection.is_some_and(|(s, _)| s == slot) {
                    self.bundle_selection = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::types::component::DataComponent;
    use mcr_protocol::types::item::ItemStackTemplate;

    use super::*;
    use crate::gui::env::{Env, MemoryClipboard};
    use crate::gui::input::UiInput;
    use crate::gui::screen::Screen;

    struct Bundled(Vec<Option<ItemStack>>);

    impl GameView for Bundled {
        fn slot(&self, index: usize) -> Option<&ItemStack> {
            self.0.get(index)?.as_ref()
        }
    }

    fn item(name: &str) -> i32 {
        mcr_data::v26_3().item_by_name(name).unwrap() as i32
    }

    fn actions(screen: &mut ContainerScreen, view: &Bundled, input: UiInput) -> Vec<ScreenAction> {
        let mut clipboard = MemoryClipboard::default();
        let mut env = Env {
            view,
            clipboard: &mut clipboard,
            now_ms: 0,
        };
        let mut out = Vec::new();
        screen.input(&mut env, &input, &mut out);
        out
    }

    fn selections(out: &[ScreenAction]) -> Vec<(i32, i32)> {
        out.iter()
            .filter_map(|a| match a {
                ScreenAction::SelectBundleItem { slot, index } => Some((*slot, *index)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn scrolling_a_bundle_selects_items_and_leaving_clears_it() {
        let mut bundle = ItemStack::new(item("minecraft:bundle"), 1);
        let contents = ["minecraft:stone", "minecraft:dirt", "minecraft:sand"]
            .map(|n| ItemStackTemplate {
                item: item(n),
                count: 4,
                components: Default::default(),
            })
            .to_vec();
        bundle
            .components
            .added
            .push(DataComponent::BundleContents(contents));
        let mut slots = vec![None; 46];
        slots[10] = Some(bundle);
        let view = Bundled(slots);
        let mut screen = ContainerScreen::player_inventory();
        screen.init(320, 240);
        let (cx, cy) = screen.slot_center(10).unwrap();
        let (x, y) = (cx as f32, cy as f32);
        assert!(selections(&actions(&mut screen, &view, UiInput::MouseMove { x, y })).is_empty());
        let scroll = |dy| UiInput::Scroll { x, y, dx: 0.0, dy };
        assert_eq!(
            selections(&actions(&mut screen, &view, scroll(-1.0))),
            [(10, 0)]
        );
        assert_eq!(screen.bundle_selected(&view, 10), Some(0));
        assert_eq!(
            selections(&actions(&mut screen, &view, scroll(1.0))),
            [(10, 2)]
        );
        let away = UiInput::MouseMove { x: 1.0, y: 1.0 };
        assert_eq!(selections(&actions(&mut screen, &view, away)), [(10, -1)]);
        assert_eq!(screen.bundle_selected(&view, 10), None);
    }

    #[test]
    fn wheel_selection_wraps_like_vanilla() {
        assert_eq!(next_selection(-1, -1, 4), 0);
        assert_eq!(next_selection(-1, 3, 4), 0);
        assert_eq!(next_selection(1, 0, 4), 3);
        assert_eq!(next_selection(1, -1, 4), 3);
        assert_eq!(next_selection(1, 2, 4), 1);
    }

    #[test]
    fn fractional_scrolls_accumulate() {
        let mut w = ScrollWheel::default();
        assert_eq!(w.wheel(0.5), 0);
        assert_eq!(w.wheel(0.5), 1);
        assert_eq!(w.wheel(-0.4), 0);
        assert_eq!(w.wheel(-0.7), -1);
    }
}
