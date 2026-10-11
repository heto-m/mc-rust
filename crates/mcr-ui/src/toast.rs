use std::collections::VecDeque;

use mcr_protocol::types::item::ItemStack;

use crate::gui::RenderCx;
use crate::items::StackView;
use crate::style::Cell;
use crate::ui::Ui;

pub const SLOTS: usize = 5;
pub const SLOT_HEIGHT: i32 = 32;
pub const SLIDE_MS: i64 = 600;
pub const DISPLAY_MS: i64 = 5000;
pub const TOAST_IN: &str = "ui.toast.in";
pub const TOAST_OUT: &str = "ui.toast.out";
pub const TOAST_CHALLENGE: &str = "ui.toast.challenge_complete";
const WRAP_WIDTH: f32 = 125.0;
const SYSTEM_WRAP_WIDTH: f32 = 200.0;
const MAX_QUEUED: usize = 64;
const MAX_RECIPE_ENTRIES: usize = 64;
const MAX_LINES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Show,
    Hide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastFrame {
    Task,
    Goal,
    Challenge,
}

impl ToastFrame {
    fn heading_key(self) -> &'static str {
        match self {
            Self::Task => "advancements.toast.task",
            Self::Goal => "advancements.toast.goal",
            Self::Challenge => "advancements.toast.challenge",
        }
    }

    fn heading_color(self) -> u32 {
        match self {
            Self::Challenge => 0xFFFF_88FF,
            _ => 0xFFFF_FF00,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemToastId {
    pub key: u32,
    pub display_ms: i64,
}

impl SystemToastId {
    pub const fn new(key: u32) -> Self {
        Self {
            key,
            display_ms: DISPLAY_MS,
        }
    }

    pub const fn long(key: u32) -> Self {
        Self {
            key,
            display_ms: 2 * DISPLAY_MS,
        }
    }
}

#[derive(Debug)]
enum Kind {
    Advancement {
        frame: ToastFrame,
        title: Vec<Vec<Cell>>,
        icon: ItemStack,
    },
    Recipe {
        entries: Vec<(ItemStack, ItemStack)>,
        last_changed: i64,
        changed: bool,
        shown: usize,
    },
    System {
        id: SystemToastId,
        title: Vec<Vec<Cell>>,
        message: Vec<Vec<Cell>>,
        last_changed: i64,
        changed: bool,
        force_hide: bool,
    },
}

#[derive(Debug)]
pub struct Toast {
    kind: Kind,
    width: i32,
    wanted: Visibility,
}

impl Toast {
    pub fn advancement(frame: ToastFrame, title: Vec<Vec<Cell>>, icon: ItemStack) -> Self {
        Self {
            kind: Kind::Advancement { frame, title, icon },
            width: 160,
            wanted: Visibility::Hide,
        }
    }

    pub fn recipe(station: ItemStack, result: ItemStack) -> Self {
        Self {
            kind: Kind::Recipe {
                entries: vec![(station, result)],
                last_changed: 0,
                changed: true,
                shown: 0,
            },
            width: 160,
            wanted: Visibility::Hide,
        }
    }

    pub fn system(
        id: SystemToastId,
        title: Vec<Vec<Cell>>,
        message: Vec<Vec<Cell>>,
        text_width: f32,
    ) -> Self {
        Self {
            kind: Kind::System {
                id,
                title,
                message,
                last_changed: 0,
                changed: true,
                force_hide: false,
            },
            width: (text_width.ceil() as i32).max(160) + 30,
            wanted: Visibility::Hide,
        }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        match &self.kind {
            Kind::System { title, message, .. } => {
                20 + title.len().saturating_sub(1) as i32 * 12 + message.len().max(1) as i32 * 12
            }
            _ => SLOT_HEIGHT,
        }
    }

    fn slots(&self) -> usize {
        (self.height() as usize).div_ceil(SLOT_HEIGHT as usize)
    }

    fn sound(&self) -> Option<&'static str> {
        match self.kind {
            Kind::Advancement {
                frame: ToastFrame::Challenge,
                ..
            } => Some(TOAST_CHALLENGE),
            _ => None,
        }
    }

    fn update(&mut self, fully_visible_for: i64, multiplier: f64) {
        let scaled = |ms: i64| (ms as f64 * multiplier) as i64;
        let wanted = |expired: bool| {
            if expired {
                Visibility::Hide
            } else {
                Visibility::Show
            }
        };
        self.wanted = match &mut self.kind {
            Kind::Advancement { .. } => wanted(fully_visible_for >= scaled(DISPLAY_MS)),
            Kind::Recipe {
                entries,
                last_changed,
                changed,
                shown,
            } => {
                if std::mem::take(changed) {
                    *last_changed = fully_visible_for;
                }
                let period = (scaled(DISPLAY_MS) / entries.len().max(1) as i64).max(1);
                *shown = (fully_visible_for / period) as usize % entries.len().max(1);
                wanted(
                    entries.is_empty() || fully_visible_for - *last_changed >= scaled(DISPLAY_MS),
                )
            }
            Kind::System {
                id,
                last_changed,
                changed,
                force_hide,
                ..
            } => {
                if std::mem::take(changed) {
                    *last_changed = fully_visible_for;
                }
                wanted(*force_hide || fully_visible_for - *last_changed >= scaled(id.display_ms))
            }
        };
    }

    fn draw(&self, cx: &mut RenderCx<'_, '_>, (ox, oy): (i32, i32), fully_visible_for: i64) {
        let line = |cx: &mut RenderCx<'_, '_>, cells: &[Cell], x: i32, y: i32, argb: u32| {
            cx.frame
                .cells(cells, (ox + x) as f32, (oy + y) as f32, argb, false);
        };
        match &self.kind {
            Kind::Advancement { frame, title, icon } => {
                cx.sprite("toast/advancement", ox, oy, self.width, self.height());
                let heading = cx.lang(frame.heading_key());
                let color = frame.heading_color();
                if let [only] = title.as_slice() {
                    cx.text(&heading, ox + 30, oy + 7, color, false);
                    line(cx, only, 30, 18, 0xFFFF_FFFF);
                } else if fully_visible_for < 1500 {
                    let fade = ((1500 - fully_visible_for) as f32 / 300.0).clamp(0.0, 1.0);
                    let alpha = (fade * 255.0) as u32;
                    let tinted = (alpha << 24) | (color & 0xFF_FFFF);
                    cx.text(&heading, ox + 30, oy + 11, tinted, false);
                } else {
                    let fade = ((fully_visible_for - 1500) as f32 / 300.0).clamp(0.0, 1.0);
                    let alpha = (fade * 252.0) as u32;
                    let mut y = self.height() / 2 - title.len() as i32 * 9 / 2;
                    for text in title {
                        line(cx, text, 30, y, (alpha << 24) | 0xFF_FFFF);
                        y += 9;
                    }
                }
                cx.stack_opt(Some(icon), ox + 8, oy + 8);
            }
            Kind::Recipe { entries, shown, .. } => {
                cx.sprite("toast/recipe", ox, oy, self.width, self.height());
                let title = cx.lang("recipe.toast.title");
                let description = cx.lang("recipe.toast.description");
                cx.text(&title, ox + 30, oy + 7, 0xFF50_0050, false);
                cx.text(&description, ox + 30, oy + 18, 0xFF00_0000, false);
                if let Some((station, result)) = entries.get(*shown) {
                    if let Some(view) = StackView::new(station) {
                        let item = view.view(0.0);
                        cx.icons.draw_sized(cx.frame, &item, ox + 2, oy + 2, 10, 10);
                    }
                    cx.stack_opt(Some(result), ox + 8, oy + 8);
                }
            }
            Kind::System { title, message, .. } => {
                cx.sprite("toast/system", ox, oy, self.width, self.height());
                let (title_y, message_y) = if message.is_empty() {
                    (12, 0)
                } else {
                    (7, 7 + title.len() as i32 * 12)
                };
                for (i, text) in title.iter().enumerate() {
                    line(cx, text, 18, title_y + i as i32 * 12, 0xFFFF_FF00);
                }
                for (i, text) in message.iter().enumerate() {
                    line(cx, text, 18, message_y + i as i32 * 12, 0xFFFF_FFFF);
                }
            }
        }
    }
}

#[derive(Debug)]
struct Instance {
    toast: Toast,
    first_slot: usize,
    slots: usize,
    animation_start: Option<i64>,
    fully_visible_at: i64,
    visibility: Visibility,
    fully_visible_for: i64,
    portion: f32,
    finished: bool,
}

impl Instance {
    fn new(toast: Toast, first_slot: usize) -> Self {
        let slots = toast.slots();
        Self {
            toast,
            first_slot,
            slots,
            animation_start: None,
            fully_visible_at: 0,
            visibility: Visibility::Hide,
            fully_visible_for: 0,
            portion: 0.0,
            finished: false,
        }
    }

    fn update(&mut self, now: i64, multiplier: f64) {
        let start = *self.animation_start.get_or_insert_with(|| {
            self.visibility = Visibility::Show;
            now
        });
        if self.visibility == Visibility::Show && now - start <= SLIDE_MS {
            self.fully_visible_at = now;
        }
        self.fully_visible_for = now - self.fully_visible_at;
        let progress = ((now - start) as f32 / SLIDE_MS as f32).clamp(0.0, 1.0);
        let progress = progress * progress;
        self.portion = if self.visibility == Visibility::Hide {
            1.0 - progress
        } else {
            progress
        };
        self.toast.update(self.fully_visible_for, multiplier);
        let wanted = self.toast.wanted;
        let mut start = start;
        if wanted != self.visibility {
            start = now - ((1.0 - self.portion) * SLIDE_MS as f32) as i64;
            self.animation_start = Some(start);
            self.visibility = wanted;
        }
        self.finished = self.visibility == Visibility::Hide && now - start > SLIDE_MS;
    }
}

#[derive(Debug, Default)]
pub struct ToastManager {
    visible: Vec<Instance>,
    occupied: [bool; SLOTS],
    queued: VecDeque<Toast>,
}

impl ToastManager {
    pub fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.queued.is_empty()
    }

    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    pub fn queued_count(&self) -> usize {
        self.queued.len()
    }

    pub fn add(&mut self, toast: Toast) {
        if self.queued.len() < MAX_QUEUED {
            self.queued.push_back(toast);
        }
    }

    pub fn clear(&mut self) {
        self.visible.clear();
        self.queued.clear();
        self.occupied = [false; SLOTS];
    }

    pub fn add_advancement(
        &mut self,
        ui: &mut Ui,
        frame: ToastFrame,
        title: &mcr_protocol::types::Component,
        icon: ItemStack,
    ) {
        let cells = ui.flatten(title);
        let lines = wrap_lines(ui, &cells, WRAP_WIDTH);
        self.add(Toast::advancement(frame, lines, icon));
    }

    /// Joins the recipe toast that is already waiting or showing, like vanilla.
    pub fn add_recipe(&mut self, station: ItemStack, result: ItemStack) {
        let existing = self
            .visible
            .iter_mut()
            .map(|i| &mut i.toast)
            .chain(self.queued.iter_mut())
            .find(|t| matches!(t.kind, Kind::Recipe { .. }));
        match existing {
            Some(Toast {
                kind: Kind::Recipe {
                    entries, changed, ..
                },
                ..
            }) => {
                if entries.len() < MAX_RECIPE_ENTRIES {
                    entries.push((station, result));
                }
                *changed = true;
            }
            _ => self.add(Toast::recipe(station, result)),
        }
    }

    /// Replaces the text of a waiting or showing toast with the same id, else queues a new one.
    pub fn add_system(
        &mut self,
        ui: &mut Ui,
        id: SystemToastId,
        title: &str,
        message: Option<&str>,
    ) {
        let title_cells = crate::text::parse_legacy(title);
        let title = wrap_lines(ui, &title_cells, SYSTEM_WRAP_WIDTH);
        let message = message.map_or_else(Vec::new, |m| {
            wrap_lines(ui, &crate::text::parse_legacy(m), SYSTEM_WRAP_WIDTH)
        });
        let text_width = title
            .iter()
            .chain(&message)
            .map(|l| ui.width(l))
            .fold(0.0, f32::max);
        let fresh = Toast::system(id, title, message, text_width);
        let existing = self
            .visible
            .iter_mut()
            .map(|i| &mut i.toast)
            .chain(self.queued.iter_mut())
            .find(|t| matches!(&t.kind, Kind::System { id: other, .. } if other.key == id.key));
        match existing {
            Some(toast) => {
                let Kind::System { title, message, .. } = fresh.kind else {
                    return;
                };
                toast.width = fresh.width;
                if let Kind::System {
                    title: t,
                    message: m,
                    changed,
                    ..
                } = &mut toast.kind
                {
                    *t = title;
                    *m = message;
                    *changed = true;
                }
            }
            None => self.add(fresh),
        }
    }

    fn free_slots(&self) -> usize {
        self.occupied.iter().filter(|o| !**o).count()
    }

    fn find_slots(&self, needed: usize) -> Option<usize> {
        if needed == 0 || self.free_slots() < needed {
            return None;
        }
        let mut run = 0;
        for (i, taken) in self.occupied.iter().enumerate() {
            run = if *taken { 0 } else { run + 1 };
            if run == needed {
                return Some(i + 1 - needed);
            }
        }
        None
    }

    /// Advances every toast to `now_ms`; returns the sound events to play, in order.
    pub fn update(&mut self, now_ms: u64, multiplier: f64) -> Vec<&'static str> {
        let now = now_ms as i64;
        let mut sounds = Vec::new();
        let mut visibility_sound = false;
        let occupied = &mut self.occupied;
        self.visible.retain_mut(|instance| {
            let before = instance.visibility;
            instance.update(now, multiplier);
            if instance.visibility != before && !visibility_sound {
                visibility_sound = true;
                sounds.push(match instance.visibility {
                    Visibility::Show => TOAST_IN,
                    Visibility::Hide => TOAST_OUT,
                });
            }
            if instance.finished {
                for slot in &mut occupied[instance.first_slot..instance.first_slot + instance.slots]
                {
                    *slot = false;
                }
            }
            !instance.finished
        });
        let mut played = Vec::new();
        let mut index = 0;
        while index < self.queued.len() {
            let needed = self.queued[index].slots();
            let Some(first) = self.find_slots(needed) else {
                index += 1;
                continue;
            };
            let toast = self.queued.remove(index).expect("index is in range");
            self.occupied[first..first + needed].fill(true);
            if let Some(sound) = toast.sound()
                && !played.contains(&sound)
            {
                played.push(sound);
                sounds.push(sound);
            }
            self.visible.push(Instance::new(toast, first));
        }
        sounds
    }

    pub fn draw(&self, cx: &mut RenderCx<'_, '_>) {
        let (screen_width, _) = cx.gui_size();
        for instance in &self.visible {
            if instance.finished {
                continue;
            }
            let toast = &instance.toast;
            let x = screen_width as f32 - toast.width() as f32 * instance.portion;
            let y = instance.first_slot as i32 * toast.height();
            toast.draw(cx, (x.floor() as i32, y), instance.fully_visible_for);
        }
    }
}

