#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub page: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug)]
struct Shelf {
    y: u32,
    height: u32,
    cursor: u32,
}

#[derive(Debug)]
pub struct Page {
    pub pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_y: u32,
    dirty: Option<(u32, u32)>,
}

/// RGBA8 shelf-packed pages with 1px spacing; every page starts with a white 2x2 block for solid fills.
#[derive(Debug)]
pub struct Atlas {
    size: u32,
    pages: Vec<Page>,
}

impl Atlas {
    pub fn new(size: u32) -> Self {
        let mut atlas = Self {
            size,
            pages: Vec::new(),
        };
        atlas.add_page();
        atlas
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn pages_mut(&mut self) -> &mut [Page] {
        &mut self.pages
    }

    pub fn white_uv(&self) -> [f32; 2] {
        [1.0 / self.size as f32; 2]
    }

    fn add_page(&mut self) -> u32 {
        let mut page = Page {
            pixels: vec![0; (self.size * self.size * 4) as usize],
            shelves: Vec::new(),
            next_y: 0,
            dirty: None,
        };
        let size = self.size;
        let slot = Self::alloc_in(&mut page, size, 2, 2).expect("fresh page fits 2x2");
        let index = self.pages.len() as u32;
        self.pages.push(page);
        self.write(
            Slot {
                page: index,
                ..slot
            },
            2,
            2,
            &[255; 16],
        );
        index
    }

    fn alloc_in(page: &mut Page, size: u32, w: u32, h: u32) -> Option<Slot> {
        let (pw, ph) = (w + 1, h + 1);
        if pw > size || ph > size {
            return None;
        }
        let best = page
            .shelves
            .iter_mut()
            .filter(|s| s.height >= ph && s.cursor + pw <= size)
            .min_by_key(|s| s.height);
        if let Some(shelf) = best {
            let slot = Slot {
                page: 0,
                x: shelf.cursor,
                y: shelf.y,
            };
            shelf.cursor += pw;
            return Some(slot);
        }
        if page.next_y + ph > size {
            return None;
        }
        let slot = Slot {
            page: 0,
            x: 0,
            y: page.next_y,
        };
        page.shelves.push(Shelf {
            y: page.next_y,
            height: ph,
            cursor: pw,
        });
        page.next_y += ph;
        Some(slot)
    }

    /// `None` when the rectangle is larger than a page.
    pub fn alloc(&mut self, w: u32, h: u32) -> Option<Slot> {
        let size = self.size;
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some(slot) = Self::alloc_in(page, size, w, h) {
                return Some(Slot {
                    page: i as u32,
                    ..slot
                });
            }
        }
        if w + 1 > size || h + 1 > size {
            return None;
        }
        let index = self.add_page();
        let page = &mut self.pages[index as usize];
        Self::alloc_in(page, size, w, h).map(|slot| Slot {
            page: index,
            ..slot
        })
    }

    pub fn write(&mut self, slot: Slot, w: u32, h: u32, rgba: &[u8]) {
        debug_assert_eq!(rgba.len(), (w * h * 4) as usize);
        let size = self.size as usize;
        let page = &mut self.pages[slot.page as usize];
        for row in 0..h as usize {
            let dst = ((slot.y as usize + row) * size + slot.x as usize) * 4;
            let src = row * w as usize * 4;
            page.pixels[dst..dst + w as usize * 4]
                .copy_from_slice(&rgba[src..src + w as usize * 4]);
        }
        let (y0, y1) = (slot.y, slot.y + h);
        page.dirty = Some(match page.dirty {
            Some((a, b)) => (a.min(y0), b.max(y1)),
            None => (y0, y1),
        });
    }
}

impl Page {
    /// Rows `[y0, y1)` that changed since the last call.
    pub fn take_dirty(&mut self) -> Option<(u32, u32)> {
        self.dirty.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_do_not_overlap_and_spill_to_new_pages() {
        let mut atlas = Atlas::new(32);
        let mut slots = Vec::new();
        for _ in 0..40 {
            slots.push(atlas.alloc(7, 5).unwrap());
        }
        assert!(atlas.pages().len() > 1);
        for (i, a) in slots.iter().enumerate() {
            for b in &slots[i + 1..] {
                if a.page == b.page {
                    let apart =
                        a.x + 7 <= b.x || b.x + 7 <= a.x || a.y + 5 <= b.y || b.y + 5 <= a.y;
                    assert!(apart, "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn oversized_rectangles_are_rejected() {
        assert!(Atlas::new(16).alloc(16, 4).is_none());
    }

    #[test]
    fn white_block_is_opaque_white() {
        let atlas = Atlas::new(16);
        assert_eq!(&atlas.pages()[0].pixels[..4], &[255; 4]);
    }
}
