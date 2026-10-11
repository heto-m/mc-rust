use std::ops::Range;

use mcr_nbt::{Compound, Value};
use mcr_protocol::types::item::ItemStack;
use mcr_protocol::types::text::{ClickEvent, Component};

use super::clicks::click_event_actions;
use super::dialog::{
    Action, ActionButton, AfterAction, Body, Control, Dialog, DialogKind, escape_without_quotes,
    instantiate,
};
use super::draw_dim_background;
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, MouseButton, UiInput, contains};
use super::menus::ConfirmScreen;
use super::multiline::MultiLineEdit;
use super::screen::{Screen, ScreenAction};
use super::tooltip::TooltipLines;
use super::widgets::{Button, CycleButton, Slider, WidgetEvent, WidgetEventKind, WidgetSet};
use crate::style::Cell;
use crate::text::{ClickSpan, click_at, flatten_events};

const WHITE: u32 = 0xFFFF_FFFF;
const HEADER: i32 = 33;
const FOOTER: i32 = 33;
const SPACING: i32 = 10;
const SCROLL_STEP: i32 = 12;
const CHECKBOX: i32 = 17;

const WARNING: u32 = 1;
const BODY_ID: u32 = 1000;
const FOOTER_ID: u32 = 2000;

enum Content {
    Text {
        cells: Vec<Cell>,
        spans: Vec<ClickSpan>,
        lines: Vec<Range<usize>>,
        ends: Vec<Vec<f32>>,
        width: i32,
    },
    Item {
        stack: Option<ItemStack>,
        decorations: bool,
        tooltip: bool,
    },
    Label(String),
    Widget(u32),
    Multi(usize),
    Check(usize),
}

struct Placed {
    dx: i32,
    dy: i32,
    w: i32,
    h: i32,
    content: Content,
}

struct Row {
    w: i32,
    h: i32,
    parts: Vec<Placed>,
}

struct Multi {
    edit: MultiLineEdit,
}

struct Check {
    label: String,
    on: bool,
    on_true: String,
    on_false: String,
    rect: [i32; 4],
}

enum Getter {
    Text(u32),
    Multi(usize),
    Check(usize),
    Option(u32, Vec<String>),
    Number(u32, NumberSpec),
}

#[derive(Clone, Copy)]
struct NumberSpec {
    start: f32,
    end: f32,
    initial: Option<f32>,
    step: Option<f32>,
}

impl NumberSpec {
    fn initial_scaled(self) -> f32 {
        self.initial.unwrap_or((self.start + self.end) / 2.0)
    }

    fn to_slider(self, value: f32) -> f32 {
        if self.start == self.end {
            0.5
        } else {
            (value - self.start) / (self.end - self.start)
        }
    }

    fn initial_slider(self) -> f32 {
        self.to_slider(self.initial_scaled())
    }

    fn scaled(self, slider: f32) -> f32 {
        let in_range = self.start + (self.end - self.start) * slider;
        let Some(step) = self.step else {
            return in_range;
        };
        let initial = self.initial_scaled();
        let steps = ((in_range - initial) / step).round();
        if !steps.is_finite() {
            return in_range;
        }
        let result = initial + steps * step;
        let pos = self.to_slider(result);
        if (0.0..=1.0).contains(&pos) {
            return result;
        }
        initial + (steps - steps.signum()) * step
    }
}

fn number_text(v: f32) -> String {
    if v.fract() == 0.0 && v.abs() < 1.0e9 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A server-defined dialog: body text and items, input controls and action buttons.
pub struct DialogScreen {
    dialog: Dialog,
    links: Vec<(Component, String)>,
    gui: (i32, i32),
    body: WidgetSet,
    footer: WidgetSet,
    actions: Vec<(u32, Option<Action>)>,
    getters: Vec<(String, Getter)>,
    multis: Vec<Multi>,
    checks: Vec<Check>,
    numbers: Vec<(u32, String, String, NumberSpec)>,
    labels: Vec<(u32, Component)>,
    focus_multi: Option<usize>,
    rows: Vec<Row>,
    built: bool,
    scroll: i32,
    max_scroll: i32,
    footer_height: i32,
    title_rect: [i32; 4],
    warning: [i32; 4],
}

impl std::fmt::Debug for DialogScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogScreen")
            .field("title", &self.dialog.title.to_plain_string())
            .finish_non_exhaustive()
    }
}

