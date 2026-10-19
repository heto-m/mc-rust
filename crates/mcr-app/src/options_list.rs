//! `OptionsSubScreen` with its `OptionsList`: a titled, scrolling column of option buttons and sliders.

use std::rc::Rc;

use mcr_settings::Options;
use mcr_ui::{
    Button, Env, Key, MouseButton, RenderCx, Screen, ScreenAction, ScrollList, Slider,
    TooltipLines, UiInput, WidgetEventKind, WidgetSet, contains, draw_menu_background,
    tile_texture,
};

use crate::settings::SharedSettings;

pub const HEADER: i32 = 33;
pub const FOOTER: i32 = 33;
const ROW: i32 = 25;
const LINE: i32 = 9;
const SMALL: i32 = 150;
const BIG: i32 = 310;
const COLUMN: i32 = 160;
const BUTTON_HEIGHT: i32 = 20;
const FOOTER_ID: u32 = 1 << 20;
const WHITE: u32 = 0xFFFF_FFFF;

pub type Text = Rc<dyn Fn(&Options, &mut RenderCx<'_, '_>) -> String>;
type Get<T> = Rc<dyn Fn(&Options) -> T>;
type Set<T> = Rc<dyn Fn(&mut Options, T)>;
pub type Action = Rc<dyn Fn(&Options) -> Vec<ScreenAction>>;

pub enum Kind {
    /// Clicking steps through `count` values; Shift steps back.
    Cycle {
        count: usize,
        get: Get<usize>,
        set: Set<usize>,
    },
    /// The slider position is 0..=1; `deferred` sliders apply on release, like vanilla's
    /// `applyValueImmediately = false` ranges.
    Slider {
        get: Get<f64>,
        set: Set<f64>,
        deferred: bool,
    },
    Button(Action),
}

pub struct Opt {
    kind: Kind,
    label: Text,
    tooltip: Option<Text>,
    active: Get<bool>,
}

/// `options.generic_value`: "Caption: value".
pub fn generic(cx: &mut RenderCx<'_, '_>, caption: &str, value: &str) -> String {
    let caption = cx.lang(caption);
    cx.lang_fmt("options.generic_value", &[&caption, value])
}

/// `Options#genericValueLabel` with a translated value.
pub fn generic_key(cx: &mut RenderCx<'_, '_>, caption: &str, value_key: &str) -> String {
    let value = cx.lang(value_key);
    generic(cx, caption, &value)
}

pub fn percent(cx: &mut RenderCx<'_, '_>, caption: &str, value: f64) -> String {
    let caption = cx.lang(caption);
    let pct = ((value * 100.0) as i32).to_string();
    cx.lang_fmt("options.percent_value", &[&caption, &pct])
}

pub fn percent_or_off(cx: &mut RenderCx<'_, '_>, caption: &str, value: f64) -> String {
    if value == 0.0 {
        generic_key(cx, caption, "options.off")
    } else {
        percent(cx, caption, value)
    }
}

pub fn pixels(cx: &mut RenderCx<'_, '_>, caption: &str, px: i32) -> String {
    let caption = cx.lang(caption);
    cx.lang_fmt("options.pixel_value", &[&caption, &px.to_string()])
}

/// `CommonComponents#optionStatus(caption, value)`.
pub fn status(cx: &mut RenderCx<'_, '_>, caption: &str, on: bool) -> String {
    let caption = cx.lang(caption);
    let key = if on {
        "options.on.composed"
    } else {
        "options.off.composed"
    };
    cx.lang_fmt(key, &[&caption])
}

fn on_off(on: bool) -> &'static str {
    if on { "options.on" } else { "options.off" }
}

impl Opt {
    fn new(kind: Kind, label: Text) -> Self {
        Self {
            kind,
            label,
            tooltip: None,
            active: Rc::new(|_| true),
        }
    }

    pub fn button(
        label: &'static str,
        action: impl Fn(&Options) -> Vec<ScreenAction> + 'static,
    ) -> Self {
        Self::new(
            Kind::Button(Rc::new(action)),
            Rc::new(move |_, cx| cx.lang(label)),
        )
    }

