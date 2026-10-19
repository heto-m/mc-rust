//! The enchanting screen's book: it opens while an enchantment is offered and flips pages when the
//! item changes (`EnchantmentScreen#tickBook`).

use mcr_protocol::types::item::ItemStack;

use crate::gui::env::GameView;

const MAX_FLIP_TRIES: usize = 64;

#[derive(Clone, Debug)]
pub(super) struct BookAnim {
    pub(super) flip: f32,
    pub(super) o_flip: f32,
    flip_t: f32,
    flip_a: f32,
    pub(super) open: f32,
    pub(super) o_open: f32,
    last: ItemStack,
    rng: u64,
}

impl Default for BookAnim {
    fn default() -> Self {
        Self {
            flip: 0.0,
            o_flip: 0.0,
            flip_t: 0.0,
            flip_a: 0.0,
            open: 0.0,
            o_open: 0.0,
            last: ItemStack::default(),
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl BookAnim {
    fn next_int4(&mut self) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 32) as i32 & 3
    }

    pub(super) fn tick(&mut self, item: Option<&ItemStack>, offered: bool) {
        let current = item.cloned().unwrap_or_default();
        if current != self.last {
            self.last = current;
            for _ in 0..MAX_FLIP_TRIES {
                self.flip_t += (self.next_int4() - self.next_int4()) as f32;
                if (self.flip - self.flip_t).abs() > 1.0 {
                    break;
                }
            }
        }
        self.o_flip = self.flip;
        self.o_open = self.open;
        self.open = (self.open + if offered { 0.2 } else { -0.2 }).clamp(0.0, 1.0);
        let diff = ((self.flip_t - self.flip) * 0.4).clamp(-0.2, 0.2);
        self.flip_a += (diff - self.flip_a) * 0.9;
        self.flip += self.flip_a;
    }

    pub(super) fn tick_view(&mut self, view: &dyn GameView) {
        let offered = (0..3).any(|i| view.data(i) != 0);
        self.tick(view.slot(0), offered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_opens_while_offered_and_flips_on_a_new_item() {
        let mut book = BookAnim::default();
        let sword = ItemStack::new(1, 1);
        for _ in 0..10 {
            book.tick(Some(&sword), true);
        }
        assert_eq!(book.open, 1.0);
        assert!(book.flip_t.abs() > 1.0);
        for _ in 0..80 {
            book.tick(Some(&sword), false);
        }
        assert_eq!(book.open, 0.0);
        assert!((book.flip - book.flip_t).abs() < 0.05);
    }
}