impl DialogScreen {
    pub fn new(dialog: Dialog, links: Vec<(Component, String)>) -> Self {
        Self {
            dialog,
            links,
            gui: (0, 0),
            body: WidgetSet::default(),
            footer: WidgetSet::default(),
            actions: Vec::new(),
            getters: Vec::new(),
            multis: Vec::new(),
            checks: Vec::new(),
            numbers: Vec::new(),
            labels: Vec::new(),
            focus_multi: None,
            rows: Vec::new(),
            built: false,
            scroll: 0,
            max_scroll: 0,
            footer_height: FOOTER,
            title_rect: [0; 4],
            warning: [0; 4],
        }
    }

    pub fn dialog(&self) -> &Dialog {
        &self.dialog
    }

    fn footer_buttons(&self) -> Vec<&ActionButton> {
        match &self.dialog.kind {
            DialogKind::Notice { action } => vec![action],
            DialogKind::Confirmation { yes, no } => vec![yes, no],
            DialogKind::MultiAction { exit, .. }
            | DialogKind::DialogList { exit, .. }
            | DialogKind::ServerLinks { exit, .. } => exit.iter().collect(),
        }
    }

    fn list_buttons(&self) -> (Vec<ActionButton>, usize) {
        match &self.dialog.kind {
            DialogKind::MultiAction {
                actions, columns, ..
            } => (actions.clone(), *columns),
            DialogKind::DialogList {
                dialogs,
                columns,
                button_width,
                ..
            } => (
                dialogs
                    .iter()
                    .filter_map(|c| {
                        let sub = Dialog::from_nbt(&Value::Compound(c.clone()))?;
                        Some(ActionButton {
                            label: sub.list_title().clone(),
                            tooltip: None,
                            width: *button_width,
                            action: Some(Action::Static(ClickEvent::ShowDialog(Value::Compound(
                                c.clone(),
                            )))),
                        })
                    })
                    .collect(),
                *columns,
            ),
            DialogKind::ServerLinks {
                columns,
                button_width,
                ..
            } => (
                self.links
                    .iter()
                    .map(|(label, url)| ActionButton {
                        label: label.clone(),
                        tooltip: None,
                        width: *button_width,
                        action: Some(Action::Static(ClickEvent::OpenUrl(url.clone()))),
                    })
                    .collect(),
                *columns,
            ),
            DialogKind::Notice { .. } | DialogKind::Confirmation { .. } => (Vec::new(), 1),
        }
    }

    fn add_button(
        widgets: &mut WidgetSet,
        actions: &mut Vec<(u32, Option<Action>)>,
        id: u32,
        b: &ActionButton,
    ) {
        let mut button = Button::new(id, [0, 0, b.width, 20], "").flat();
        if let Some(tip) = &b.tooltip {
            let mut lines = TooltipLines::default();
            lines.push_component(tip.clone());
            button.tooltip = Some(lines);
        }
        widgets.add(button);
        actions.push((id, b.action.clone()));
    }

    fn value_of(&mut self, getter: &Getter) -> Option<(String, Value)> {
        Some(match getter {
            Getter::Text(id) => {
                let text = self.body.edit(*id)?.value().to_owned();
                (escape_without_quotes(&text), Value::String(text))
            }
            Getter::Multi(i) => {
                let text = self.multis.get(*i)?.edit.value();
                (escape_without_quotes(&text), Value::String(text))
            }
            Getter::Check(i) => {
                let c = self.checks.get(*i)?;
                (
                    if c.on {
                        c.on_true.clone()
                    } else {
                        c.on_false.clone()
                    },
                    Value::Byte(i8::from(c.on)),
                )
            }
            Getter::Option(id, ids) => {
                let index = self.body.cycle_mut(*id)?.index();
                let id = ids.get(index)?.clone();
                (id.clone(), Value::String(id))
            }
            Getter::Number(id, spec) => {
                let v = spec.scaled(self.body.slider_mut(*id)?.value());
                (number_text(v), Value::Float(v))
            }
        })
    }

    fn collect_values(&mut self) -> Vec<(String, String, Value)> {
        let getters = std::mem::take(&mut self.getters);
        let values = getters
            .iter()
            .filter_map(|(key, g)| {
                let (text, tag) = self.value_of(g)?;
                Some((key.clone(), text, tag))
            })
            .collect();
        self.getters = getters;
        values
    }

    fn event_for(&mut self, action: &Action) -> Option<ClickEvent> {
        match action {
            Action::Static(event) => Some(event.clone()),
            Action::CommandTemplate(template) => {
                let values = self.collect_values();
                let command = instantiate(template, |name| {
                    values
                        .iter()
                        .find(|(k, _, _)| k == name)
                        .map(|(_, text, _)| text.clone())
                });
                Some(ClickEvent::RunCommand(command))
            }
            Action::Custom { id, additions } => {
                let mut payload: Compound = additions.clone().unwrap_or_default();
                for (key, _, tag) in self.collect_values() {
                    payload.insert(key, tag);
                }
                Some(ClickEvent::Custom {
                    id: id.clone(),
                    payload: Some(Value::Compound(payload)),
                })
            }
        }
    }

