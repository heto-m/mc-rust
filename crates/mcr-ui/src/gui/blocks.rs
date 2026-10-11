use std::cell::RefCell;
use std::rc::Rc;

use mcr_buf::{BlockPos, Identifier};
use mcr_protocol::play::serverbound::{
    CommandBlockMode, JigsawGenerate, JointType, Mirror, Rotation, SetCommandBlock,
    SetCommandMinecart, SetJigsawBlock, SetStructureBlock, SetTestBlock, StructureMode,
    StructureUpdateType, TestBlockMode, TestInstanceAction, TestInstanceBlockAction,
    TestInstanceData, TestInstanceStatus, Vec3i,
};
use mcr_protocol::types::text::{Arg, Color, Component, NamedColor, Style};

use super::draw_dim_background;
use super::edit_box::EditBox;
use super::env::{Env, RenderCx};
use super::input::{Key, UiInput};
use super::screen::{EditorPacket, Screen, ScreenAction};
use super::widgets::{Button, CycleButton, Slider, WidgetEvent, WidgetEventKind, WidgetSet};

const WHITE: u32 = 0xFFFF_FFFF;
const LABEL: u32 = 0xFFA0_A0A0;
const EXTENT: i32 = 48;

fn label(cx: &mut RenderCx<'_, '_>, text: &str, x: i32, y: i32) {
    cx.text(text, x, y, LABEL, true);
}

fn label_key(cx: &mut RenderCx<'_, '_>, key: &str, x: i32, y: i32) {
    let text = cx.lang(key);
    label(cx, &text, x, y);
}

fn label_key_right(cx: &mut RenderCx<'_, '_>, key: &str, right: i32, y: i32) {
    let text = cx.lang(key);
    let w = cx.text_width(&text);
    label(cx, &text, right - w, y);
}

fn edit(id: u32, rect: [i32; 4], max: usize, text: &str) -> EditBox {
    let mut e = EditBox::new(id, rect).with_max_length(max);
    e.set_value(text);
    e.take_changed();
    e
}

fn set_edit(widgets: &mut WidgetSet, id: u32, text: &str) {
    if let Some(e) = widgets.edit_mut(id) {
        e.set_value(text);
        e.take_changed();
    }
}

fn edit_value(widgets: &WidgetSet, id: u32) -> String {
    widgets
        .edit(id)
        .map(|e| e.value().to_owned())
        .unwrap_or_default()
}

fn show(widgets: &mut WidgetSet, id: u32, visible: bool) {
    if let Some(w) = widgets.find_mut(id) {
        w.set_visible(visible);
    }
}

fn enable(widgets: &mut WidgetSet, id: u32, active: bool) {
    if let Some(w) = widgets.find_mut(id) {
        w.set_active(active);
    }
}

fn on_off(on: bool) -> usize {
    usize::from(!on)
}

fn on_off_button(id: u32, rect: [i32; 4], on: bool) -> CycleButton {
    CycleButton::new(
        id,
        rect,
        "",
        vec!["lang:options.on".into(), "lang:options.off".into()],
        on_off(on),
    )
}

fn parse_int(text: &str) -> i32 {
    text.trim().parse().unwrap_or(0)
}

/// A finished form on Enter in any field, Escape cancels, everything else goes to the widgets.
fn form_input(
    widgets: &mut WidgetSet,
    env: &mut Env<'_>,
    input: &UiInput,
) -> (Vec<WidgetEvent>, Option<Key>) {
    if let UiInput::Key {
        key: Key::Escape, ..
    } = *input
    {
        return (Vec::new(), Some(Key::Escape));
    }
    let mut events = Vec::new();
    widgets.input(env, input, &mut events);
    (events, None)
}

fn pressed(events: &[WidgetEvent], id: u32) -> bool {
    events
        .iter()
        .any(|e| e.id == id && e.kind == WidgetEventKind::Pressed)
}