fn wrap_lines(ui: &mut Ui, cells: &[Cell], width: f32) -> Vec<Vec<Cell>> {
    ui.wrap(cells, width)
        .into_iter()
        .take(MAX_LINES)
        .map(|r| cells[r].to_vec())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> Toast {
        Toast::advancement(ToastFrame::Task, vec![Vec::new()], ItemStack::default())
    }

    fn challenge() -> Toast {
        Toast::advancement(
            ToastFrame::Challenge,
            vec![Vec::new()],
            ItemStack::default(),
        )
    }

    #[test]
    fn slides_in_holds_and_slides_out() {
        let mut m = ToastManager::default();
        m.add(task());
        assert!(m.update(0, 1.0).is_empty());
        assert_eq!(m.update(0, 1.0), [TOAST_IN]);
        assert_eq!(m.visible[0].portion, 0.0);
        m.update(300, 1.0);
        assert!((m.visible[0].portion - 0.25).abs() < 1e-6);
        m.update(600, 1.0);
        assert_eq!(m.visible[0].portion, 1.0);
        assert!(m.update(4000, 1.0).is_empty());
        assert_eq!(m.visible[0].visibility, Visibility::Show);
        assert_eq!(m.update(5600, 1.0), [TOAST_OUT]);
        m.update(6000, 1.0);
        assert!(m.visible[0].portion < 1.0 && m.visible[0].portion > 0.0);
        m.update(6200, 1.0);
        assert_eq!(m.visible_count(), 1);
        m.update(6201, 1.0);
        assert_eq!(m.visible_count(), 0);
        assert!(m.is_empty());
    }

    #[test]
    fn at_most_five_visible_and_the_rest_wait() {
        let mut m = ToastManager::default();
        for _ in 0..7 {
            m.add(task());
        }
        m.update(0, 1.0);
        m.update(0, 1.0);
        assert_eq!((m.visible_count(), m.queued_count()), (5, 2));
        let slots: Vec<_> = m.visible.iter().map(|i| i.first_slot).collect();
        assert_eq!(slots, [0, 1, 2, 3, 4]);
        m.update(5600, 1.0);
        m.update(6300, 1.0);
        m.update(6300 + 601, 1.0);
        assert_eq!((m.visible_count(), m.queued_count()), (2, 0));
    }

    #[test]
    fn one_visibility_sound_per_update_and_challenge_sound_deduped() {
        let mut m = ToastManager::default();
        m.add(challenge());
        m.add(challenge());
        m.add(task());
        assert_eq!(m.update(0, 1.0), [TOAST_CHALLENGE]);
        assert_eq!(m.update(0, 1.0), [TOAST_IN]);
        assert_eq!(m.update(5600, 1.0), [TOAST_OUT]);
    }

    #[test]
    fn display_time_follows_the_multiplier() {
        let mut m = ToastManager::default();
        m.add(task());
        m.update(0, 2.0);
        m.update(0, 2.0);
        assert!(m.update(9000, 2.0).is_empty());
        assert_eq!(m.update(10600, 2.0), [TOAST_OUT]);
    }

    #[test]
    fn recipes_merge_and_cycle() {
        let mut m = ToastManager::default();
        m.add_recipe(ItemStack::new(1, 1), ItemStack::new(2, 1));
        m.add_recipe(ItemStack::new(1, 1), ItemStack::new(3, 1));
        assert_eq!(m.queued_count(), 1);
        m.update(0, 1.0);
        m.add_recipe(ItemStack::new(1, 1), ItemStack::new(4, 1));
        assert_eq!(m.visible_count() + m.queued_count(), 1);
        m.update(100, 1.0);
        let Kind::Recipe { shown, entries, .. } = &m.visible[0].toast.kind else {
            panic!("recipe toast");
        };
        assert_eq!((entries.len(), *shown), (3, 0));
        m.update(1800, 1.0);
        let Kind::Recipe { shown, .. } = &m.visible[0].toast.kind else {
            panic!("recipe toast");
        };
        assert_eq!(*shown, 1);
    }

    #[test]
    fn adding_to_a_recipe_toast_restarts_its_timer() {
        let mut m = ToastManager::default();
        m.add_recipe(ItemStack::new(1, 1), ItemStack::new(2, 1));
        m.update(0, 1.0);
        m.update(4000, 1.0);
        m.add_recipe(ItemStack::new(1, 1), ItemStack::new(3, 1));
        m.update(4100, 1.0);
        m.update(7000, 1.0);
        assert_eq!(m.visible[0].visibility, Visibility::Show);
    }

    #[test]
    fn multi_slot_toast_needs_consecutive_free_slots() {
        let mut m = ToastManager::default();
        let tall = Toast::system(
            SystemToastId::new(1),
            vec![Vec::new(); 3],
            vec![Vec::new(); 2],
            10.0,
        );
        assert_eq!(tall.height(), 20 + 24 + 24);
        assert_eq!(tall.slots(), 3);
        assert_eq!(tall.width(), 190);
        for _ in 0..3 {
            m.add(task());
        }
        m.update(0, 1.0);
        m.add(tall);
        m.update(10, 1.0);
        assert_eq!(m.queued_count(), 1);
    }

    #[test]
    fn force_hidden_system_toast_leaves_early() {
        let mut m = ToastManager::default();
        m.add(Toast::system(
            SystemToastId::new(7),
            vec![Vec::new()],
            Vec::new(),
            0.0,
        ));
        m.update(0, 1.0);
        m.update(1000, 1.0);
        assert_eq!(m.visible[0].visibility, Visibility::Show);
        let Kind::System { force_hide, .. } = &mut m.visible[0].toast.kind else {
            panic!("system toast");
        };
        *force_hide = true;
        assert_eq!(m.update(1100, 1.0), [TOAST_OUT]);
    }

    #[test]
    fn queue_is_bounded() {
        let mut m = ToastManager::default();
        for _ in 0..1000 {
            m.add(task());
        }
        assert_eq!(m.queued_count(), MAX_QUEUED);
    }
}