    /// Applies the after-action to the screen stack, then performs the click event.
    fn run(&mut self, event: Option<ClickEvent>, after: AfterAction, out: &mut Vec<ScreenAction>) {
        match after {
            AfterAction::None => {}
            AfterAction::Close => out.push(ScreenAction::Pop),
            AfterAction::WaitForResponse => out.push(ScreenAction::Replace(Box::new(
                WaitingForResponseScreen::default(),
            ))),
        }
        let Some(event) = event else { return };
        match event {
            ClickEvent::RunCommand(command) => out.push(ScreenAction::DialogCommand(
                command.strip_prefix('/').unwrap_or(&command).to_owned(),
            )),
            ClickEvent::ShowDialog(dialog) => out.push(ScreenAction::ShowDialog(dialog)),
            ClickEvent::Custom { id, payload } => {
                out.push(ScreenAction::CustomClick { id, payload });
            }
            other => click_event_actions(other, out),
        }
    }

    fn press(&mut self, id: u32, out: &mut Vec<ScreenAction>) {
        let Some(action) = self
            .actions
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, a)| a.clone())
        else {
            return;
        };
        let event = action.and_then(|a| self.event_for(&a));
        let after = self.dialog.after_action;
        self.run(event, after, out);
    }

    fn cancel(&mut self, out: &mut Vec<ScreenAction>) {
        if !self.dialog.can_close_with_escape {
            return;
        }
        let event = self
            .dialog
            .on_cancel()
            .cloned()
            .and_then(|a| self.event_for(&a));
        self.run(event, AfterAction::Close, out);
    }

    fn build(&mut self, cx: &mut RenderCx<'_, '_>) {
        self.built = true;
        self.rows.clear();
        let mut rows = Vec::new();
        for body in self.dialog.body.clone() {
            rows.push(self.body_row(cx, &body));
        }
        let inputs = self.dialog.inputs.clone();
        for (n, input) in inputs.iter().enumerate() {
            rows.push(self.input_row(cx, n, &input.key, &input.control));
        }
        let (list, columns) = self.list_buttons();
        if !list.is_empty() {
            rows.push(self.button_grid(&list, columns));
        }
        self.rows = rows;
    }

    fn text_content(
        cx: &mut RenderCx<'_, '_>,
        component: &Component,
        max_width: i32,
    ) -> (Content, i32, i32) {
        let (cells, spans) = flatten_events(cx.frame.ui().lang(), component);
        let lines = cx.frame.ui().wrap(&cells, max_width as f32);
        let widths: Vec<Vec<f32>> = lines
            .iter()
            .map(|range| {
                let mut x = 0.0;
                cells[range.clone()]
                    .iter()
                    .map(|c| {
                        x += cx.frame.ui().width(std::slice::from_ref(c));
                        x
                    })
                    .collect()
            })
            .collect();
        let width = widths
            .iter()
            .filter_map(|w| w.last().copied())
            .fold(0.0f32, f32::max)
            .ceil() as i32;
        let height = lines.len().clamp(1, 64) as i32 * 9;
        (
            Content::Text {
                cells,
                spans,
                lines,
                ends: widths,
                width,
            },
            width.max(1),
            height,
        )
    }

    fn body_row(&mut self, cx: &mut RenderCx<'_, '_>, body: &Body) -> Row {
        match body {
            Body::Text { contents, width } => {
                let (content, w, h) = Self::text_content(cx, contents, *width);
                Row {
                    w,
                    h,
                    parts: vec![Placed {
                        dx: 0,
                        dy: 0,
                        w,
                        h,
                        content,
                    }],
                }
            }
            Body::Item {
                item,
                count,
                description,
                show_decorations,
                show_tooltip,
                width,
                height,
            } => {
                let stack = cx.view.item_stack(item, *count);
                let mut parts = vec![Placed {
                    dx: 0,
                    dy: 0,
                    w: *width,
                    h: *height,
                    content: Content::Item {
                        stack,
                        decorations: *show_decorations,
                        tooltip: *show_tooltip,
                    },
                }];
                let (mut w, mut h) = (*width, *height);
                if let Some((text, max)) = description {
                    let (content, tw, th) = Self::text_content(cx, text, *max);
                    h = h.max(th);
                    parts[0].dy = (h - *height) / 2;
                    parts.push(Placed {
                        dx: w + 2,
                        dy: (h - th) / 2,
                        w: tw,
                        h: th,
                        content,
                    });
                    w += 2 + tw;
                }
                Row { w, h, parts }
            }
        }
    }

    fn input_row(
        &mut self,
        cx: &mut RenderCx<'_, '_>,
        n: usize,
        key: &str,
        control: &Control,
    ) -> Row {
        let id = BODY_ID + n as u32;
        let label_part = |cx: &mut RenderCx<'_, '_>, label: &Component| {
            let text = label.to_plain_string();
            let w = cx.text_width(&text);
            (text, w)
        };
        match control {
            Control::Text {
                width,
                label,
                label_visible,
                initial,
                max_length,
                multiline,
            } => {
                let (control_h, content) = match multiline {
                    None => {
                        let mut edit =
                            EditBox::new(id, [0, 0, *width, 20]).with_max_length(*max_length);
                        edit.set_value(initial);
                        edit.take_changed();
                        self.body.add(edit);
                        self.getters.push((key.to_owned(), Getter::Text(id)));
                        (20, Content::Widget(id))
                    }
                    Some(m) => {
                        let h = m
                            .height
                            .unwrap_or_else(|| (9 * m.max_lines.unwrap_or(4) as i32 + 8).min(512));
                        let mut edit = MultiLineEdit::new(id, [0, 0, *width, h])
                            .with_limits(*max_length, m.max_lines);
                        edit.set_value(initial);
                        edit.take_changed();
                        self.multis.push(Multi { edit });
                        let index = self.multis.len() - 1;
                        self.getters.push((key.to_owned(), Getter::Multi(index)));
                        (h, Content::Multi(index))
                    }
                };
                let mut parts = Vec::new();
                let mut dy = 0;
                let mut w = *width;
                if *label_visible {
                    let (text, lw) = label_part(cx, label);
                    w = w.max(lw);
                    parts.push(Placed {
                        dx: 0,
                        dy: 0,
                        w: lw,
                        h: 9,
                        content: Content::Label(text),
                    });
                    dy = 13;
                }
                parts.push(Placed {
                    dx: 0,
                    dy,
                    w: *width,
                    h: control_h,
                    content,
                });
                Row {
                    w,
                    h: dy + control_h,
                    parts,
                }
            }
            Control::Boolean {
                label,
                initial,
                on_true,
                on_false,
            } => {
                let (text, lw) = label_part(cx, label);
                self.checks.push(Check {
                    label: text,
                    on: *initial,
                    on_true: on_true.clone(),
                    on_false: on_false.clone(),
                    rect: [0; 4],
                });
                let index = self.checks.len() - 1;
                self.getters.push((key.to_owned(), Getter::Check(index)));
                let w = CHECKBOX + 4 + lw;
                Row {
                    w,
                    h: CHECKBOX,
                    parts: vec![Placed {
                        dx: 0,
                        dy: 0,
                        w,
                        h: CHECKBOX,
                        content: Content::Check(index),
                    }],
                }
            }
            Control::SingleOption {
                width,
                options,
                label,
                label_visible,
            } => {
                let values = options
                    .iter()
                    .map(|o| match &o.display {
                        Some(d) => d.to_plain_string(),
                        None => o.id.clone(),
                    })
                    .collect();
                let initial = options.iter().position(|o| o.initial).unwrap_or(0);
                let name = if *label_visible {
                    label.to_plain_string()
                } else {
                    String::new()
                };
                self.body.add(CycleButton::new(
                    id,
                    [0, 0, *width, 20],
                    name,
                    values,
                    initial,
                ));
                self.getters.push((
                    key.to_owned(),
                    Getter::Option(id, options.iter().map(|o| o.id.clone()).collect()),
                ));
                Self::single(id, *width, 20)
            }
            Control::NumberRange {
                width,
                label,
                label_format,
                start,
                end,
                initial,
                step,
            } => {
                let spec = NumberSpec {
                    start: *start,
                    end: *end,
                    initial: *initial,
                    step: *step,
                };
                self.body.add(Slider::new(
                    id,
                    [0, 0, *width, 20],
                    "",
                    spec.initial_slider(),
                ));
                self.getters
                    .push((key.to_owned(), Getter::Number(id, spec)));
                self.numbers
                    .push((id, label.to_plain_string(), label_format.clone(), spec));
                Self::single(id, *width, 20)
            }
        }
    }

    fn single(id: u32, w: i32, h: i32) -> Row {
        Row {
            w,
            h,
            parts: vec![Placed {
                dx: 0,
                dy: 0,
                w,
                h,
                content: Content::Widget(id),
            }],
        }
    }

    fn button_grid(&mut self, buttons: &[ActionButton], columns: usize) -> Row {
        let columns = columns.clamp(1, super::dialog::MAX_COLUMNS);
        let mut rows: Vec<Vec<&ActionButton>> = Vec::new();
        let full = buttons.len() / columns * columns;
        for chunk in buttons[..full].chunks(columns) {
            rows.push(chunk.iter().collect());
        }
        if full < buttons.len() {
            rows.push(buttons[full..].iter().collect());
        }
        let col_w: Vec<i32> = (0..columns)
            .map(|c| {
                buttons[..full]
                    .iter()
                    .skip(c)
                    .step_by(columns)
                    .map(|b| b.width)
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let grid_w = |row: &[&ActionButton], full_row: bool| {
            if full_row {
                col_w.iter().sum::<i32>() + 2 * (columns as i32 - 1)
            } else {
                row.iter().map(|b| b.width).sum::<i32>() + 2 * (row.len() as i32 - 1).max(0)
            }
        };
        let total_w = rows
            .iter()
            .enumerate()
            .map(|(i, r)| grid_w(r, i * columns < full))
            .max()
            .unwrap_or(0);
        let mut parts = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let full_row = r * columns < full;
            let row_w = grid_w(row, full_row);
            let mut x = (total_w - row_w) / 2;
            for (c, button) in row.iter().enumerate() {
                let id = BODY_ID + 500 + (r * columns + c) as u32;
                Self::add_button(&mut self.body, &mut self.actions, id, button);
                self.labels.push((id, button.label.clone()));
                let cell_w = if full_row { col_w[c] } else { button.width };
                parts.push(Placed {
                    dx: x + (cell_w - button.width) / 2,
                    dy: r as i32 * 22,
                    w: button.width,
                    h: 20,
                    content: Content::Widget(id),
                });
                x += cell_w + 2;
            }
        }
        Row {
            w: total_w,
            h: rows.len() as i32 * 22 - 2,
            parts,
        }
    }
}

