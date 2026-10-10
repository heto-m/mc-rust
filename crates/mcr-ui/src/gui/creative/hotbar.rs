use mcr_protocol::types::item::{ItemStack, UntrustedItemStack};

pub const HOTBAR_SAVES: usize = 9;
pub const HOTBAR_SIZE: usize = 9;

/// The nine saved hotbars of the creative "Saved Hotbar Activator" tab.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HotbarStore {
    saves: [Option<[ItemStack; HOTBAR_SIZE]>; HOTBAR_SAVES],
    pub dirty: bool,
}

impl HotbarStore {
    pub fn get(&self, index: usize) -> Option<&[ItemStack; HOTBAR_SIZE]> {
        self.saves.get(index)?.as_ref()
    }

    pub fn save(&mut self, index: usize, hotbar: [ItemStack; HOTBAR_SIZE]) {
        if let Some(slot) = self.saves.get_mut(index) {
            *slot = Some(hotbar);
            self.dirty = true;
        }
    }

    /// One line per non-empty save: `index` then nine hex stacks.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (i, save) in self.saves.iter().enumerate() {
            let Some(save) = save else { continue };
            out.push_str(&i.to_string());
            for stack in save {
                out.push(' ');
                for byte in mcr_buf::to_bytes(&UntrustedItemStack(stack.clone())) {
                    out.push_str(&format!("{byte:02x}"));
                }
            }
            out.push('\n');
        }
        out
    }

    pub fn from_text(text: &str) -> Self {
        let mut store = Self::default();
        for line in text.lines() {
            let mut parts = line.split(' ');
            let Some(index) = parts.next().and_then(|i| i.parse::<usize>().ok()) else {
                continue;
            };
            let stacks: Vec<ItemStack> = parts.filter_map(parse_stack).collect();
            if let (true, Ok(hotbar)) = (
                index < HOTBAR_SAVES,
                <[ItemStack; HOTBAR_SIZE]>::try_from(stacks),
            ) {
                store.saves[index] = Some(hotbar);
            }
        }
        store
    }
}

fn parse_stack(hex: &str) -> Option<ItemStack> {
    if !hex.len().is_multiple_of(2) || hex.len() > 1 << 17 {
        return None;
    }
    let bytes: Option<Vec<u8>> = hex
        .as_bytes()
        .chunks(2)
        .map(|p| {
            let d = |b: u8| char::from(b).to_digit(16);
            Some((d(p[0])? * 16 + d(p[1])?) as u8)
        })
        .collect();
    mcr_buf::from_bytes_exact::<UntrustedItemStack>(&bytes?)
        .ok()
        .map(|s| s.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_round_trip_through_text() {
        let mut store = HotbarStore::default();
        let mut hotbar: [ItemStack; HOTBAR_SIZE] = Default::default();
        hotbar[0] = ItemStack::new(5, 64);
        hotbar[8] = ItemStack::new(9, 1);
        store.save(2, hotbar.clone());
        let back = HotbarStore::from_text(&store.to_text());
        assert_eq!(back.get(2), Some(&hotbar));
        assert_eq!(back.get(0), None);
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let store = HotbarStore::from_text("x\n3 zz\n99 00 00\n");
        assert!(store.saves.iter().all(Option::is_none));
    }
}
