//! Name tags above players and named entities, drawn through the UI text renderer.

use mcr_client::{ClientState, Entity, ScoreStyle, format_score};
use mcr_entity_model::mcr_protocol::play::clientbound::ui::{NameTagVisibility, TeamColor};
use mcr_entity_model::mcr_protocol::types::Component;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_render::Camera;
use mcr_ui::{Cell, UiFrame};

use crate::entities::EntityLayer;
use crate::target::raycast_block;

const MAX_TAGS: usize = 48;
const MAX_CELLS: usize = 64;
const PLAIN_DISTANCE: f64 = 64.0;
const SNEAK_DISTANCE: f64 = 32.0;
const SCORE_DISTANCE: f64 = 10.0;
const DIM: u32 = 0x20FF_FFFF;
const BELOW_NAME_SLOT: u8 = 2;

#[derive(Debug, Clone)]
pub struct Tag {
    pub anchor: [f64; 3],
    pub distance: f64,
    pub sneaking: bool,
    name: Component,
    color: Option<u32>,
    prefix: Option<Component>,
    suffix: Option<Component>,
    score: Option<Component>,
}

pub(crate) fn team_color(color: TeamColor) -> u32 {
    match color {
        TeamColor::Black => 0x000000,
        TeamColor::DarkBlue => 0x0000AA,
        TeamColor::DarkGreen => 0x00AA00,
        TeamColor::DarkAqua => 0x00AAAA,
        TeamColor::DarkRed => 0xAA0000,
        TeamColor::DarkPurple => 0xAA00AA,
        TeamColor::Gold => 0xFFAA00,
        TeamColor::Gray => 0xAAAAAA,
        TeamColor::DarkGray => 0x555555,
        TeamColor::Blue => 0x5555FF,
        TeamColor::Green => 0x55FF55,
        TeamColor::Aqua => 0x55FFFF,
        TeamColor::Red => 0xFF5555,
        TeamColor::LightPurple => 0xFF55FF,
        TeamColor::Yellow => 0xFFFF55,
        TeamColor::White => 0xFFFFFF,
    }
}

fn custom_name(e: &Entity) -> Option<&Component> {
    match e.data.get(&2) {
        Some(EntityDataValue::OptionalComponent(Some(c))) => Some(c),
        _ => None,
    }
}

fn is_empty_component(c: &Component) -> bool {
    c.to_plain_string().is_empty()
}

fn shared_flag(e: &Entity, bit: u8) -> bool {
    matches!(e.data.get(&0), Some(EntityDataValue::Byte(f)) if (*f as u8) >> bit & 1 != 0)
}

fn team_allows(state: &ClientState, entry: &str, invisible: bool) -> Option<bool> {
    let board = &state.scoreboard;
    let team = board.teams.get(board.team_of(entry)?)?;
    let own = board.team_of(&state.name);
    let visible = !invisible;
    Some(match team.parameters.name_tag_visibility {
        NameTagVisibility::Always => visible,
        NameTagVisibility::Never => false,
        NameTagVisibility::HideForOtherTeams => match own {
            None => visible,
            Some(o) => board.team_of(entry) == Some(o) && visible,
        },
        NameTagVisibility::HideForOwnTeam => match own {
            None => visible,
            Some(o) => board.team_of(entry) != Some(o) && visible,
        },
    })
}