impl Screen for DialogScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.body.clear();
        self.footer.clear();
        self.actions.clear();
        self.getters.clear();
        self.multis.clear();
        self.checks.clear();
        self.numbers.clear();
        self.labels.clear();
        self.focus_multi = None;
        self.built = false;
        let footer: Vec<ActionButton> = self.footer_buttons().into_iter().cloned().collect();
        self.footer_height = if footer.is_empty() { 5 } else { FOOTER };
        for (n, b) in footer.iter().enumerate() {
            let id = FOOTER_ID + n as u32;
            Self::add_button(&mut self.footer, &mut self.actions, id, b);
            self.labels.push((id, b.label.clone()));
        }
        self.footer
            .add(Button::new(WARNING, [0, 0, 20, 20], "").flat());
    }

    fn pauses_game(&self) -> bool {
        self.dialog.pause
    }

    fn is_dialog(&self) -> bool {
        true
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        if !self.built {
            self.build(cx);
        }
        draw_dim_background(cx);
        self.layout(cx);
        self.draw_header(cx);
        let (w, h) = self.gui;
        let (top, bottom) = (HEADER, h - self.footer_height);
        cx.frame.push_scissor(0, top, w, bottom);
        self.draw_body(cx, top);
        cx.frame.pop_scissor();
        self.draw_scrollbar(cx, top, bottom);
        self.draw_buttons(cx);
        self.body.render(cx);
        self.footer.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        match *input {
            UiInput::Key { key, mods, .. } => {
                if let Some(i) = self.focus_multi {
                    if key == Key::Escape {
                        self.cancel(out);
                    } else if key == Key::Tab {
                        self.focus_multi = None;
                        self.body.navigate(!mods.shift, env.now_ms);
                    } else {
                        self.multis[i].edit.key(key, mods, env.clipboard);
                    }
                    return true;
                }
                if key == Key::Escape {
                    self.cancel(out);
                    return true;
                }
            }
            UiInput::Char(c) => {
                if let Some(i) = self.focus_multi {
                    self.multis[i].edit.char(c);
                    return true;
                }
            }
            UiInput::Scroll { dy, .. } if self.max_scroll > 0 => {
                self.scroll =
                    (self.scroll - (dy * SCROLL_STEP as f32) as i32).clamp(0, self.max_scroll);
                return true;
            }
            UiInput::MousePress {
                x, y, button, mods, ..
            } => {
                let (_, h) = self.gui;
                let in_body = y >= HEADER as f32 && y < (h - self.footer_height) as f32;
                if button == MouseButton::Left && contains(self.warning, x, y) {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Push(Box::new(warning_screen())));
                    return true;
                }
                if in_body {
                    self.footer.set_focus(None, env.now_ms);
                    if let Some(i) = self.multis.iter().position(|m| m.edit.contains(x, y)) {
                        self.body.set_focus(None, env.now_ms);
                        self.focus_multi = Some(i);
                        self.multis[i].edit.click(x, y, env.now_ms, mods.shift);
                        return true;
                    }
                    if let Some(i) = self.checks.iter().position(|c| contains(c.rect, x, y)) {
                        self.checks[i].on = !self.checks[i].on;
                        out.push(ScreenAction::ClickSound);
                        return true;
                    }
                    if let Some(event) = self.text_click(x, y) {
                        let after = self.dialog.after_action;
                        self.run(Some(event), after, out);
                        return true;
                    }
                    self.focus_multi = None;
                    return self.widgets_input(true, env, input, out);
                }
                self.focus_multi = None;
                self.body.set_focus(None, env.now_ms);
                return self.widgets_input(false, env, input, out);
            }
            UiInput::MouseMove { .. } | UiInput::MouseRelease { .. } => {
                self.widgets_input(true, env, input, out);
                self.widgets_input(false, env, input, out);
                return true;
            }
            _ => {}
        }
        let body = self.footer.focused().is_none();
        self.widgets_input(body, env, input, out);
        true
    }
}

