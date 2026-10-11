use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, Modifiers, MouseButton, UiInput, contains};
use super::tooltip::TooltipLines;

const WHITE: u32 = 0xFFFF_FFFF;
const DISABLED_TEXT: u32 = 0xFFA0_A0A0;

#[derive(Clone, Debug)]
pub struct Button {
    pub id: u32,
    pub rect: [i32; 4],
    pub label: String,
    pub active: bool,
    pub visible: bool,
    /// Draws nothing itself; the owner paints the button.
    pub flat: bool,
    pub tooltip: Option<TooltipLines>,
}

impl Button {
    pub fn new(id: u32, rect: [i32; 4], label: impl Into<String>) -> Self {
        Self {
            id,
            rect,
            label: label.into(),
            active: true,
            visible: true,
            flat: false,
            tooltip: None,
        }
    }

    pub fn flat(mut self) -> Self {
        self.flat = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.active = false;
        self
    }

    fn render(&self, cx: &mut RenderCx<'_, '_>, highlighted: bool) {
        let [x, y, w, h] = self.rect;
        let sprite = match (self.active, highlighted) {
            (false, _) => "widget/button_disabled",
            (true, true) => "widget/button_highlighted",
            (true, false) => "widget/button",
        };
        cx.sprite(sprite, x, y, w, h);
        draw_label(cx, &self.label, self.rect, self.active);
    }
}

/// Labels written `lang:key` are looked up in the language file when drawn.
fn draw_label(cx: &mut RenderCx<'_, '_>, label: &str, rect: [i32; 4], active: bool) {
    let resolved;
    let label = match label.strip_prefix("lang:") {
        Some(key) => {
            resolved = cx.lang(key);
            resolved.as_str()
        }
        None => label,
    };
    let [x, y, w, h] = rect;
    let width = cx.text_width(label);
    let color = if active { WHITE } else { DISABLED_TEXT };
    let ty = y + (h - 8) / 2;
    if width > w - 4 {
        cx.frame.push_scissor(x + 2, y, x + w - 2, y + h);
        cx.text(label, x + 2, ty, color, true);
        cx.frame.pop_scissor();
    } else {
        cx.text(label, x + (w - width) / 2, ty, color, true);
    }
}

#[derive(Clone, Debug)]
pub struct Slider {
    pub id: u32,
    pub rect: [i32; 4],
    pub label: String,
    pub active: bool,
    pub visible: bool,
    value: f32,
}