    /// `OptionInstance.createBoolean`: "Caption: ON".
    pub fn toggle(
        caption: &'static str,
        get: fn(&Options) -> bool,
        set: fn(&mut Options, bool),
    ) -> Self {
        Self::toggle_labels(caption, get, set, "options.on", "options.off")
    }

    pub fn toggle_labels(
        caption: &'static str,
        get: fn(&Options) -> bool,
        set: fn(&mut Options, bool),
        on: &'static str,
        off: &'static str,
    ) -> Self {
        Self::new(
            Kind::Cycle {
                count: 2,
                get: Rc::new(move |o| usize::from(!get(o))),
                set: Rc::new(move |o, i| set(o, i == 0)),
            },
            Rc::new(move |o, cx| generic_key(cx, caption, if get(o) { on } else { off })),
        )
    }

    /// A toggle stored outside [`Options`] fields, such as a model part.
    pub fn toggle_with(
        caption: &'static str,
        get: impl Fn(&Options) -> bool + 'static,
        set: impl Fn(&mut Options, bool) + 'static,
    ) -> Self {
        let get: Get<bool> = Rc::new(get);
        let label_get = get.clone();
        Self::new(
            Kind::Cycle {
                count: 2,
                get: Rc::new(move |o| usize::from(!get(o))),
                set: Rc::new(move |o, i| set(o, i == 0)),
            },
            Rc::new(move |o, cx| generic_key(cx, caption, on_off(label_get(o)))),
        )
    }

    /// `OptionInstance.Enum`: "Caption: Value" with each value's own translation key.
    pub fn cycle<T: Copy + PartialEq + 'static>(
        caption: &'static str,
        values: &'static [T],
        get: fn(&Options) -> T,
        set: fn(&mut Options, T),
        name: fn(T) -> &'static str,
    ) -> Self {
        Self::new(
            Kind::Cycle {
                count: values.len(),
                get: Rc::new(move |o| values.iter().position(|v| *v == get(o)).unwrap_or(0)),
                set: Rc::new(move |o, i| set(o, values[i.min(values.len() - 1)])),
            },
            Rc::new(move |o, cx| generic_key(cx, caption, name(get(o)))),
        )
    }

    /// A cycle over values known only at runtime, labelled by `label`.
    pub fn cycle_with(
        count: usize,
        get: impl Fn(&Options) -> usize + 'static,
        set: impl Fn(&mut Options, usize) + 'static,
        label: impl Fn(&Options, &mut RenderCx<'_, '_>) -> String + 'static,
    ) -> Self {
        Self::new(
            Kind::Cycle {
                count: count.max(1),
                get: Rc::new(get),
                set: Rc::new(set),
            },
            Rc::new(label),
        )
    }

    pub fn slider(
        get: impl Fn(&Options) -> f64 + 'static,
        set: impl Fn(&mut Options, f64) + 'static,
        label: impl Fn(&Options, &mut RenderCx<'_, '_>) -> String + 'static,
    ) -> Self {
        Self::new(
            Kind::Slider {
                get: Rc::new(get),
                set: Rc::new(set),
                deferred: false,
            },
            Rc::new(label),
        )
    }

    /// `OptionInstance.UnitDouble`: the stored value is the slider position.
    pub fn unit(
        get: fn(&Options) -> f64,
        set: fn(&mut Options, f64),
        label: impl Fn(&mut RenderCx<'_, '_>, f64) -> String + 'static,
    ) -> Self {
        Self::slider(get, set, move |o, cx| label(cx, get(o)))
    }

    /// `OptionInstance.IntRange`: whole steps from `lo` to `hi`.
    pub fn int(
        (lo, hi): (i32, i32),
        get: impl Fn(&Options) -> i32 + 'static,
        set: impl Fn(&mut Options, i32) + 'static,
        label: impl Fn(&mut RenderCx<'_, '_>, i32) -> String + 'static,
    ) -> Self {
        let span = f64::from((hi - lo).max(1));
        let get: Get<i32> = Rc::new(get);
        let (g1, g2) = (get.clone(), get);
        Self::slider(
            move |o| f64::from(g1(o).clamp(lo, hi) - lo) / span,
            move |o, v| set(o, lo + (v.clamp(0.0, 1.0) * span).round() as i32),
            move |o, cx| label(cx, g2(o)),
        )
    }

    /// Runs `after` whenever the player's change actually alters the options.
    pub fn on_set(mut self, after: impl Fn(&mut Options) + 'static) -> Self {
        let after = Rc::new(after);
        match &mut self.kind {
            Kind::Cycle { set, .. } => {
                let inner = set.clone();
                *set = Rc::new(move |o, v| {
                    let before = o.clone();
                    inner(o, v);
                    if *o != before {
                        after(o);
                    }
                });
            }
            Kind::Slider { set, .. } => {
                let inner = set.clone();
                *set = Rc::new(move |o, v| {
                    let before = o.clone();
                    inner(o, v);
                    if *o != before {
                        after(o);
                    }
                });
            }
            Kind::Button(_) => {}
        }
        self
    }

    pub fn deferred(mut self) -> Self {
        if let Kind::Slider { deferred, .. } = &mut self.kind {
            *deferred = true;
        }
        self
    }

    pub fn tooltip(self, key: &'static str) -> Self {
        self.tooltip_with(move |_, cx| cx.lang(key))
    }

    pub fn tooltip_with(
        mut self,
        f: impl Fn(&Options, &mut RenderCx<'_, '_>) -> String + 'static,
    ) -> Self {
        self.tooltip = Some(Rc::new(f));
        self
    }

    pub fn active_if(mut self, f: impl Fn(&Options) -> bool + 'static) -> Self {
        self.active = Rc::new(f);
        self
    }

    pub fn inactive(self) -> Self {
        self.active_if(|_| false)
    }
}