impl DialogScreen {
    fn widgets_input(
        &mut self,
        body: bool,
        env: &mut Env<'_>,
        input: &UiInput,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        let mut events = Vec::new();
        let set = if body {
            &mut self.body
        } else {
            &mut self.footer
        };
        let used = set.input(env, input, &mut events);
        for WidgetEvent { id, kind } in events {
            match kind {
                WidgetEventKind::Pressed if id == WARNING => {
                    out.push(ScreenAction::ClickSound);
                    out.push(ScreenAction::Push(Box::new(warning_screen())));
                }
                WidgetEventKind::Pressed => {
                    out.push(ScreenAction::ClickSound);
                    self.press(id, out);
                }
                _ => {}
            }
        }
        used
    }

    fn text_click(&self, x: f32, y: f32) -> Option<ClickEvent> {
        for (row, base) in self.rows.iter().zip(self.row_tops()) {
            for part in &row.parts {
                let Content::Text {
                    spans, lines, ends, ..
                } = &part.content
                else {
                    continue;
                };
                let px = (self.gui.0 - row.w) / 2 + part.dx;
                let py = base + part.dy - self.scroll;
                let rel_y = y - py as f32;
                if rel_y < 0.0 || rel_y >= (lines.len() * 9) as f32 {
                    continue;
                }
                let line = (rel_y / 9.0) as usize;
                let line_w = ends[line].last().copied().unwrap_or(0.0);
                let rel_x = x - (px as f32 + (part.w as f32 - line_w) / 2.0);
                if rel_x < 0.0 {
                    continue;
                }
                let col = ends[line].iter().position(|&e| rel_x < e)?;
                return click_at(spans, lines[line].start + col).cloned();
            }
        }
        None
    }

