use mcr_ui::{Cell, PageKind, Ui};

use crate::data::{DyeColor, SignSide};

pub const MAX_LINE_CELLS: usize = 128;
pub const DARK_GLOW_OUTLINE: u32 = 0xFFF0_EBCC;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextQuad {
    pub pos: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [u8; 4],
    pub page: u32,
}

/// Glyph quads of one sign side in font pixels (x right, y down), centred on the line origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SideLayout {
    pub main: Vec<TextQuad>,
    pub outline: Vec<TextQuad>,
    pub glowing: bool,
    pub black_text: bool,
}

pub fn dark_color(side: &SignSide) -> u32 {
    let rgb = side.color.text_rgb();
    if side.color == DyeColor::BLACK && side.glowing {
        DARK_GLOW_OUTLINE
    } else {
        let ch = |s: u32| (((rgb >> s) & 0xFF) as f32 * 0.4) as u32;
        0xFF00_0000 | (ch(16) << 16) | (ch(8) << 8) | ch(0)
    }
}

fn collect(ui: &Ui, white: [f32; 2], keep_effects: bool, out: &mut Vec<TextQuad>) {
    let batch = ui.batch();
    for cmd in &batch.cmds {
        if cmd.page.kind != PageKind::Glyph {
            continue;
        }
        for q in cmd.quads.clone() {
            let base = q as usize * 4;
            let Some(v) = batch.vertices.get(base..base + 4) else {
                continue;
            };
            let is_effect = v.iter().all(|v| v.uv == white);
            if is_effect && !keep_effects {
                continue;
            }
            out.push(TextQuad {
                pos: [v[0].pos, v[1].pos, v[2].pos, v[3].pos],
                uv: [v[0].uv, v[1].uv, v[2].uv, v[3].uv],
                color: v[0].color,
                page: cmd.page.index,
            });
        }
    }
}

fn first_line(ui: &mut Ui, cells: &mut Vec<Cell>, max_width: f32) {
    cells.truncate(MAX_LINE_CELLS);
    if let Some(range) = ui.wrap(cells, max_width).into_iter().next() {
        cells.truncate(range.end);
        cells.drain(..range.start);
    } else {
        cells.clear();
    }
}

pub fn layout_side(ui: &mut Ui, side: &SignSide, hanging: bool) -> SideLayout {
    let (line_height, max_width) = if hanging { (9.0, 60.0) } else { (10.0, 90.0) };
    let midpoint = 4.0 * line_height / 2.0;
    let mut lines: Vec<Vec<Cell>> = Vec::with_capacity(4);
    for component in &side.lines {
        let mut cells = ui.flatten(component);
        first_line(ui, &mut cells, max_width);
        lines.push(cells);
    }
    let text_argb = if side.glowing {
        0xFF00_0000 | side.color.text_rgb()
    } else {
        dark_color(side)
    };
    let outline_argb = dark_color(side);
    let white = ui.fonts().atlas().white_uv();
    let mut out = SideLayout {
        glowing: side.glowing,
        black_text: side.color.text_rgb() == 0,
        ..SideLayout::default()
    };
    let place = |ui: &mut Ui, argb: u32, strip: bool, offset: [f32; 2], out: &mut Vec<TextQuad>| {
        {
            let mut frame = ui.frame(1, 1);
            for (i, cells) in lines.iter().enumerate() {
                let width = ceil_width(&mut frame, cells);
                let x = (-width / 2) as f32 + offset[0];
                let y = i as f32 * line_height - midpoint + offset[1];
                if strip {
                    let plain: Vec<Cell> = cells
                        .iter()
                        .map(|c| {
                            let mut c = *c;
                            c.style.color = None;
                            c.style.shadow = None;
                            c
                        })
                        .collect();
                    frame.cells(&plain, x, y, argb, false);
                } else {
                    frame.cells(cells, x, y, argb, false);
                }
            }
        }
        collect(ui, white, !strip, out);
    };
    place(ui, text_argb, false, [0.0, 0.0], &mut out.main);
    if side.glowing {
        for xo in -1..=1 {
            for yo in -1..=1 {
                if xo != 0 || yo != 0 {
                    place(
                        ui,
                        outline_argb,
                        true,
                        [xo as f32, yo as f32],
                        &mut out.outline,
                    );
                }
            }
        }
    }
    out
}

fn ceil_width(frame: &mut mcr_ui::UiFrame<'_>, cells: &[Cell]) -> i32 {
    frame.ui().width(cells).ceil() as i32
}

pub const MAX_DISPLAY_LINES: usize = 64;
pub const MAX_DISPLAY_CELLS: usize = 4096;
pub const MAX_DISPLAY_WIDTH: i32 = 1024;
pub const DISPLAY_LINE_HEIGHT: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Center,
    Left,
    Right,
}

/// Glyph quads of a text display in font pixels (x right, y down, origin at the first line's top left).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DisplayLayout {
    pub quads: Vec<TextQuad>,
    pub width: i32,
    /// `lines * 10 - 1`, or 0 without text.
    pub height: i32,
    pub white: [f32; 2],
}

/// `TextDisplayRenderer.splitLines` plus the per-line alignment of `submitInner`.
pub fn layout_display(
    ui: &mut Ui,
    cells: &[Cell],
    line_width: i32,
    align: Align,
    shadow: bool,
) -> DisplayLayout {
    let cells = &cells[..cells.len().min(MAX_DISPLAY_CELLS)];
    let white = ui.fonts().atlas().white_uv();
    let mut lines = ui.wrap(cells, line_width.clamp(1, MAX_DISPLAY_WIDTH) as f32);
    lines.truncate(MAX_DISPLAY_LINES);
    let widths: Vec<i32> = lines
        .iter()
        .map(|r| ui.width(&cells[r.clone()]).ceil() as i32)
        .collect();
    let width = widths.iter().copied().max().unwrap_or(0);
    let mut out = DisplayLayout {
        quads: Vec::new(),
        width,
        height: if lines.is_empty() {
            0
        } else {
            lines.len() as i32 * DISPLAY_LINE_HEIGHT - 1
        },
        white,
    };
    for (i, (range, lw)) in lines.iter().zip(&widths).enumerate() {
        let x = match align {
            Align::Left => 0.0,
            Align::Right => (width - lw) as f32,
            Align::Center => width as f32 / 2.0 - *lw as f32 / 2.0,
        };
        let y = (i as i32 * DISPLAY_LINE_HEIGHT) as f32;
        {
            let mut frame = ui.frame(1, 1);
            frame.cells(&cells[range.clone()], x, y, 0xFFFF_FFFF, shadow);
        }
        collect(ui, white, true, &mut out.quads);
    }
    out
}