pub enum Row {
    Header(&'static str),
    Big(Opt),
    Small(Opt, Option<Opt>),
}

/// `OptionsList#addSmall`: two per row.
pub fn small(opts: Vec<Opt>) -> Vec<Row> {
    let mut out = Vec::new();
    let mut it = opts.into_iter();
    while let Some(a) = it.next() {
        out.push(Row::Small(a, it.next()));
    }
    out
}

pub enum FooterButton {
    Done,
    Custom(&'static str, Action),
}

struct Placed {
    opt: Opt,
    /// Offset from the top of the list content.
    y: i32,
    x: i32,
    width: i32,
}

pub struct OptionsPage {
    title: &'static str,
    settings: SharedSettings,
    placed: Vec<Placed>,
    headers: Vec<(&'static str, i32)>,
    content: i32,
    footer: Vec<FooterButton>,
    widgets: WidgetSet,
    footer_widgets: WidgetSet,
    list: ScrollList,
    gui: (i32, i32),
    pending: Option<(usize, f64)>,
}

impl std::fmt::Debug for OptionsPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OptionsPage")
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl OptionsPage {
    pub fn new(title: &'static str, settings: SharedSettings, rows: Vec<Row>) -> Self {
        let mut placed = Vec::new();
        let mut headers = Vec::new();
        let mut y = 0;
        for row in rows {
            match row {
                Row::Header(text) => {
                    let pad = if y == 0 && placed.is_empty() {
                        0
                    } else {
                        LINE * 2
                    };
                    headers.push((text, y + pad));
                    y += pad + LINE * 2;
                }
                Row::Big(opt) => {
                    placed.push(Placed {
                        opt,
                        y,
                        x: 0,
                        width: BIG,
                    });
                    y += ROW;
                }
                Row::Small(a, b) => {
                    placed.push(Placed {
                        opt: a,
                        y,
                        x: 0,
                        width: SMALL,
                    });
                    if let Some(b) = b {
                        placed.push(Placed {
                            opt: b,
                            y,
                            x: COLUMN,
                            width: SMALL,
                        });
                    }
                    y += ROW;
                }
            }
        }
        Self {
            title,
            settings,
            placed,
            headers,
            content: y,
            footer: vec![FooterButton::Done],
            widgets: WidgetSet::default(),
            footer_widgets: WidgetSet::default(),
            list: ScrollList::new([0, HEADER, 0, 0], 1),
            gui: (0, 0),
            pending: None,
        }
    }

    #[cfg(test)]
    pub fn option_count(&self) -> usize {
        self.placed.len()
    }

    /// Clicks option `index` the way a left click on its button would.
    #[cfg(test)]
    pub fn click(&mut self, index: usize) -> Vec<ScreenAction> {
        let mut out = Vec::new();
        self.pressed(index as u32, false, &mut out);
        out
    }

    pub fn with_footer(mut self, footer: Vec<FooterButton>) -> Self {
        self.footer = footer;
        self
    }

    fn options(&self) -> Options {
        let mut options = self.settings.borrow().file.options.clone();
        if let Some((i, v)) = self.pending
            && let Kind::Slider { set, .. } = &self.placed[i].opt.kind
        {
            set(&mut options, v);
        }
        options
    }

    fn bar_x(&self) -> i32 {
        self.gui.0 / 2 + BIG / 2 + 8
    }

    fn place(&mut self) {
        let [_, top, _, height] = self.list.rect();
        let left = self.gui.0 / 2 - BIG / 2;
        let options = self.options();
        for (i, p) in self.placed.iter().enumerate() {
            let y = self.list.item_top(p.y.max(0) as usize);
            let rect = [left + p.x, y, p.width, BUTTON_HEIGHT];
            let visible = y + BUTTON_HEIGHT > top && y < top + height;
            let active = (p.opt.active)(&options);
            if let Some(w) = self.widgets.find_mut(i as u32) {
                w.set_rect(rect);
                w.set_visible(visible);
                w.set_active(active);
            }
        }
    }

    fn commit(&mut self) {
        let Some((i, v)) = self.pending.take() else {
            return;
        };
        if let Kind::Slider { set, .. } = &self.placed[i].opt.kind {
            let set = set.clone();
            self.settings.borrow_mut().edit(|f| set(&mut f.options, v));
        }
    }

    fn close(&mut self, out: &mut Vec<ScreenAction>) {
        self.commit();
        out.push(ScreenAction::Pop);
    }

    fn pressed(&mut self, id: u32, back: bool, out: &mut Vec<ScreenAction>) {
        let Some(p) = self.placed.get(id as usize) else {
            return;
        };
        match &p.opt.kind {
            Kind::Cycle { count, get, set } => {
                let (count, set) = (*count, set.clone());
                let current = get(&self.settings.borrow().file.options);
                let next = if back {
                    (current + count - 1) % count
                } else {
                    (current + 1) % count
                };
                self.settings
                    .borrow_mut()
                    .edit(|f| set(&mut f.options, next));
                out.push(ScreenAction::ClickSound);
            }
            Kind::Button(action) => {
                out.push(ScreenAction::ClickSound);
                let options = self.settings.borrow().file.options.clone();
                out.extend(action(&options));
            }
            Kind::Slider { .. } => {}
        }
    }

    fn slid(&mut self, id: u32, value: f32) {
        let Some(p) = self.placed.get(id as usize) else {
            return;
        };
        let Kind::Slider { get, set, deferred } = &p.opt.kind else {
            return;
        };
        let (get, set) = (get.clone(), set.clone());
        let value = f64::from(value);
        let snapped = if *deferred {
            let mut o = self.settings.borrow().file.options.clone();
            set(&mut o, value);
            self.pending = Some((id as usize, value));
            get(&o)
        } else {
            let mut s = self.settings.borrow_mut();
            s.edit(|f| set(&mut f.options, value));
            get(&s.file.options)
        };
        if let Some(w) = self.widgets.slider_mut(id) {
            w.set_value(snapped as f32);
        }
    }

    fn draw_frame(&self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = self.gui;
        draw_menu_background(cx);
        tile_texture(
            cx,
            "menu_list_background",
            [0, HEADER, w, (h - HEADER - FOOTER).max(0)],
        );
        if let Some(id) = cx.assets.texture("header_separator") {
            for x in (0..w).step_by(32) {
                cx.frame.sprite(id, x, HEADER - 2, 32, 2);
            }
        }
        if let Some(id) = cx.assets.texture("footer_separator") {
            for x in (0..w).step_by(32) {
                cx.frame.sprite(id, x, h - FOOTER, 32, 2);
            }
        }
    }
}

impl Screen for OptionsPage {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.list
            .set_rect([0, HEADER, width, (height - HEADER - FOOTER).max(0)]);
        self.list.set_count(self.content.max(0) as usize);
        self.widgets.clear();
        let options = self.options();
        for (i, p) in self.placed.iter().enumerate() {
            let rect = [0, 0, p.width, BUTTON_HEIGHT];
            match &p.opt.kind {
                Kind::Slider { get, .. } => {
                    self.widgets
                        .add(Slider::new(i as u32, rect, "", get(&options) as f32));
                }
                Kind::Cycle { .. } | Kind::Button(_) => {
                    self.widgets.add(Button::new(i as u32, rect, ""));
                }
            }
        }
        self.place();
        self.footer_widgets.clear();
        let n = self.footer.len() as i32;
        let width = if n == 1 { 200 } else { 150 };
        let total = n * width + (n - 1) * 8;
        for (i, b) in self.footer.iter().enumerate() {
            let label = match b {
                FooterButton::Done => "lang:gui.done".to_owned(),
                FooterButton::Custom(key, _) => format!("lang:{key}"),
            };
            let x = self.gui.0 / 2 - total / 2 + i as i32 * (width + 8);
            self.footer_widgets.add(Button::new(
                FOOTER_ID + i as u32,
                [
                    x,
                    height - FOOTER + (FOOTER - BUTTON_HEIGHT) / 2,
                    width,
                    BUTTON_HEIGHT,
                ],
                label,
            ));
        }
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.place();
        self.draw_frame(cx);
        let title = cx.lang(self.title);
        cx.centered_text(&title, self.gui.0 / 2, (HEADER - LINE) / 2 + 1, WHITE, true);
        let options = self.options();
        for (i, p) in self.placed.iter().enumerate() {
            let label = (p.opt.label)(&options, cx);
            match self.widgets.find_mut(i as u32) {
                Some(mcr_ui::Widget::Button(b)) => b.label = label,
                Some(mcr_ui::Widget::Slider(s)) => s.label = label,
                _ => {}
            }
        }
        let [lx, ly, lw, lh] = self.list.rect();
        cx.frame.push_scissor(lx, ly, lx + lw, ly + lh);
        for &(key, y) in &self.headers {
            let text = format!("§n§l{}", cx.lang(key));
            let top = self.list.item_top(y.max(0) as usize);
            cx.centered_text(&text, self.gui.0 / 2, top, WHITE, true);
        }
        self.widgets.render(cx);
        cx.frame.pop_scissor();
        self.list.render_scrollbar(cx, self.bar_x());
        self.footer_widgets.render(cx);
        let (mx, my) = cx.mouse;
        if contains(self.list.rect(), mx, my)
            && let Some(i) = (0..self.placed.len()).find(|&i| {
                self.widgets
                    .find(i as u32)
                    .is_some_and(|w| w.visible() && contains(w.rect(), mx, my))
            })
            && let Some(tip) = &self.placed[i].opt.tooltip
        {
            let text = tip(&options, cx);
            if !text.is_empty() {
                let lines = TooltipLines::wrapped(cx, &text);
                cx.set_tooltip(lines);
            }
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            self.close(out);
            return true;
        }
        if self.list.input(input, self.bar_x()) {
            self.place();
            return true;
        }
        let mut events = Vec::new();
        let in_list = match *input {
            UiInput::MousePress { x, y, .. } => contains(self.list.rect(), x, y),
            _ => true,
        };
        let mut used = false;
        if in_list {
            used |= self.widgets.input(env, input, &mut events);
        }
        used |= self.footer_widgets.input(env, input, &mut events);
        let back = matches!(input, UiInput::MousePress { mods, .. } if mods.shift);
        for e in events {
            if e.id >= FOOTER_ID {
                if e.kind != WidgetEventKind::Pressed {
                    continue;
                }
                out.push(ScreenAction::ClickSound);
                match self.footer.get((e.id - FOOTER_ID) as usize) {
                    Some(FooterButton::Done) => self.close(out),
                    Some(FooterButton::Custom(_, action)) => {
                        let action = action.clone();
                        let options = self.settings.borrow().file.options.clone();
                        out.extend(action(&options));
                    }
                    None => {}
                }
                continue;
            }
            match e.kind {
                WidgetEventKind::Pressed => self.pressed(e.id, back, out),
                WidgetEventKind::SliderChanged(v) => self.slid(e.id, v),
                _ => {}
            }
        }
        if matches!(
            input,
            UiInput::MouseRelease {
                button: MouseButton::Left,
                ..
            }
        ) {
            self.commit();
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn page(name: &str, rows: Vec<Row>) -> OptionsPage {
        let dir = std::env::temp_dir().join(format!("mcr-options-{name}-{}", std::process::id()));
        OptionsPage::new("options.videoTitle", Settings::load(&dir), rows)
    }

    #[test]
    fn rows_pair_up_and_headers_add_space() {
        let mut rows = vec![Row::Header("options.video.display.header")];
        rows.extend(small(vec![
            Opt::toggle(
                "options.vsync",
                |o| o.enable_vsync,
                |o, v| o.enable_vsync = v,
            ),
            Opt::toggle("options.viewBobbing", |o| o.bob_view, |o, v| o.bob_view = v),
            Opt::toggle(
                "options.entityShadows",
                |o| o.entity_shadows,
                |o, v| o.entity_shadows = v,
            ),
        ]));
        rows.push(Row::Header("options.video.quality.header"));
        rows.push(Row::Big(Opt::int(
            (2, 32),
            |o| o.render_distance,
            |o, v| o.render_distance = v,
            |_, v| v.to_string(),
        )));
        let p = page("rows", rows);
        let ys: Vec<(i32, i32)> = p.placed.iter().map(|p| (p.y, p.x)).collect();
        assert_eq!(ys, vec![(18, 0), (18, 160), (43, 0), (104, 0)]);
        assert_eq!(
            p.headers,
            vec![
                ("options.video.display.header", 0),
                ("options.video.quality.header", 86)
            ]
        );
        assert_eq!(p.content, 129);
    }

    #[test]
    fn cycles_step_and_sliders_snap() {
        let mut p = page(
            "cycles",
            small(vec![
                Opt::toggle(
                    "options.vsync",
                    |o| o.enable_vsync,
                    |o, v| o.enable_vsync = v,
                ),
                Opt::int(
                    (2, 32),
                    |o| o.render_distance,
                    |o, v| o.render_distance = v,
                    |_, v| v.to_string(),
                ),
            ]),
        );
        p.init(400, 300);
        let mut out = Vec::new();
        let before = p.settings.borrow().file.options.enable_vsync;
        p.pressed(0, false, &mut out);
        assert_eq!(p.settings.borrow().file.options.enable_vsync, !before);
        p.slid(1, 0.5);
        assert_eq!(p.settings.borrow().file.options.render_distance, 17);
    }

    #[test]
    fn deferred_sliders_apply_on_close() {
        let mut p = page(
            "deferred",
            vec![Row::Big(
                Opt::int(
                    (2, 32),
                    |o| o.render_distance,
                    |o, v| o.render_distance = v,
                    |_, v| v.to_string(),
                )
                .deferred(),
            )],
        );
        p.init(400, 300);
        let start = p.settings.borrow().file.options.render_distance;
        p.slid(0, 0.0);
        assert_eq!(p.settings.borrow().file.options.render_distance, start);
        assert_eq!(p.options().render_distance, 2);
        let mut out = Vec::new();
        p.close(&mut out);
        assert_eq!(p.settings.borrow().file.options.render_distance, 2);
    }
}