impl Slider {
    pub fn new(id: u32, rect: [i32; 4], label: impl Into<String>, value: f32) -> Self {
        Self {
            id,
            rect,
            label: label.into(),
            active: true,
            visible: true,
            value: value.clamp(0.0, 1.0),
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }

    pub fn set_from_x(&mut self, x: f32) {
        let [bx, _, w, _] = self.rect;
        self.set_value((x - (bx + 4) as f32) / (w - 8).max(1) as f32);
    }

    fn step(&mut self, direction: f32) {
        let step = 1.0 / (self.rect[2] - 8).max(1) as f32;
        self.set_value(self.value + direction * step);
    }

    fn render(&self, cx: &mut RenderCx<'_, '_>, highlighted: bool) {
        let [x, y, w, h] = self.rect;
        let bg = if highlighted {
            "widget/slider_highlighted"
        } else {
            "widget/slider"
        };
        cx.sprite(bg, x, y, w, h);
        let handle = if highlighted {
            "widget/slider_handle_highlighted"
        } else {
            "widget/slider_handle"
        };
        cx.sprite(handle, x + (self.value * (w - 8) as f32) as i32, y, 8, h);
        draw_label(cx, &self.label, self.rect, self.active);
    }
}

#[derive(Clone, Debug)]
pub struct CycleButton {
    pub id: u32,
    pub rect: [i32; 4],
    pub name: String,
    pub values: Vec<String>,
    pub active: bool,
    pub visible: bool,
    index: usize,
}

impl CycleButton {
    pub fn new(
        id: u32,
        rect: [i32; 4],
        name: impl Into<String>,
        values: Vec<String>,
        index: usize,
    ) -> Self {
        let index = index.min(values.len().saturating_sub(1));
        Self {
            id,
            rect,
            name: name.into(),
            values,
            active: true,
            visible: true,
            index,
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn cycle(&mut self, delta: i32) {
        let n = self.values.len() as i32;
        if n > 0 {
            self.index = (self.index as i32 + delta).rem_euclid(n) as usize;
        }
    }

    fn label(&self, cx: &mut RenderCx<'_, '_>) -> String {
        let resolve = |cx: &mut RenderCx<'_, '_>, s: &str| match s.strip_prefix("lang:") {
            Some(key) => cx.lang(key),
            None => s.to_owned(),
        };
        match self.values.get(self.index) {
            Some(v) if self.name.is_empty() => resolve(cx, v),
            Some(v) => format!("{}: {}", resolve(cx, &self.name), resolve(cx, v)),
            None => resolve(cx, &self.name),
        }
    }

    fn render(&self, cx: &mut RenderCx<'_, '_>, highlighted: bool) {
        let [x, y, w, h] = self.rect;
        let sprite = match (self.active, highlighted) {
            (false, _) => "widget/button_disabled",
            (true, true) => "widget/button_highlighted",
            (true, false) => "widget/button",
        };
        cx.sprite(sprite, x, y, w, h);
        let label = self.label(cx);
        draw_label(cx, &label, self.rect, self.active);
    }
}

#[derive(Clone, Debug)]
pub enum Widget {
    Button(Button),
    Slider(Slider),
    Cycle(CycleButton),
    Edit(EditBox),
}

impl Widget {
    pub fn id(&self) -> u32 {
        match self {
            Self::Button(w) => w.id,
            Self::Slider(w) => w.id,
            Self::Cycle(w) => w.id,
            Self::Edit(w) => w.id,
        }
    }

    pub fn rect(&self) -> [i32; 4] {
        match self {
            Self::Button(w) => w.rect,
            Self::Slider(w) => w.rect,
            Self::Cycle(w) => w.rect,
            Self::Edit(w) => w.rect,
        }
    }

    pub fn set_rect(&mut self, rect: [i32; 4]) {
        match self {
            Self::Button(w) => w.rect = rect,
            Self::Slider(w) => w.rect = rect,
            Self::Cycle(w) => w.rect = rect,
            Self::Edit(w) => w.rect = rect,
        }
    }

    pub fn active(&self) -> bool {
        match self {
            Self::Button(w) => w.active,
            Self::Slider(w) => w.active,
            Self::Cycle(w) => w.active,
            Self::Edit(w) => w.active,
        }
    }

    pub fn visible(&self) -> bool {
        match self {
            Self::Button(w) => w.visible,
            Self::Slider(w) => w.visible,
            Self::Cycle(w) => w.visible,
            Self::Edit(w) => w.visible,
        }
    }

    pub fn focusable(&self) -> bool {
        self.active() && self.visible()
    }

    pub fn set_active(&mut self, active: bool) {
        match self {
            Self::Button(w) => w.active = active,
            Self::Slider(w) => w.active = active,
            Self::Cycle(w) => w.active = active,
            Self::Edit(w) => w.active = active,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        match self {
            Self::Button(w) => w.visible = visible,
            Self::Slider(w) => w.visible = visible,
            Self::Cycle(w) => w.visible = visible,
            Self::Edit(w) => w.visible = visible,
        }
    }
}

impl From<Button> for Widget {
    fn from(w: Button) -> Self {
        Self::Button(w)
    }
}

impl From<Slider> for Widget {
    fn from(w: Slider) -> Self {
        Self::Slider(w)
    }
}

impl From<CycleButton> for Widget {
    fn from(w: CycleButton) -> Self {
        Self::Cycle(w)
    }
}

impl From<EditBox> for Widget {
    fn from(w: EditBox) -> Self {
        Self::Edit(w)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WidgetEventKind {
    Pressed,
    SliderChanged(f32),
    Cycled(usize),
    Edited,
    Submitted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidgetEvent {
    pub id: u32,
    pub kind: WidgetEventKind,
}

#[derive(Clone, Debug, Default)]
pub struct WidgetSet {
    widgets: Vec<Widget>,
    focus: Option<usize>,
    dragging: Option<usize>,
}

impl WidgetSet {
    pub fn clear(&mut self) {
        self.widgets.clear();
        self.focus = None;
        self.dragging = None;
    }

    pub fn add(&mut self, widget: impl Into<Widget>) -> usize {
        self.widgets.push(widget.into());
        self.widgets.len() - 1
    }

    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    pub fn find(&self, id: u32) -> Option<&Widget> {
        self.widgets.iter().find(|w| w.id() == id)
    }

    pub fn find_mut(&mut self, id: u32) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|w| w.id() == id)
    }

    pub fn button_mut(&mut self, id: u32) -> Option<&mut Button> {
        match self.find_mut(id)? {
            Widget::Button(b) => Some(b),
            _ => None,
        }
    }

    pub fn edit(&self, id: u32) -> Option<&EditBox> {
        match self.find(id)? {
            Widget::Edit(e) => Some(e),
            _ => None,
        }
    }

    pub fn edit_mut(&mut self, id: u32) -> Option<&mut EditBox> {
        match self.find_mut(id)? {
            Widget::Edit(e) => Some(e),
            _ => None,
        }
    }

    pub fn slider_mut(&mut self, id: u32) -> Option<&mut Slider> {
        match self.find_mut(id)? {
            Widget::Slider(s) => Some(s),
            _ => None,
        }
    }

    pub fn cycle_mut(&mut self, id: u32) -> Option<&mut CycleButton> {
        match self.find_mut(id)? {
            Widget::Cycle(c) => Some(c),
            _ => None,
        }
    }

    pub fn focused(&self) -> Option<u32> {
        self.focus.map(|i| self.widgets[i].id())
    }

    pub fn set_focus(&mut self, id: Option<u32>, now_ms: u64) {
        self.focus = id.and_then(|id| self.widgets.iter().position(|w| w.id() == id));
        self.touch_focus(now_ms);
    }

    fn touch_focus(&mut self, now_ms: u64) {
        if let Some(Widget::Edit(e)) = self.focus.map(|i| &mut self.widgets[i]) {
            e.focus(now_ms);
        }
    }

    pub fn navigate(&mut self, forward: bool, now_ms: u64) {
        let n = self.widgets.len();
        if n == 0 {
            return;
        }
        let start = match (self.focus, forward) {
            (Some(i), _) => i,
            (None, true) => n - 1,
            (None, false) => 0,
        };
        for step in 1..=n {
            let i = if forward {
                (start + step) % n
            } else {
                (start + n - step % n) % n
            };
            if self.widgets[i].focusable() {
                self.focus = Some(i);
                self.touch_focus(now_ms);
                return;
            }
        }
    }

    pub fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (mx, my) = cx.mouse;
        let mut hovered_tip = None;
        for (i, widget) in self.widgets.iter_mut().enumerate() {
            if !widget.visible() {
                continue;
            }
            let focused = self.focus == Some(i);
            let hovered = contains(widget.rect(), mx, my);
            match widget {
                Widget::Button(b) => {
                    if !b.flat {
                        b.render(cx, hovered && b.active || focused);
                    }
                    if hovered && let Some(t) = &b.tooltip {
                        hovered_tip = Some(t.clone());
                    }
                }
                Widget::Slider(s) => s.render(cx, hovered || focused),
                Widget::Cycle(c) => c.render(cx, hovered && c.active || focused),
                Widget::Edit(e) => e.render(cx, focused),
            }
        }
        if let Some(t) = hovered_tip {
            cx.set_tooltip(t);
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.widgets
            .iter()
            .rposition(|w| w.visible() && contains(w.rect(), x, y))
    }

    pub fn input(
        &mut self,
        env: &mut Env<'_>,
        input: &UiInput,
        events: &mut Vec<WidgetEvent>,
    ) -> bool {
        let now = env.now_ms;
        match *input {
            UiInput::MousePress { x, y, button, .. } => {
                let hit = self.hit(x, y);
                if let Some(old) = self.focus
                    && Some(old) != hit
                    && matches!(self.widgets[old], Widget::Edit(_))
                {
                    self.focus = None;
                }
                let Some(i) = hit else {
                    return false;
                };
                if !self.widgets[i].active() {
                    return true;
                }
                self.focus = Some(i);
                let id = self.widgets[i].id();
                match &mut self.widgets[i] {
                    Widget::Button(_) if button == MouseButton::Left => {
                        events.push(WidgetEvent {
                            id,
                            kind: WidgetEventKind::Pressed,
                        });
                    }
                    Widget::Cycle(c)
                        if matches!(button, MouseButton::Left | MouseButton::Right) =>
                    {
                        c.cycle(if button == MouseButton::Left { 1 } else { -1 });
                        events.push(WidgetEvent {
                            id,
                            kind: WidgetEventKind::Cycled(c.index()),
                        });
                    }
                    Widget::Slider(s) if button == MouseButton::Left => {
                        s.set_from_x(x);
                        self.dragging = Some(i);
                        events.push(WidgetEvent {
                            id,
                            kind: WidgetEventKind::SliderChanged(s.value()),
                        });
                    }
                    Widget::Edit(e) if button == MouseButton::Left => {
                        e.click(x, now, false);
                    }
                    _ => {}
                }
                true
            }
            UiInput::MouseMove { x, .. } => {
                if let Some(i) = self.dragging
                    && let Widget::Slider(s) = &mut self.widgets[i]
                {
                    s.set_from_x(x);
                    events.push(WidgetEvent {
                        id: s.id,
                        kind: WidgetEventKind::SliderChanged(s.value()),
                    });
                    return true;
                }
                false
            }
            UiInput::MouseRelease { .. } => self.dragging.take().is_some(),
            UiInput::Scroll { x, y, dy, .. } => {
                let Some(i) = self.hit(x, y) else {
                    return false;
                };
                let id = self.widgets[i].id();
                match &mut self.widgets[i] {
                    Widget::Cycle(c) if c.active => {
                        c.cycle(if dy > 0.0 { -1 } else { 1 });
                        events.push(WidgetEvent {
                            id,
                            kind: WidgetEventKind::Cycled(c.index()),
                        });
                        true
                    }
                    _ => false,
                }
            }
            UiInput::Key { key, mods, .. } => self.key(env, key, mods, events),
            UiInput::Char(c) => {
                let Some(i) = self.focus else {
                    return false;
                };
                if let Widget::Edit(e) = &mut self.widgets[i]
                    && e.char(c)
                {
                    events.push(WidgetEvent {
                        id: e.id,
                        kind: WidgetEventKind::Edited,
                    });
                    return true;
                }
                false
            }
        }
    }

    fn key(
        &mut self,
        env: &mut Env<'_>,
        key: Key,
        mods: Modifiers,
        events: &mut Vec<WidgetEvent>,
    ) -> bool {
        if let Some(i) = self.focus {
            let id = self.widgets[i].id();
            match &mut self.widgets[i] {
                Widget::Edit(e) => {
                    let before = e.value().to_owned();
                    if key == Key::Enter {
                        events.push(WidgetEvent {
                            id,
                            kind: WidgetEventKind::Submitted,
                        });
                        return true;
                    }
                    if e.key(key, mods, env.clipboard) {
                        if e.value() != before {
                            events.push(WidgetEvent {
                                id,
                                kind: WidgetEventKind::Edited,
                            });
                        }
                        return true;
                    }
                }
                Widget::Button(b) if b.active && matches!(key, Key::Enter | Key::Space) => {
                    events.push(WidgetEvent {
                        id,
                        kind: WidgetEventKind::Pressed,
                    });
                    return true;
                }
                Widget::Cycle(c) if c.active && matches!(key, Key::Enter | Key::Space) => {
                    c.cycle(1);
                    events.push(WidgetEvent {
                        id,
                        kind: WidgetEventKind::Cycled(c.index()),
                    });
                    return true;
                }
                Widget::Slider(s) if s.active && matches!(key, Key::Left | Key::Right) => {
                    s.step(if key == Key::Left { -1.0 } else { 1.0 });
                    events.push(WidgetEvent {
                        id,
                        kind: WidgetEventKind::SliderChanged(s.value()),
                    });
                    return true;
                }
                _ => {}
            }
        }
        match key {
            Key::Tab => {
                self.navigate(!mods.shift, env.now_ms);
                true
            }
            Key::Down if !matches!(self.focused_widget(), Some(Widget::Edit(_))) => {
                self.navigate(true, env.now_ms);
                true
            }
            Key::Up if !matches!(self.focused_widget(), Some(Widget::Edit(_))) => {
                self.navigate(false, env.now_ms);
                true
            }
            _ => false,
        }
    }

    fn focused_widget(&self) -> Option<&Widget> {
        self.focus.map(|i| &self.widgets[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};
    use crate::gui::input::Modifiers;

    fn set() -> WidgetSet {
        let mut w = WidgetSet::default();
        w.add(Button::new(1, [0, 0, 100, 20], "a"));
        w.add(Button::new(2, [0, 30, 100, 20], "b").disabled());
        w.add(Button::new(3, [0, 60, 100, 20], "c"));
        w
    }

    fn feed(w: &mut WidgetSet, input: UiInput) -> Vec<WidgetEvent> {
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut events = Vec::new();
        w.input(&mut env, &input, &mut events);
        events
    }

    #[test]
    fn tab_wraps_and_skips_disabled() {
        let mut w = set();
        feed(&mut w, UiInput::key(Key::Tab));
        assert_eq!(w.focused(), Some(1));
        feed(&mut w, UiInput::key(Key::Tab));
        assert_eq!(w.focused(), Some(3));
        feed(&mut w, UiInput::key(Key::Tab));
        assert_eq!(w.focused(), Some(1));
    }

    #[test]
    fn shift_tab_goes_backwards_and_wraps() {
        let mut w = set();
        feed(&mut w, UiInput::key_mods(Key::Tab, Modifiers::SHIFT));
        assert_eq!(w.focused(), Some(3));
        feed(&mut w, UiInput::key_mods(Key::Tab, Modifiers::SHIFT));
        assert_eq!(w.focused(), Some(1));
        feed(&mut w, UiInput::key_mods(Key::Tab, Modifiers::SHIFT));
        assert_eq!(w.focused(), Some(3));
    }

    #[test]
    fn arrows_navigate_between_buttons() {
        let mut w = set();
        feed(&mut w, UiInput::key(Key::Down));
        assert_eq!(w.focused(), Some(1));
        feed(&mut w, UiInput::key(Key::Down));
        assert_eq!(w.focused(), Some(3));
        feed(&mut w, UiInput::key(Key::Up));
        assert_eq!(w.focused(), Some(1));
    }

    #[test]
    fn enter_and_space_press_the_focused_button() {
        let mut w = set();
        feed(&mut w, UiInput::key(Key::Tab));
        let events = feed(&mut w, UiInput::key(Key::Enter));
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::Pressed
            }]
        );
        let events = feed(&mut w, UiInput::key(Key::Space));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn clicking_presses_enabled_buttons_only() {
        let mut w = set();
        let events = feed(&mut w, UiInput::press(10.0, 10.0, MouseButton::Left));
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::Pressed
            }]
        );
        assert!(feed(&mut w, UiInput::press(10.0, 40.0, MouseButton::Left)).is_empty());
        assert!(feed(&mut w, UiInput::press(10.0, 10.0, MouseButton::Right)).is_empty());
    }

    #[test]
    fn slider_follows_the_mouse_while_dragging() {
        let mut w = WidgetSet::default();
        w.add(Slider::new(1, [0, 0, 108, 20], "s", 0.0));
        let events = feed(&mut w, UiInput::press(4.0 + 50.0, 5.0, MouseButton::Left));
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::SliderChanged(0.5)
            }]
        );
        let events = feed(&mut w, UiInput::MouseMove { x: 500.0, y: 5.0 });
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::SliderChanged(1.0)
            }]
        );
        feed(&mut w, UiInput::release(500.0, 5.0, MouseButton::Left));
        assert!(feed(&mut w, UiInput::MouseMove { x: 0.0, y: 5.0 }).is_empty());
    }

    #[test]
    fn slider_keys_step_by_one_pixel() {
        let mut w = WidgetSet::default();
        w.add(Slider::new(1, [0, 0, 108, 20], "s", 0.5));
        w.set_focus(Some(1), 0);
        feed(&mut w, UiInput::key(Key::Right));
        let v = w.slider_mut(1).unwrap().value();
        assert!((v - (0.5 + 0.01)).abs() < 1e-6);
    }

    #[test]
    fn cycle_button_cycles_both_ways() {
        let mut w = WidgetSet::default();
        w.add(CycleButton::new(
            1,
            [0, 0, 100, 20],
            "Mode",
            vec!["a".into(), "b".into(), "c".into()],
            0,
        ));
        let events = feed(&mut w, UiInput::press(5.0, 5.0, MouseButton::Left));
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::Cycled(1)
            }]
        );
        let events = feed(&mut w, UiInput::press(5.0, 5.0, MouseButton::Right));
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::Cycled(0)
            }]
        );
        let events = feed(&mut w, UiInput::press(5.0, 5.0, MouseButton::Right));
        assert_eq!(
            events,
            [WidgetEvent {
                id: 1,
                kind: WidgetEventKind::Cycled(2)
            }]
        );
    }

    #[test]
    fn edit_box_loses_focus_when_clicking_elsewhere() {
        let mut w = WidgetSet::default();
        w.add(EditBox::new(1, [0, 0, 100, 20]));
        w.add(Button::new(2, [0, 30, 100, 20], "b"));
        w.set_focus(Some(1), 0);
        assert_eq!(w.focused(), Some(1));
        feed(&mut w, UiInput::press(5.0, 200.0, MouseButton::Left));
        assert_eq!(w.focused(), None);
    }

    #[test]
    fn edit_box_arrows_do_not_move_focus() {
        let mut w = WidgetSet::default();
        w.add(EditBox::new(1, [0, 0, 100, 20]));
        w.add(Button::new(2, [0, 30, 100, 20], "b"));
        w.set_focus(Some(1), 0);
        feed(&mut w, UiInput::key(Key::Down));
        assert_eq!(w.focused(), Some(1));
    }
}