    fn row_tops(&self) -> Vec<i32> {
        let total: i32 = self.rows.iter().map(|r| r.h).sum::<i32>()
            + SPACING * (self.rows.len() as i32 - 1).max(0);
        let area = self.gui.1 - HEADER - self.footer_height;
        let mut y = HEADER + if total < area { (area - total) / 2 } else { 0 };
        self.rows
            .iter()
            .map(|r| {
                let top = y;
                y += r.h + SPACING;
                top
            })
            .collect()
    }

    fn layout(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = self.gui;
        let total: i32 = self.rows.iter().map(|r| r.h).sum::<i32>()
            + SPACING * (self.rows.len() as i32 - 1).max(0);
        let area = h - HEADER - self.footer_height;
        self.max_scroll = (total - area).max(0);
        self.scroll = self.scroll.clamp(0, self.max_scroll);
        let tops = self.row_tops();
        for (row, base) in self.rows.iter().zip(tops) {
            let rx = (w - row.w) / 2;
            for part in &row.parts {
                let rect = [rx + part.dx, base + part.dy - self.scroll, part.w, part.h];
                match &part.content {
                    Content::Widget(id) => {
                        if let Some(widget) = self.body.find_mut(*id) {
                            widget.set_rect(rect);
                        }
                    }
                    Content::Multi(i) => self.multis[*i].edit.rect = rect,
                    Content::Check(i) => self.checks[*i].rect = rect,
                    _ => {}
                }
            }
        }
        let count = self.footer.len() as i32 - 1;
        let widths: Vec<i32> = (0..count)
            .filter_map(|n| self.footer.find(FOOTER_ID + n as u32).map(|w| w.rect()[2]))
            .collect();
        let total_w = widths.iter().sum::<i32>() + 8 * (count - 1).max(0);
        let mut x = (w - total_w) / 2;
        for (n, width) in widths.iter().enumerate() {
            if let Some(widget) = self.footer.find_mut(FOOTER_ID + n as u32) {
                widget.set_rect([
                    x,
                    h - self.footer_height + (self.footer_height - 20) / 2,
                    *width,
                    20,
                ]);
            }
            x += width + 8;
        }
        let title = self.dialog.title.to_plain_string();
        let title_w = cx.text_width(&title);
        let row_w = title_w + SPACING + 20;
        let left = (w - row_w) / 2;
        self.title_rect = [left, 6, title_w, 20];
        self.warning = [left + title_w + SPACING, 6, 20, 20];
        let warning = self.warning;
        if let Some(widget) = self.footer.find_mut(WARNING) {
            widget.set_rect(warning);
        }
        let numbers = self.numbers.clone();
        for (id, label, format, spec) in numbers {
            let value =
                number_text(spec.scaled(self.body.slider_mut(id).map_or(0.0, |s| s.value())));
            let text = cx.lang_fmt(&format, &[&label, &value]);
            if let Some(s) = self.body.slider_mut(id) {
                s.label = text;
            }
        }
    }