/// Every tag that vanilla would draw for the entities drawn this frame.
pub fn collect(
    layer: &EntityLayer,
    state: &ClientState,
    camera: [f64; 3],
    partial: f32,
    crosshair: Option<i32>,
) -> Vec<Tag> {
    let player_type = mcr_data::v26_3()
        .entity_type_by_name("minecraft:player")
        .map_or(-1, |id| id as i32);
    let mut tags = Vec::new();
    for (id, pos, height, sneaking) in layer.drawn_entities(state, partial) {
        let Some(e) = state.entities.get(id) else {
            continue;
        };
        if !e.passengers.is_empty() || shared_flag(e, 5) {
            continue;
        }
        let is_player = e.kind == player_type;
        let profile = is_player.then(|| state.players.name_of(e.uuid)).flatten();
        let own_name = custom_name(e).filter(|n| !is_empty_component(n));
        let visible_flag = matches!(e.data.get(&3), Some(EntityDataValue::Boolean(true)));
        let name = match (profile, own_name) {
            (Some(p), _) => Component::text(p),
            (None, Some(n)) if visible_flag || crosshair == Some(id) => n.clone(),
            _ => continue,
        };
        let entry = profile.map_or_else(|| e.uuid.to_string(), str::to_owned);
        if team_allows(state, &entry, false) == Some(false) {
            continue;
        }
        let limit = if sneaking {
            SNEAK_DISTANCE
        } else {
            PLAIN_DISTANCE
        };
        let anchor = [pos[0], pos[1] + f64::from(height) + 0.5, pos[2]];
        let d: f64 = (0..3)
            .map(|i| (pos[i] - camera[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        if d >= limit {
            continue;
        }
        let board = &state.scoreboard;
        let team = board.team_of(&entry).and_then(|t| board.teams.get(t));
        let score = (is_player && d < SCORE_DISTANCE)
            .then(|| board.display_slots.get(&BELOW_NAME_SLOT))
            .flatten()
            .and_then(|objective| {
                let o = board.objectives.get(objective)?;
                let value = board
                    .format_value(&entry, objective, ScoreStyle::Plain)
                    .unwrap_or_else(|| {
                        format_score(o.number_format.as_ref(), ScoreStyle::Plain, 0)
                    });
                Some(
                    Component::text("")
                        .with(value)
                        .with(Component::text(" "))
                        .with(o.display_name.clone()),
                )
            });
        tags.push(Tag {
            anchor,
            distance: d,
            sneaking,
            name,
            color: team.and_then(|t| t.parameters.color).map(team_color),
            prefix: team.map(|t| t.parameters.player_prefix.clone()),
            suffix: team.map(|t| t.parameters.player_suffix.clone()),
            score,
        });
        if tags.len() >= MAX_TAGS {
            break;
        }
    }
    tags.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    tags
}

fn cells_of(frame: &mut UiFrame<'_>, tag: &Tag) -> Vec<Cell> {
    let ui = frame.ui();
    let mut cells = Vec::new();
    if let Some(p) = &tag.prefix {
        cells.extend(ui.flatten(p).into_iter().take(MAX_CELLS));
    }
    let mut name = ui.flatten(&tag.name);
    name.truncate(MAX_CELLS);
    if let Some(color) = tag.color {
        for c in &mut name {
            c.style.color.get_or_insert(color);
        }
    }
    cells.extend(name);
    if let Some(s) = &tag.suffix {
        cells.extend(ui.flatten(s).into_iter().take(MAX_CELLS));
    }
    cells.truncate(MAX_CELLS * 2);
    cells
}

pub struct TagParams<'a> {
    pub camera: &'a Camera,
    pub framebuffer: (u32, u32),
    pub opacity: f64,
    pub state: &'a ClientState,
}

pub fn draw(frame: &mut UiFrame<'_>, tags: &[Tag], p: &TagParams<'_>) {
    let aspect = p.framebuffer.0 as f32 / p.framebuffer.1.max(1) as f32;
    let vp = p.camera.view_proj(aspect);
    let gui_scale = frame.scale() as f32;
    let tan_half = (p.camera.fov_y / 2.0).tan().max(1.0e-3);
    let bg_alpha = ((p.opacity.clamp(0.0, 1.0) * 255.0) as u32) << 24;
    let data = mcr_data::v26_3();
    let cam = p.camera.position;
    for tag in tags {
        let rel = mcr_render::glam::Vec3::new(
            (tag.anchor[0] - cam.x) as f32,
            (tag.anchor[1] - cam.y) as f32,
            (tag.anchor[2] - cam.z) as f32,
        );
        let clip = vp * mcr_render::glam::Vec4::new(rel.x, rel.y, rel.z, 1.0);
        if clip.w <= 0.05 {
            continue;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        if ndc_x.abs() > 1.5 || ndc_y.abs() > 1.5 {
            continue;
        }
        let occluded = {
            let dir = [rel.x, rel.y, rel.z].map(|v| f64::from(v) / tag.distance.max(1.0e-6));
            raycast_block(
                &p.state.chunks,
                data,
                cam.to_array(),
                dir,
                (tag.distance - 0.5).max(0.0),
            )
            .is_some()
        };
        if tag.sneaking && occluded {
            continue;
        }
        let fb_h = p.framebuffer.1 as f32;
        let px_per_font = 0.025 * (fb_h / 2.0) / (tan_half * clip.w) / gui_scale;
        let sx = (ndc_x * 0.5 + 0.5) * p.framebuffer.0 as f32 / gui_scale;
        let sy = (0.5 - ndc_y * 0.5) * fb_h / gui_scale;
        let color = if tag.sneaking || occluded {
            DIM
        } else {
            0xFFFF_FFFF
        };
        let mut y_lift = 0.0;
        if let Some(score) = &tag.score {
            let mut cells = frame.ui().flatten(score);
            cells.truncate(MAX_CELLS * 2);
            draw_line(frame, &cells, sx, sy, px_per_font, color, bg_alpha);
            y_lift = 9.0 * 1.15 * px_per_font;
        }
        let cells = cells_of(frame, tag);
        draw_line(frame, &cells, sx, sy - y_lift, px_per_font, color, bg_alpha);
    }
}

fn draw_line(
    frame: &mut UiFrame<'_>,
    cells: &[Cell],
    cx: f32,
    bottom: f32,
    scale: f32,
    color: u32,
    bg: u32,
) {
    if cells.is_empty() || !(0.05..=8.0).contains(&scale) {
        return;
    }
    let width = frame.ui().width(cells);
    let x = cx - width * scale / 2.0;
    let y = bottom - 9.0 * scale;
    frame.fill_rect(
        (x - scale).floor() as i32,
        (y - scale).floor() as i32,
        (x + width * scale + scale).ceil() as i32,
        (y + 8.0 * scale + scale).ceil() as i32,
        bg,
    );
    frame.cells_scaled(cells, x, y, color, false, scale);
}