fn submitted(events: &[WidgetEvent]) -> bool {
    events.iter().any(|e| e.kind == WidgetEventKind::Submitted)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandBlockInfo {
    pub pos: BlockPos,
    pub command: String,
    pub mode: CommandBlockMode,
    pub conditional: bool,
    pub automatic: bool,
    pub track_output: bool,
    pub last_output: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MinecartCommandInfo {
    pub entity_id: i32,
    pub command: String,
    pub track_output: bool,
    pub last_output: String,
}

enum CommandTarget {
    Block(CommandBlockInfo),
    Minecart(MinecartCommandInfo),
}

const CMD_COMMAND: u32 = 1;
const CMD_PREVIOUS: u32 = 2;
const CMD_OUTPUT: u32 = 3;
const CMD_MODE: u32 = 4;
const CMD_COND: u32 = 5;
const CMD_AUTO: u32 = 6;
const CMD_DONE: u32 = 7;
const CMD_CANCEL: u32 = 8;

/// Command block and command block minecart editor.
pub struct CommandBlockScreen {
    target: CommandTarget,
    last_output: String,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for CommandBlockScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandBlockScreen").finish_non_exhaustive()
    }
}

impl CommandBlockScreen {
    pub fn block(info: CommandBlockInfo) -> Self {
        Self::new(info.last_output.clone(), CommandTarget::Block(info))
    }

    pub fn minecart(info: MinecartCommandInfo) -> Self {
        Self::new(info.last_output.clone(), CommandTarget::Minecart(info))
    }

    fn new(last_output: String, target: CommandTarget) -> Self {
        Self {
            target,
            last_output,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn previous_y(&self) -> i32 {
        match self.target {
            CommandTarget::Block(_) => 135,
            CommandTarget::Minecart(_) => 150,
        }
    }

    fn track(&self) -> bool {
        match &self.target {
            CommandTarget::Block(b) => b.track_output,
            CommandTarget::Minecart(m) => m.track_output,
        }
    }

    fn previous_text(&self) -> String {
        if self.track() {
            self.last_output.clone()
        } else {
            "-".into()
        }
    }

    fn command_text(&self) -> String {
        match &self.target {
            CommandTarget::Block(b) => b.command.clone(),
            CommandTarget::Minecart(m) => m.command.clone(),
        }
    }

    fn sync_from_widgets(&mut self) {
        let command = edit_value(&self.widgets, CMD_COMMAND);
        let track = self.widgets.cycle_mut(CMD_OUTPUT).map(|c| c.index() == 0);
        let mode = self.widgets.cycle_mut(CMD_MODE).map(|c| c.index());
        let cond = self.widgets.cycle_mut(CMD_COND).map(|c| c.index() == 0);
        let auto = self.widgets.cycle_mut(CMD_AUTO).map(|c| c.index() == 0);
        match &mut self.target {
            CommandTarget::Block(b) => {
                b.command = command;
                b.track_output = track.unwrap_or(b.track_output);
                b.conditional = cond.unwrap_or(b.conditional);
                b.automatic = auto.unwrap_or(b.automatic);
                if let Some(mode) = mode {
                    b.mode = [
                        CommandBlockMode::Sequence,
                        CommandBlockMode::Auto,
                        CommandBlockMode::Redstone,
                    ][mode.min(2)];
                }
            }
            CommandTarget::Minecart(m) => {
                m.command = command;
                m.track_output = track.unwrap_or(m.track_output);
            }
        }
    }

    fn done(&mut self, out: &mut Vec<ScreenAction>) {
        self.sync_from_widgets();
        let packet = match &self.target {
            CommandTarget::Block(b) => EditorPacket::CommandBlock(SetCommandBlock {
                pos: b.pos,
                command: b.command.clone(),
                mode: b.mode,
                track_output: b.track_output,
                conditional: b.conditional,
                automatic: b.automatic,
            }),
            CommandTarget::Minecart(m) => EditorPacket::Minecart(SetCommandMinecart {
                entity_id: m.entity_id,
                command: m.command.clone(),
                track_output: m.track_output,
            }),
        };
        out.push(ScreenAction::Editor(packet));
        out.push(ScreenAction::Pop);
    }
}

impl Screen for CommandBlockScreen {
    fn init(&mut self, width: i32, height: i32) {
        if !self.widgets.is_empty() {
            self.sync_from_widgets();
        }
        self.gui = (width, height);
        let track = self.track();
        let py = self.previous_y();
        let command = self.command_text();
        let previous = self.previous_text();
        let w = &mut self.widgets;
        w.clear();
        w.add(edit(
            CMD_COMMAND,
            [width / 2 - 150, 50, 300, 20],
            32500,
            &command,
        ));
        let mut prev = edit(
            CMD_PREVIOUS,
            [width / 2 - 150, py, 276, 20],
            32500,
            &previous,
        );
        prev.editable = false;
        w.add(prev);
        w.add(CycleButton::new(
            CMD_OUTPUT,
            [width / 2 + 150 - 20, py, 20, 20],
            "",
            vec!["O".into(), "X".into()],
            on_off(track),
        ));
        if let CommandTarget::Block(b) = &self.target {
            w.add(CycleButton::new(
                CMD_MODE,
                [width / 2 - 50 - 100 - 4, 165, 100, 20],
                "",
                vec![
                    "lang:advMode.mode.sequence".into(),
                    "lang:advMode.mode.auto".into(),
                    "lang:advMode.mode.redstone".into(),
                ],
                match b.mode {
                    CommandBlockMode::Sequence => 0,
                    CommandBlockMode::Auto => 1,
                    CommandBlockMode::Redstone => 2,
                },
            ));
            w.add(CycleButton::new(
                CMD_COND,
                [width / 2 - 50, 165, 100, 20],
                "",
                vec![
                    "lang:advMode.mode.conditional".into(),
                    "lang:advMode.mode.unconditional".into(),
                ],
                on_off(b.conditional),
            ));
            w.add(CycleButton::new(
                CMD_AUTO,
                [width / 2 + 50 + 4, 165, 100, 20],
                "",
                vec![
                    "lang:advMode.mode.autoexec.bat".into(),
                    "lang:advMode.mode.redstoneTriggered".into(),
                ],
                on_off(b.automatic),
            ));
        }
        let y = height / 4 + 120 + 12;
        w.add(Button::new(
            CMD_DONE,
            [width / 2 - 4 - 150, y, 150, 20],
            "lang:gui.done",
        ));
        w.add(Button::new(
            CMD_CANCEL,
            [width / 2 + 4, y, 150, 20],
            "lang:gui.cancel",
        ));
        w.set_focus(Some(CMD_COMMAND), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let w = self.gui.0;
        let title = cx.lang("advMode.setCommand");
        cx.centered_text(&title, w / 2, 20, WHITE, true);
        label_key(cx, "advMode.command", w / 2 - 150 + 1, 40);
        let previous = self.previous_text();
        set_edit(&mut self.widgets, CMD_PREVIOUS, &previous);
        self.widgets.render(cx);
        if !previous.is_empty() {
            let y = 75 + 5 * 9 + 1 + self.previous_y() - 135;
            label_key(cx, "advMode.previousOutput", w / 2 - 150 + 1, y + 4);
        }
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let (events, key) = form_input(&mut self.widgets, env, input);
        if key.is_some() || pressed(&events, CMD_CANCEL) {
            out.push(ScreenAction::Pop);
        } else if pressed(&events, CMD_DONE) || submitted(&events) {
            self.done(out);
        }
        if events.iter().any(|e| e.id == CMD_OUTPUT) {
            let on = self
                .widgets
                .cycle_mut(CMD_OUTPUT)
                .is_some_and(|c| c.index() == 0);
            match &mut self.target {
                CommandTarget::Block(b) => b.track_output = on,
                CommandTarget::Minecart(m) => m.track_output = on,
            }
        }
        if pressed(&events, CMD_DONE) || pressed(&events, CMD_CANCEL) {
            out.push(ScreenAction::ClickSound);
        }
        true
    }
}

pub fn valid_identifier(text: &str) -> bool {
    Identifier::parse(text).is_ok()
}

fn name_char_ok(text: &str, c: char, cursor: usize) -> bool {
    let colon = text.chars().position(|ch| ch == ':');
    if c == ':' {
        return colon.is_none();
    }
    let in_path = colon.is_some_and(|i| cursor > i);
    matches!(c, 'a'..='z' | '0'..='9' | '_' | '-' | '.') || (in_path || colon.is_none()) && c == '/'
}

fn integrity_text(value: f32) -> String {
    let mut text = format!("{value:.4}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructureInfo {
    pub pos: BlockPos,
    pub name: String,
    pub mode: StructureMode,
    pub offset: [i32; 3],
    pub size: [i32; 3],
    pub mirror: Mirror,
    pub rotation: Rotation,
    pub data: String,
    pub ignore_entities: bool,
    pub strict: bool,
    pub show_air: bool,
    pub show_bounding_box: bool,
    pub integrity: f32,
    pub seed: i64,
}

const ST_NAME: u32 = 1;
const ST_POS: u32 = 2;
const ST_SIZE: u32 = 5;
const ST_INTEGRITY: u32 = 8;
const ST_SEED: u32 = 9;
const ST_DATA: u32 = 10;
const ST_DONE: u32 = 11;
const ST_CANCEL: u32 = 12;
const ST_SAVE: u32 = 13;
const ST_LOAD: u32 = 14;
const ST_DETECT: u32 = 15;
const ST_ROT: u32 = 16;
const ST_MODE: u32 = 20;
const ST_ENTITIES: u32 = 21;
const ST_STRICT: u32 = 22;
const ST_MIRROR: u32 = 23;
const ST_AIR: u32 = 24;
const ST_BOX: u32 = 25;

const MODES: [StructureMode; 4] = [
    StructureMode::Save,
    StructureMode::Load,
    StructureMode::Corner,
    StructureMode::Data,
];
const MIRRORS: [Mirror; 3] = [Mirror::None, Mirror::LeftRight, Mirror::FrontBack];
const ROTATIONS: [Rotation; 4] = [
    Rotation::None,
    Rotation::Clockwise90,
    Rotation::Clockwise180,
    Rotation::CounterClockwise90,
];

fn mode_name(mode: StructureMode) -> &'static str {
    match mode {
        StructureMode::Save => "save",
        StructureMode::Load => "load",
        StructureMode::Corner => "corner",
        StructureMode::Data => "data",
    }
}

/// Structure block editor with save, load, corner and data modes.
pub struct StructureBlockScreen {
    info: StructureInfo,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for StructureBlockScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructureBlockScreen")
            .finish_non_exhaustive()
    }
}

impl StructureBlockScreen {
    pub fn new(info: StructureInfo) -> Self {
        Self {
            info,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn read_fields(&mut self) {
        if self.widgets.is_empty() {
            return;
        }
        let w = &self.widgets;
        self.info.name = edit_value(w, ST_NAME);
        for i in 0..3 {
            self.info.offset[i] = parse_int(&edit_value(w, ST_POS + i as u32));
            self.info.size[i] = parse_int(&edit_value(w, ST_SIZE + i as u32));
        }
        self.info.integrity = edit_value(w, ST_INTEGRITY).trim().parse().unwrap_or(1.0);
        self.info.seed = edit_value(w, ST_SEED).trim().parse().unwrap_or(0);
        self.info.data = edit_value(w, ST_DATA);
    }

    fn packet(&mut self, update_type: StructureUpdateType) -> EditorPacket {
        self.read_fields();
        let clamp = |v: i32, min: i32| v.clamp(min, EXTENT) as i8;
        EditorPacket::Structure(Box::new(SetStructureBlock {
            pos: self.info.pos,
            update_type,
            mode: self.info.mode,
            name: self.info.name.clone(),
            offset: self.info.offset.map(|v| clamp(v, -EXTENT)),
            size: self.info.size.map(|v| clamp(v, 0)),
            mirror: self.info.mirror,
            rotation: self.info.rotation,
            data: self.info.data.clone(),
            ignore_entities: self.info.ignore_entities,
            strict: self.info.strict,
            show_air: self.info.show_air,
            show_bounding_box: self.info.show_bounding_box,
            integrity: self.info.integrity,
            seed: self.info.seed,
        }))
    }

    fn apply_mode(&mut self) {
        let mode = self.info.mode;
        let save = mode == StructureMode::Save;
        let load = mode == StructureMode::Load;
        let w = &mut self.widgets;
        for id in ST_NAME..=ST_NAME {
            show(w, id, mode != StructureMode::Data);
        }
        for i in 0..3 {
            show(w, ST_POS + i, save || load);
            show(w, ST_SIZE + i, save);
        }
        show(w, ST_INTEGRITY, load);
        show(w, ST_SEED, load);
        show(w, ST_DATA, mode == StructureMode::Data);
        show(w, ST_SAVE, save);
        show(w, ST_LOAD, load);
        show(w, ST_DETECT, save);
        show(w, ST_ENTITIES, save || load);
        show(w, ST_STRICT, load);
        show(w, ST_MIRROR, load);
        for i in 0..4 {
            show(w, ST_ROT + i, load);
            enable(w, ST_ROT + i, ROTATIONS[i as usize] != self.info.rotation);
        }
        show(w, ST_AIR, save);
        show(w, ST_BOX, load);
    }

    fn event(&mut self, event: WidgetEvent, out: &mut Vec<ScreenAction>) {
        let WidgetEvent { id, kind } = event;
        match (id, kind) {
            (ST_DONE, WidgetEventKind::Pressed) => {
                let packet = self.packet(StructureUpdateType::UpdateData);
                out.push(ScreenAction::Editor(packet));
                out.push(ScreenAction::Pop);
            }
            (ST_CANCEL, WidgetEventKind::Pressed) => out.push(ScreenAction::Pop),
            (ST_SAVE, WidgetEventKind::Pressed) => {
                let packet = self.packet(StructureUpdateType::SaveArea);
                out.push(ScreenAction::Editor(packet));
                out.push(ScreenAction::Pop);
            }
            (ST_LOAD, WidgetEventKind::Pressed) => {
                let packet = self.packet(StructureUpdateType::LoadArea);
                out.push(ScreenAction::Editor(packet));
                out.push(ScreenAction::Pop);
            }
            (ST_DETECT, WidgetEventKind::Pressed) => {
                let packet = self.packet(StructureUpdateType::ScanArea);
                out.push(ScreenAction::Editor(packet));
                out.push(ScreenAction::Pop);
            }
            (id, WidgetEventKind::Pressed) if (ST_ROT..ST_ROT + 4).contains(&id) => {
                self.info.rotation = ROTATIONS[(id - ST_ROT) as usize];
                self.apply_mode();
            }
            (ST_MODE, WidgetEventKind::Cycled(i)) => {
                self.info.mode = self.mode_values()[i];
                self.apply_mode();
            }
            (ST_ENTITIES, WidgetEventKind::Cycled(i)) => self.info.ignore_entities = i != 0,
            (ST_STRICT, WidgetEventKind::Cycled(i)) => self.info.strict = i == 0,
            (ST_MIRROR, WidgetEventKind::Cycled(i)) => self.info.mirror = MIRRORS[i % 3],
            (ST_AIR, WidgetEventKind::Cycled(i)) => self.info.show_air = i == 0,
            (ST_BOX, WidgetEventKind::Cycled(i)) => self.info.show_bounding_box = i == 0,
            (_, WidgetEventKind::Submitted) => {
                let packet = self.packet(StructureUpdateType::UpdateData);
                out.push(ScreenAction::Editor(packet));
                out.push(ScreenAction::Pop);
            }
            _ => {}
        }
    }

    fn mode_values(&self) -> Vec<StructureMode> {
        let mut modes = MODES.to_vec();
        if self.info.mode != StructureMode::Data {
            modes.pop();
        }
        modes
    }
}

impl Screen for StructureBlockScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.read_fields();
        self.gui = (width, height);
        let c = width / 2;
        let i = self.info.clone();
        let modes = self.mode_values();
        let w = &mut self.widgets;
        w.clear();
        w.add(Button::new(
            ST_DONE,
            [c - 4 - 150, 210, 150, 20],
            "lang:gui.done",
        ));
        w.add(Button::new(
            ST_CANCEL,
            [c + 4, 210, 150, 20],
            "lang:gui.cancel",
        ));
        w.add(Button::new(
            ST_SAVE,
            [c + 4 + 100, 185, 50, 20],
            "lang:structure_block.button.save",
        ));
        w.add(Button::new(
            ST_LOAD,
            [c + 4 + 100, 185, 50, 20],
            "lang:structure_block.button.load",
        ));
        w.add(CycleButton::new(
            ST_MODE,
            [c - 4 - 150, 185, 50, 20],
            "",
            modes
                .iter()
                .map(|m| format!("lang:structure_block.mode.{}", mode_name(*m)))
                .collect(),
            modes.iter().position(|m| *m == i.mode).unwrap_or(0),
        ));
        w.add(Button::new(
            ST_DETECT,
            [c + 4 + 100, 120, 50, 20],
            "lang:structure_block.button.detect_size",
        ));
        w.add(on_off_button(
            ST_ENTITIES,
            [c + 4 + 100, 160, 50, 20],
            !i.ignore_entities,
        ));
        w.add(on_off_button(
            ST_STRICT,
            [c + 4 + 100, 120, 50, 20],
            i.strict,
        ));
        w.add(CycleButton::new(
            ST_MIRROR,
            [c - 20, 185, 40, 20],
            "",
            vec![
                "lang:mirror.none".into(),
                "lang:mirror.left_right".into(),
                "lang:mirror.front_back".into(),
            ],
            MIRRORS.iter().position(|m| *m == i.mirror).unwrap_or(0),
        ));
        w.add(on_off_button(ST_AIR, [c + 4 + 100, 80, 50, 20], i.show_air));
        w.add(on_off_button(
            ST_BOX,
            [c + 4 + 100, 80, 50, 20],
            i.show_bounding_box,
        ));
        for (n, (label, x)) in [
            ("0", c - 1 - 40 - 1 - 40 - 20),
            ("90", c - 1 - 40 - 20),
            ("180", c + 1 + 20),
            ("270", c + 1 + 40 + 1 + 20),
        ]
        .into_iter()
        .enumerate()
        {
            w.add(Button::new(ST_ROT + n as u32, [x, 185, 40, 20], label));
        }
        w.add(edit(ST_NAME, [c - 152, 40, 300, 20], 128, &i.name));
        for n in 0..3 {
            let x = c - 152 + 80 * n as i32;
            w.add(edit(
                ST_POS + n,
                [x, 80, 80, 20],
                15,
                &i.offset[n as usize].to_string(),
            ));
            w.add(edit(
                ST_SIZE + n,
                [x, 120, 80, 20],
                15,
                &i.size[n as usize].to_string(),
            ));
        }
        w.add(edit(
            ST_INTEGRITY,
            [c - 152, 120, 80, 20],
            15,
            &integrity_text(i.integrity),
        ));
        w.add(edit(
            ST_SEED,
            [c - 72, 120, 80, 20],
            31,
            &i.seed.to_string(),
        ));
        w.add(edit(ST_DATA, [c - 152, 120, 240, 20], 128, &i.data));
        self.apply_mode();
        self.widgets.set_focus(Some(ST_NAME), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let c = self.gui.0 / 2;
        let mode = self.info.mode;
        let title = cx.lang("block.minecraft.structure_block");
        cx.centered_text(&title, c, 10, WHITE, true);
        if mode != StructureMode::Data {
            label_key(cx, "structure_block.structure_name", c - 153, 30);
        }
        let right = c + 154;
        if matches!(mode, StructureMode::Load | StructureMode::Save) {
            label_key(cx, "structure_block.position", c - 153, 70);
            label_key_right(cx, "structure_block.include_entities", right, 150);
        }
        if mode == StructureMode::Save {
            label_key(cx, "structure_block.size", c - 153, 110);
            label_key_right(cx, "structure_block.detect_size", right, 110);
            label_key_right(cx, "structure_block.show_air", right, 70);
        }
        if mode == StructureMode::Load {
            label_key(cx, "structure_block.integrity", c - 153, 110);
            label_key_right(cx, "structure_block.strict", right, 110);
            label_key_right(cx, "structure_block.show_boundingbox", right, 70);
        }
        if mode == StructureMode::Data {
            label_key(cx, "structure_block.custom_data", c - 153, 110);
        }
        label_key(
            cx,
            &format!("structure_block.mode_info.{}", mode_name(mode)),
            c - 153,
            174,
        );
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Char(ch) = *input
            && self.widgets.focused() == Some(ST_NAME)
        {
            let (text, cursor) = self
                .widgets
                .edit(ST_NAME)
                .map(|e| (e.value().to_owned(), e.cursor()))
                .unwrap_or_default();
            if !name_char_ok(&text, ch, cursor) {
                return true;
            }
        }
        let (events, key) = form_input(&mut self.widgets, env, input);
        if key.is_some() {
            out.push(ScreenAction::Pop);
        }
        for event in events {
            if event.kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
            }
            self.event(event, out);
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JigsawInfo {
    pub pos: BlockPos,
    pub name: String,
    pub target: String,
    pub pool: String,
    pub final_state: String,
    pub joint: JointType,
    pub selection_priority: i32,
    pub placement_priority: i32,
    pub vertical: bool,
}

const JG_POOL: u32 = 1;
const JG_NAME: u32 = 2;
const JG_TARGET: u32 = 3;
const JG_FINAL: u32 = 4;
const JG_SELECTION: u32 = 5;
const JG_PLACEMENT: u32 = 6;
const JG_JOINT: u32 = 7;
const JG_LEVELS: u32 = 8;
const JG_KEEP: u32 = 9;
const JG_GENERATE: u32 = 10;
const JG_DONE: u32 = 11;
const JG_CANCEL: u32 = 12;

pub struct JigsawBlockScreen {
    info: JigsawInfo,
    levels: i32,
    keep: bool,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for JigsawBlockScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JigsawBlockScreen").finish_non_exhaustive()
    }
}

impl JigsawBlockScreen {
    pub fn new(info: JigsawInfo) -> Self {
        Self {
            info,
            levels: 0,
            keep: true,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn read_fields(&mut self) {
        if self.widgets.is_empty() {
            return;
        }
        let w = &self.widgets;
        self.info.pool = edit_value(w, JG_POOL);
        self.info.name = edit_value(w, JG_NAME);
        self.info.target = edit_value(w, JG_TARGET);
        self.info.final_state = edit_value(w, JG_FINAL);
        self.info.selection_priority = parse_int(&edit_value(w, JG_SELECTION));
        self.info.placement_priority = parse_int(&edit_value(w, JG_PLACEMENT));
    }

    fn valid(&self) -> bool {
        let w = &self.widgets;
        [JG_NAME, JG_TARGET, JG_POOL]
            .iter()
            .all(|id| valid_identifier(&edit_value(w, *id)))
    }

    fn packet(&mut self) -> Option<EditorPacket> {
        self.read_fields();
        Some(EditorPacket::Jigsaw(SetJigsawBlock {
            pos: self.info.pos,
            name: Identifier::parse(&self.info.name).ok()?,
            target: Identifier::parse(&self.info.target).ok()?,
            pool: Identifier::parse(&self.info.pool).ok()?,
            final_state: self.info.final_state.clone(),
            joint: self.info.joint,
            selection_priority: self.info.selection_priority,
            placement_priority: self.info.placement_priority,
        }))
    }

    fn done(&mut self, generate: bool, out: &mut Vec<ScreenAction>) {
        let Some(packet) = self.packet() else { return };
        out.push(ScreenAction::Editor(packet));
        if generate {
            out.push(ScreenAction::Editor(EditorPacket::JigsawGenerate(
                JigsawGenerate {
                    pos: self.info.pos,
                    levels: self.levels,
                    keep_jigsaws: self.keep,
                },
            )));
        }
        out.push(ScreenAction::Pop);
    }
}

impl Screen for JigsawBlockScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.read_fields();
        self.gui = (width, height);
        let c = width / 2;
        let i = self.info.clone();
        let (levels, keep) = (self.levels, self.keep);
        let w = &mut self.widgets;
        w.clear();
        w.add(edit(JG_POOL, [c - 153, 20, 300, 20], 128, &i.pool));
        w.add(edit(JG_NAME, [c - 153, 55, 300, 20], 128, &i.name));
        w.add(edit(JG_TARGET, [c - 153, 90, 300, 20], 128, &i.target));
        w.add(edit(JG_FINAL, [c - 153, 125, 300, 20], 256, &i.final_state));
        w.add(edit(
            JG_SELECTION,
            [c - 153, 160, 98, 20],
            3,
            &i.selection_priority.to_string(),
        ));
        w.add(edit(
            JG_PLACEMENT,
            [c - 50, 160, 98, 20],
            3,
            &i.placement_priority.to_string(),
        ));
        let mut joint = CycleButton::new(
            JG_JOINT,
            [c + 54, 160, 100, 20],
            "",
            vec![
                "lang:jigsaw_block.joint.rollable".into(),
                "lang:jigsaw_block.joint.aligned".into(),
            ],
            usize::from(i.joint == JointType::Aligned),
        );
        joint.visible = i.vertical;
        joint.active = i.vertical;
        w.add(joint);
        w.add(Slider::new(
            JG_LEVELS,
            [c - 154, 185, 100, 20],
            "",
            levels as f32 / 20.0,
        ));
        w.add(CycleButton::new(
            JG_KEEP,
            [c - 50, 185, 100, 20],
            "lang:jigsaw_block.keep_jigsaws",
            vec!["lang:options.on".into(), "lang:options.off".into()],
            on_off(keep),
        ));
        w.add(Button::new(
            JG_GENERATE,
            [c + 54, 185, 100, 20],
            "lang:jigsaw_block.generate",
        ));
        w.add(Button::new(
            JG_DONE,
            [c - 4 - 150, 210, 150, 20],
            "lang:gui.done",
        ));
        w.add(Button::new(
            JG_CANCEL,
            [c + 4, 210, 150, 20],
            "lang:gui.cancel",
        ));
        w.set_focus(Some(JG_POOL), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let c = self.gui.0 / 2;
        let valid = self.valid();
        enable(&mut self.widgets, JG_DONE, valid);
        enable(&mut self.widgets, JG_GENERATE, valid);
        let levels = cx.lang_fmt("jigsaw_block.levels", &[&self.levels.to_string()]);
        if let Some(s) = self.widgets.slider_mut(JG_LEVELS) {
            s.label = levels;
        }
        label_key(cx, "jigsaw_block.pool", c - 153, 10);
        label_key(cx, "jigsaw_block.name", c - 153, 45);
        label_key(cx, "jigsaw_block.target", c - 153, 80);
        label_key(cx, "jigsaw_block.final_state", c - 153, 115);
        label_key(cx, "jigsaw_block.selection_priority", c - 153, 150);
        label_key(cx, "jigsaw_block.placement_priority", c - 50, 150);
        if self.info.vertical {
            label_key(cx, "jigsaw_block.joint_label", c + 53, 150);
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let (events, key) = form_input(&mut self.widgets, env, input);
        if key.is_some() {
            out.push(ScreenAction::Pop);
        }
        for event in events {
            match (event.id, event.kind) {
                (JG_CANCEL, WidgetEventKind::Pressed) => out.push(ScreenAction::Pop),
                (JG_DONE, WidgetEventKind::Pressed) => self.done(false, out),
                (JG_GENERATE, WidgetEventKind::Pressed) => self.done(true, out),
                (JG_JOINT, WidgetEventKind::Cycled(i)) => {
                    self.info.joint = if i == 0 {
                        JointType::Rollable
                    } else {
                        JointType::Aligned
                    };
                }
                (JG_LEVELS, WidgetEventKind::SliderChanged(v)) => {
                    self.levels = (v * 20.0).floor() as i32;
                }
                (JG_KEEP, WidgetEventKind::Cycled(i)) => self.keep = i == 0,
                (_, WidgetEventKind::Submitted) if self.valid() => self.done(false, out),
                _ => {}
            }
            if event.kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
            }
        }
        true
    }
}

const TB_MESSAGE: u32 = 1;
const TB_MODE: u32 = 2;
const TB_DONE: u32 = 3;
const TB_CANCEL: u32 = 4;

const TEST_MODES: [(TestBlockMode, &str); 4] = [
    (TestBlockMode::Start, "start"),
    (TestBlockMode::Log, "log"),
    (TestBlockMode::Fail, "fail"),
    (TestBlockMode::Accept, "accept"),
];

pub struct TestBlockScreen {
    pos: BlockPos,
    mode: usize,
    message: String,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for TestBlockScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestBlockScreen").finish_non_exhaustive()
    }
}

impl TestBlockScreen {
    pub fn new(pos: BlockPos, mode: TestBlockMode, message: &str) -> Self {
        Self {
            pos,
            mode: TEST_MODES.iter().position(|(m, _)| *m == mode).unwrap_or(0),
            message: message.to_owned(),
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn done(&mut self, out: &mut Vec<ScreenAction>) {
        let message = edit_value(&self.widgets, TB_MESSAGE);
        out.push(ScreenAction::Editor(EditorPacket::TestBlock(
            SetTestBlock {
                pos: self.pos,
                mode: TEST_MODES[self.mode].0,
                message,
            },
        )));
        out.push(ScreenAction::Pop);
    }
}

impl Screen for TestBlockScreen {
    fn init(&mut self, width: i32, height: i32) {
        if !self.widgets.is_empty() {
            self.message = edit_value(&self.widgets, TB_MESSAGE);
        }
        self.gui = (width, height);
        let c = width / 2;
        let w = &mut self.widgets;
        w.clear();
        w.add(edit(TB_MESSAGE, [c - 152, 80, 240, 20], 128, &self.message));
        w.add(CycleButton::new(
            TB_MODE,
            [c - 4 - 150, 185, 50, 20],
            "",
            TEST_MODES
                .iter()
                .map(|(_, n)| format!("lang:test_block.mode.{n}"))
                .collect(),
            self.mode,
        ));
        w.add(Button::new(
            TB_DONE,
            [c - 4 - 150, 210, 150, 20],
            "lang:gui.done",
        ));
        w.add(Button::new(
            TB_CANCEL,
            [c + 4, 210, 150, 20],
            "lang:gui.cancel",
        ));
        show(w, TB_MESSAGE, self.mode != 0);
        w.set_focus(Some(TB_MESSAGE), 0);
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let c = self.gui.0 / 2;
        let title = cx.lang("block.minecraft.test_block");
        cx.centered_text(&title, c, 10, WHITE, true);
        if self.mode != 0 {
            label_key(cx, "test_block.message", c - 153, 70);
        }
        label_key(
            cx,
            &format!("test_block.mode_info.{}", TEST_MODES[self.mode].1),
            c - 153,
            174,
        );
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let (events, key) = form_input(&mut self.widgets, env, input);
        if key.is_some() || pressed(&events, TB_CANCEL) {
            out.push(ScreenAction::Pop);
        } else if pressed(&events, TB_DONE) || submitted(&events) {
            self.done(out);
        }
        for e in &events {
            if let (TB_MODE, WidgetEventKind::Cycled(i)) = (e.id, e.kind) {
                self.mode = i;
                show(&mut self.widgets, TB_MESSAGE, i != 0);
            }
            if e.kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
            }
        }
        true
    }
}

/// What the server last reported about the test instance block being edited.
pub type SharedTestStatus = Rc<RefCell<Option<(Component, Option<[i32; 3]>)>>>;

#[derive(Clone, Debug, PartialEq)]
pub struct TestInstanceInfo {
    pub pos: BlockPos,
    pub test: String,
    pub size: [i32; 3],
    pub rotation: Rotation,
    pub ignore_entities: bool,
    pub error: Option<Component>,
}

const TI_ID: u32 = 1;
const TI_SIZE: u32 = 2;
const TI_ROTATION: u32 = 5;
const TI_ENTITIES: u32 = 6;
const TI_RESET: u32 = 7;
const TI_SAVE: u32 = 8;
const TI_RUN: u32 = 9;
const TI_DONE: u32 = 10;
const TI_CANCEL: u32 = 11;

pub struct TestInstanceBlockScreen {
    info: TestInstanceInfo,
    status: SharedTestStatus,
    description: Component,
    sent_init: bool,
    widgets: WidgetSet,
    gui: (i32, i32),
}

impl std::fmt::Debug for TestInstanceBlockScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestInstanceBlockScreen")
            .finish_non_exhaustive()
    }
}

fn exact(count: i32) -> f32 {
    (316 - (count - 1) * 8) as f32 / count as f32
}

fn slot(index: i32, count: i32) -> (i32, i32) {
    let x = (-158.0 + index as f32 * (8.0 + exact(count))) as i32;
    (x, exact(count) as i32)
}

impl TestInstanceBlockScreen {
    pub fn new(info: TestInstanceInfo, status: SharedTestStatus) -> Self {
        Self {
            info,
            status,
            description: Component::text(""),
            sent_init: false,
            widgets: WidgetSet::default(),
            gui: (0, 0),
        }
    }

    fn valid_id(&self) -> bool {
        valid_identifier(&edit_value(&self.widgets, TI_ID))
    }

    fn packet(&self, action: TestInstanceAction) -> Option<ScreenAction> {
        let test = Identifier::parse(&edit_value(&self.widgets, TI_ID)).ok();
        let size = |n: u32| parse_int(&edit_value(&self.widgets, TI_SIZE + n)).clamp(1, EXTENT);
        let rotation = self.info.rotation;
        let ignore = self.info.ignore_entities;
        test.as_ref()?;
        Some(ScreenAction::Editor(EditorPacket::TestInstance(Box::new(
            TestInstanceBlockAction {
                pos: self.info.pos,
                action,
                data: TestInstanceData {
                    test,
                    size: Vec3i {
                        x: size(0),
                        y: size(1),
                        z: size(2),
                    },
                    rotation,
                    ignore_entities: ignore,
                    status: TestInstanceStatus::Cleared,
                    error_message: None,
                },
            },
        ))))
    }

    fn save_allowed(&self) -> bool {
        self.info.rotation == Rotation::None && self.valid_id()
    }

    fn query(&self, action: TestInstanceAction, out: &mut Vec<ScreenAction>) {
        out.extend(self.packet(action));
    }
}

impl Screen for TestInstanceBlockScreen {
    fn init(&mut self, width: i32, height: i32) {
        if !self.widgets.is_empty() {
            self.info.test = edit_value(&self.widgets, TI_ID);
            for n in 0..3 {
                self.info.size[n] = parse_int(&edit_value(&self.widgets, TI_SIZE + n as u32));
            }
        }
        self.gui = (width, height);
        let c = width / 2;
        let i = self.info.clone();
        let w = &mut self.widgets;
        w.clear();
        w.add(edit(TI_ID, [c - 158, 40, 316, 20], 128, &i.test));
        for n in 0..3 {
            let (x, size) = slot(n, 5);
            w.add(edit(
                TI_SIZE + n as u32,
                [c + x, 160, size, 20],
                15,
                &i.size[n as usize].to_string(),
            ));
        }
        let (x, size) = slot(3, 5);
        w.add(CycleButton::new(
            TI_ROTATION,
            [c + x, 160, size, 20],
            "",
            vec!["0".into(), "90".into(), "180".into(), "270".into()],
            ROTATIONS.iter().position(|r| *r == i.rotation).unwrap_or(0),
        ));
        let (x, size) = slot(4, 5);
        w.add(on_off_button(
            TI_ENTITIES,
            [c + x, 160, size, 20],
            !i.ignore_entities,
        ));
        let (x, size) = slot(0, 2);
        w.add(Button::new(
            TI_RESET,
            [c + x, 185, size, 20],
            "lang:test_instance.action.reset",
        ));
        let (x, size) = slot(1, 2);
        w.add(Button::new(
            TI_SAVE,
            [c + x, 185, size, 20],
            "lang:test_instance.action.save",
        ));
        let (x, size) = slot(0, 3);
        w.add(Button::new(
            TI_RUN,
            [c + x, 210, size, 20],
            "lang:test_instance.action.run",
        ));
        let (x, size) = slot(1, 3);
        w.add(Button::new(
            TI_DONE,
            [c + x, 210, size, 20],
            "lang:gui.done",
        ));
        let (x, size) = slot(2, 3);
        w.add(Button::new(
            TI_CANCEL,
            [c + x, 210, size, 20],
            "lang:gui.cancel",
        ));
        w.set_focus(Some(TI_ID), 0);
    }

    fn tick(&mut self, _env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        if !self.sent_init {
            self.sent_init = true;
            self.query(TestInstanceAction::Init, out);
        }
        if let Some((text, size)) = self.status.borrow_mut().take() {
            self.description = text;
            if let Some(size) = size {
                for (n, v) in size.iter().enumerate() {
                    set_edit(&mut self.widgets, TI_SIZE + n as u32, &v.to_string());
                }
            }
        }
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        draw_dim_background(cx);
        let c = self.gui.0 / 2;
        let left = c - 158;
        let allow = self.save_allowed();
        enable(&mut self.widgets, TI_SAVE, allow);
        let title = cx.lang("block.minecraft.test_instance_block");
        cx.centered_text(&title, c, 10, WHITE, true);
        label_key(cx, "test_instance_block.test_id", left, 30);
        label_key(cx, "test_instance_block.size", left, 150);
        label_key(cx, "test_instance_block.rotation", c + slot(3, 5).0, 150);
        label_key(cx, "test_instance_block.entities", c + slot(4, 5).0, 150);
        let red = Style {
            color: Some(Color::Named(NamedColor::Red)),
            ..Style::default()
        };
        let text = if self.valid_id() {
            let mut text = Component::text("");
            if let Some(error) = &self.info.error {
                let error = Component::text("").with(error.clone()).styled(red);
                text = text
                    .with(Component::translatable(
                        "test_instance.description.failed",
                        vec![Arg::Component(error)],
                    ))
                    .with(Component::text("\n\n"));
            }
            text.with(self.description.clone())
        } else {
            Component::translatable("test_instance.description.invalid_id", Vec::new()).styled(red)
        };
        let cells = cx.frame.ui().flatten(&text);
        for (n, range) in cx
            .frame
            .ui()
            .wrap(&cells, 316.0)
            .into_iter()
            .take(8)
            .enumerate()
        {
            cx.frame.cells(
                &cells[range],
                left as f32,
                (70 + n as i32 * 9) as f32,
                WHITE,
                true,
            );
        }
        self.widgets.render(cx);
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        let (events, key) = form_input(&mut self.widgets, env, input);
        if key.is_some() {
            out.push(ScreenAction::Pop);
        }
        for e in events {
            match (e.id, e.kind) {
                (TI_ID, WidgetEventKind::Edited) => self.query(TestInstanceAction::Query, out),
                (TI_ROTATION, WidgetEventKind::Cycled(i)) => self.info.rotation = ROTATIONS[i % 4],
                (TI_ENTITIES, WidgetEventKind::Cycled(i)) => self.info.ignore_entities = i != 0,
                (TI_RESET, WidgetEventKind::Pressed) => {
                    self.query(TestInstanceAction::Reset, out);
                    out.push(ScreenAction::Pop);
                }
                (TI_SAVE, WidgetEventKind::Pressed) => {
                    self.query(TestInstanceAction::Save, out);
                    out.push(ScreenAction::Pop);
                }
                (TI_RUN, WidgetEventKind::Pressed) => {
                    self.query(TestInstanceAction::Run, out);
                    out.push(ScreenAction::Pop);
                }
                (TI_DONE, WidgetEventKind::Pressed) => {
                    self.query(TestInstanceAction::Set, out);
                    out.push(ScreenAction::Pop);
                }
                (TI_CANCEL, WidgetEventKind::Pressed) => out.push(ScreenAction::Pop),
                _ => {}
            }
            if e.kind == WidgetEventKind::Pressed {
                out.push(ScreenAction::ClickSound);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};

    fn send(screen: &mut dyn Screen, input: UiInput) -> Vec<ScreenAction> {
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        screen.input(&mut env, &input, &mut out);
        out
    }

    fn click_button(screen: &mut dyn Screen, rect_center: (i32, i32)) -> Vec<ScreenAction> {
        let (x, y) = (rect_center.0 as f32, rect_center.1 as f32);
        let mut out = send(
            screen,
            UiInput::press(x, y, super::super::MouseButton::Left),
        );
        out.extend(send(
            screen,
            UiInput::release(x, y, super::super::MouseButton::Left),
        ));
        out
    }

    fn command_info() -> CommandBlockInfo {
        CommandBlockInfo {
            pos: BlockPos::new(1, 2, 3),
            command: "say hi".into(),
            mode: CommandBlockMode::Redstone,
            conditional: false,
            automatic: false,
            track_output: true,
            last_output: "out".into(),
        }
    }

    #[test]
    fn command_block_done_sends_the_edited_fields() {
        let mut s = CommandBlockScreen::block(command_info());
        s.init(320, 240);
        click_button(&mut s, (320 / 2 - 50 - 100 - 4 + 50, 175));
        click_button(&mut s, (320 / 2, 175));
        click_button(&mut s, (320 / 2 + 50 + 4 + 50, 175));
        let out = click_button(&mut s, (320 / 2 - 4 - 75, 240 / 4 + 120 + 12 + 10));
        let Some(ScreenAction::Editor(EditorPacket::CommandBlock(p))) =
            out.iter().find(|a| matches!(a, ScreenAction::Editor(_)))
        else {
            panic!("no packet in {out:?}");
        };
        assert_eq!(p.command, "say hi");
        assert_eq!(p.mode, CommandBlockMode::Sequence);
        assert!(p.conditional && p.automatic && p.track_output);
    }

    #[test]
    fn command_block_enter_submits_and_escape_cancels() {
        let mut s = CommandBlockScreen::block(command_info());
        s.init(320, 240);
        let out = send(&mut s, UiInput::key(Key::Enter));
        assert!(out.iter().any(|a| matches!(a, ScreenAction::Editor(_))));
        let mut s = CommandBlockScreen::minecart(MinecartCommandInfo {
            entity_id: 9,
            command: "x".into(),
            track_output: false,
            last_output: String::new(),
        });
        s.init(320, 240);
        assert!(matches!(
            send(&mut s, UiInput::key(Key::Escape)).as_slice(),
            [ScreenAction::Pop]
        ));
    }

    fn structure() -> StructureInfo {
        StructureInfo {
            pos: BlockPos::new(0, 64, 0),
            name: "base".into(),
            mode: StructureMode::Save,
            offset: [0, 1, 0],
            size: [100, 5, -3],
            mirror: Mirror::None,
            rotation: Rotation::None,
            data: String::new(),
            ignore_entities: false,
            strict: false,
            show_air: false,
            show_bounding_box: true,
            integrity: 1.0,
            seed: 0,
        }
    }

    #[test]
    fn structure_packet_clamps_extents_and_keeps_mode() {
        let mut s = StructureBlockScreen::new(structure());
        s.init(320, 240);
        let EditorPacket::Structure(p) = s.packet(StructureUpdateType::SaveArea) else {
            panic!()
        };
        assert_eq!(p.size, [48, 5, 0]);
        assert_eq!(p.offset, [0, 1, 0]);
        assert_eq!(p.update_type, StructureUpdateType::SaveArea);
        assert_eq!(p.mode, StructureMode::Save);
    }

    #[test]
    fn structure_name_rejects_invalid_characters() {
        assert!(name_char_ok("", 'a', 0));
        assert!(!name_char_ok("", 'A', 0));
        assert!(!name_char_ok("a", ' ', 1));
        assert!(name_char_ok("a", ':', 1));
        assert!(!name_char_ok("a:b", ':', 3));
        assert!(name_char_ok("a:b", '/', 3));
    }

    #[test]
    fn integrity_formats_like_decimal_format() {
        assert_eq!(integrity_text(1.0), "1.0");
        assert_eq!(integrity_text(0.5), "0.5");
        assert_eq!(integrity_text(0.12345), "0.1235");
    }

    #[test]
    fn jigsaw_requires_valid_identifiers() {
        let mut s = JigsawBlockScreen::new(JigsawInfo {
            pos: BlockPos::new(0, 0, 0),
            name: "minecraft:empty".into(),
            target: "minecraft:empty".into(),
            pool: "bad pool".into(),
            final_state: "minecraft:air".into(),
            joint: JointType::Aligned,
            selection_priority: 0,
            placement_priority: 0,
            vertical: false,
        });
        s.init(320, 240);
        assert!(!s.valid());
        let mut out = Vec::new();
        s.done(false, &mut out);
        assert!(out.is_empty());
        set_edit(&mut s.widgets, JG_POOL, "minecraft:village/plains");
        assert!(s.valid());
        s.done(true, &mut out);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn test_block_hides_message_in_start_mode() {
        let mut s = TestBlockScreen::new(BlockPos::new(1, 1, 1), TestBlockMode::Start, "m");
        s.init(320, 240);
        assert!(!s.widgets.find(TB_MESSAGE).unwrap().visible());
        let out = send(&mut s, UiInput::key(Key::Enter));
        assert!(out.is_empty() || out.iter().any(|a| matches!(a, ScreenAction::Editor(_))));
    }

    #[test]
    fn test_instance_init_query_goes_out_on_first_tick() {
        let status = SharedTestStatus::default();
        let mut s = TestInstanceBlockScreen::new(
            TestInstanceInfo {
                pos: BlockPos::new(1, 1, 1),
                test: "minecraft:example".into(),
                size: [3, 3, 3],
                rotation: Rotation::None,
                ignore_entities: false,
                error: None,
            },
            status.clone(),
        );
        s.init(320, 240);
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        s.tick(&mut env, &mut out);
        s.tick(&mut env, &mut out);
        assert_eq!(out.len(), 1);
        *status.borrow_mut() = Some((Component::text("ok"), Some([7, 8, 9])));
        s.tick(&mut env, &mut out);
        assert_eq!(edit_value(&s.widgets, TI_SIZE + 1), "8");
    }
}