    fn draw_header(&mut self, cx: &mut RenderCx<'_, '_>) {
        let cells = cx.frame.ui().flatten(&self.dialog.title);
        let y = self.title_rect[1] + (20 - 9) / 2;
        cx.frame
            .cells(&cells, self.title_rect[0] as f32, y as f32, WHITE, true);
        let [x, wy, w, h] = self.warning;
        let hovered = contains(self.warning, cx.mouse.0, cx.mouse.1);
        let name = if hovered {
            "dialog/warning_button_highlighted"
        } else {
            "dialog/warning_button"
        };
        cx.sprite(name, x, wy, w, h);
        if hovered {
            let text = cx.lang("menu.custom_screen_info.tooltip");
            cx.set_tooltip(TooltipLines::text(text));
        }
    }

    fn draw_body(&mut self, cx: &mut RenderCx<'_, '_>, top: i32) {
        let _ = top;
        let (w, _) = self.gui;
        let tops = self.row_tops();
        let mut tooltip = None;
        for (row, base) in self.rows.iter().zip(tops) {
            let rx = (w - row.w) / 2;
            for part in &row.parts {
                let (x, y) = (rx + part.dx, base + part.dy - self.scroll);
                match &part.content {
                    Content::Text {
                        cells,
                        lines,
                        ends,
                        width,
                        ..
                    } => {
                        for (i, range) in lines.iter().enumerate().take(64) {
                            let lw = ends[i].last().copied().unwrap_or(0.0);
                            let lx = x as f32 + ((*width as f32) - lw) / 2.0;
                            cx.frame.cells(
                                &cells[range.clone()],
                                lx.floor(),
                                (y + i as i32 * 9) as f32,
                                WHITE,
                                true,
                            );
                        }
                    }
                    Content::Item {
                        stack,
                        decorations,
                        tooltip: show_tooltip,
                    } => {
                        if let Some(stack) = stack {
                            let (ix, iy) = (x + (part.w - 16) / 2, y + (part.h - 16) / 2);
                            if *decorations {
                                cx.stack(stack, ix, iy);
                            } else {
                                let mut bare = stack.clone();
                                bare.count = 1;
                                cx.stack(&bare, ix, iy);
                            }
                            if *show_tooltip
                                && contains([x, y, part.w, part.h], cx.mouse.0, cx.mouse.1)
                            {
                                tooltip = Some(cx.item_tooltip(stack));
                            }
                        }
                    }
                    Content::Label(text) => cx.text(text, x, y, WHITE, true),
                    Content::Multi(i) => {
                        let m = &mut self.multis[*i];
                        cx.sprite("widget/text_field", x, y, part.w, part.h);
                        let focused = self.focus_multi == Some(*i);
                        m.edit.render(cx, focused);
                    }
                    Content::Check(i) => {
                        let c = &self.checks[*i];
                        let hovered = contains(c.rect, cx.mouse.0, cx.mouse.1);
                        let name = match (c.on, hovered) {
                            (false, false) => "widget/checkbox",
                            (false, true) => "widget/checkbox_highlighted",
                            (true, false) => "widget/checkbox_selected",
                            (true, true) => "widget/checkbox_selected_highlighted",
                        };
                        cx.sprite(name, x, y, CHECKBOX, CHECKBOX);
                        cx.text(
                            &c.label,
                            x + CHECKBOX + 4,
                            y + (CHECKBOX - 8) / 2,
                            WHITE,
                            true,
                        );
                    }
                    Content::Widget(_) => {}
                }
            }
        }
        if let Some(lines) = tooltip {
            cx.set_tooltip(lines);
        }
    }

    fn draw_scrollbar(&self, cx: &mut RenderCx<'_, '_>, top: i32, bottom: i32) {
        if self.max_scroll <= 0 {
            return;
        }
        let area = bottom - top;
        let total = area + self.max_scroll;
        let bar = (area * area / total).clamp(32, area);
        let y = top + (area - bar) * self.scroll / self.max_scroll;
        let x = self.gui.0 - 8;
        cx.frame.fill_rect(x, top, x + 6, bottom, 0xFF00_0000);
        cx.frame.fill_rect(x, y, x + 6, y + bar, 0xFF80_8080);
        cx.frame.fill_rect(x, y, x + 5, y + bar - 1, 0xFFC0_C0C0);
    }

    fn draw_buttons(&mut self, cx: &mut RenderCx<'_, '_>) {
        let labels = self.labels.clone();
        let (_, h) = self.gui;
        let (top, bottom) = (HEADER, h - self.footer_height);
        for (id, label) in labels {
            let in_footer = id >= FOOTER_ID;
            let set = if in_footer { &self.footer } else { &self.body };
            let Some(widget) = set.find(id) else { continue };
            let rect = widget.rect();
            if !in_footer && (rect[1] + rect[3] <= top || rect[1] >= bottom) {
                continue;
            }
            let focused = set.focused() == Some(id);
            let hovered = contains(rect, cx.mouse.0, cx.mouse.1)
                && (in_footer || (cx.mouse.1 >= top as f32 && cx.mouse.1 < bottom as f32));
            let sprite = if hovered || focused {
                "widget/button_highlighted"
            } else {
                "widget/button"
            };
            cx.sprite(sprite, rect[0], rect[1], rect[2], rect[3]);
            let cells = cx.frame.ui().flatten(&label);
            let width = cx.frame.ui().width(&cells);
            let ty = rect[1] + (rect[3] - 8) / 2;
            if width > (rect[2] - 4) as f32 {
                cx.frame.push_scissor(
                    rect[0] + 2,
                    rect[1],
                    rect[0] + rect[2] - 2,
                    rect[1] + rect[3],
                );
                cx.frame
                    .cells(&cells, (rect[0] + 2) as f32, ty as f32, WHITE, true);
                cx.frame.pop_scissor();
            } else {
                let tx = rect[0] as f32 + (rect[2] as f32 - width) / 2.0;
                cx.frame.cells(&cells, tx.floor(), ty as f32, WHITE, true);
            }
        }
    }
}

fn warning_screen() -> ConfirmScreen {
    ConfirmScreen::new(
        "lang:menu.custom_screen_info.title",
        "lang:menu.custom_screen_info.contents",
        vec![
            (
                "lang:menu.disconnect".into(),
                Box::new(|| vec![ScreenAction::DialogDisconnect]),
            ),
            ("lang:gui.back".into(), Box::new(|| vec![ScreenAction::Pop])),
        ],
    )
}

/// Shown after a dialog action that waits for the server to answer.
#[derive(Debug, Default)]
pub struct WaitingForResponseScreen {
    gui: (i32, i32),
    ticks: u32,
    widgets: WidgetSet,
}

impl Screen for WaitingForResponseScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        self.widgets.clear();
        let y = HEADER + (height - HEADER - 20) / 2;
        let mut back = Button::new(1, [(width - 200) / 2, y, 200, 20], "lang:gui.back");
        back.visible = false;
        back.active = false;
        self.widgets.add(back);
    }

    fn is_dialog(&self) -> bool {
        true
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let title = cx.lang("gui.waitingForResponse.title");
        cx.centered_text(&title, self.gui.0 / 2, (HEADER - 9) / 2, WHITE, true);
        let seconds = self.ticks / 20;
        let label = if seconds < 5 {
            cx.lang_fmt(
                "gui.waitingForResponse.button.inactive",
                &[&(5 - seconds).to_string()],
            )
        } else {
            "lang:gui.back".into()
        };
        if let Some(b) = self.widgets.button_mut(1) {
            b.visible = seconds >= 1;
            b.active = seconds >= 5;
            b.label = label;
        }
        self.widgets.render(cx);
    }

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {
        self.ticks = self.ticks.saturating_add(1);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let active = self.ticks / 20 >= 5;
        if let UiInput::Key {
            key: Key::Escape, ..
        } = *input
        {
            if active {
                out.push(ScreenAction::Pop);
            }
            return true;
        }
        let mut events = Vec::new();
        self.widgets.input(env, input, &mut events);
        if events.iter().any(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            out.push(ScreenAction::Pop);
        }
        true
    }
}
