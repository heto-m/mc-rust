//! Name tags above players and named entities, drawn through the UI text renderer.

use mcr_client::{ClientState, Entity, ScoreStyle, format_score};
use mcr_entity_model::mcr_protocol::play::clientbound::ui::{NameTagVisibility, TeamColor};
use mcr_entity_model::mcr_protocol::types::Component;
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_entity_model::mcr_protocol::types::game::GameType;
use mcr_render::Camera;
use mcr_ui::{Cell, UiFrame};

use crate::entities::EntityLayer;
use crate::target::raycast_block;

const MAX_TAGS: usize = 48;
const MAX_CELLS: usize = 64;
const PLAIN_DISTANCE: f64 = 64.0;
const SNEAK_DISTANCE: f64 = 32.0;
const SCORE_DISTANCE: f64 = 10.0;
const MAX_TAG_DISTANCE: f64 = 512.0;
const BELOW_NAME_SLOT: u8 = 2;
const SEE_FRIENDLY_INVISIBLES: i8 = 0x02;
const SNIFFER_TAG: f64 = 2.05;
const SNIFFER_HEIGHT: f64 = 1.75;

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

pub(crate) fn custom_name(e: &Entity) -> Option<&Component> {
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

/// `Entity#isInvisibleTo` for the local player.
fn invisible_to_viewer(state: &ClientState, e: &Entity, entry: &str) -> bool {
    if !shared_flag(e, 5) || state.player.game_mode == GameType::Spectator {
        return false;
    }
    let board = &state.scoreboard;
    let allied_sees = board.team_of(entry).is_some_and(|t| {
        board.team_of(&state.name) == Some(t)
            && board
                .teams
                .get(t)
                .is_some_and(|team| team.parameters.options & SEE_FRIENDLY_INVISIBLES != 0)
    });
    !allied_sees
}

/// `LivingEntityRenderer#shouldShowName` before the per-type name check.
fn living_shows(state: &ClientState, e: &Entity, entry: &str, hud_hidden: bool) -> bool {
    let visible = !invisible_to_viewer(state, e, entry);
    let board = &state.scoreboard;
    if let Some(name) = board.team_of(entry)
        && let Some(team) = board.teams.get(name)
    {
        let own = board.team_of(&state.name);
        let allied = own == Some(name);
        let sees_friends = team.parameters.options & SEE_FRIENDLY_INVISIBLES != 0;
        return match team.parameters.name_tag_visibility {
            NameTagVisibility::Always => visible,
            NameTagVisibility::Never => false,
            NameTagVisibility::HideForOtherTeams => match own {
                None => visible,
                Some(_) => allied && (sees_friends || visible),
            },
            NameTagVisibility::HideForOwnTeam => match own {
                None => visible,
                Some(_) => !allied && visible,
            },
        };
    }
    !hud_hidden && visible && e.passengers.is_empty()
}

fn attribute(e: &Entity, id: Option<i32>, default: f64) -> f64 {
    id.and_then(|a| e.attributes.get(&a))
        .map_or(default, |s| s.value(None))
        .clamp(0.0, MAX_TAG_DISTANCE)
}

/// The `NAME_TAG` attachment height: the pose's height, or the type's explicit offset.
fn tag_height(state: &ClientState, e: &Entity) -> f64 {
    let (_, height) = state.entity_dims(e);
    let data = mcr_data::v26_3();
    match data.entity_type_name(e.kind as u32) {
        Some("minecraft:sniffer") => height * SNIFFER_TAG / SNIFFER_HEIGHT,
        _ => height,
    }
}

/// Every tag that vanilla would draw for the entities drawn this frame.
pub fn collect(
    layer: &EntityLayer,
    state: &ClientState,
    camera: [f64; 3],
    partial: f32,
    crosshair: Option<i32>,
    hud_hidden: bool,
) -> Vec<Tag> {
    let data = mcr_data::v26_3();
    let player_type = data
        .entity_type_by_name("minecraft:player")
        .map_or(-1, |id| id as i32);
    let attribute_id = |name: &str| {
        data.registry("minecraft:attribute")
            .and_then(|r| r.id_of(name))
            .map(|id| id as i32)
    };
    let tag_distance = attribute_id("minecraft:name_tag_distance");
    let below_distance = attribute_id("minecraft:below_name_distance");
    let mut tags = Vec::new();
    for (id, pos, sneaking) in layer.drawn_entities(state, partial) {
        let Some(e) = state.entities.get(id) else {
            continue;
        };
        let is_player = e.kind == player_type;
        let living = matches!(e.data.get(&9), Some(EntityDataValue::Float(_)));
        let profile = is_player.then(|| state.players.name_of(e.uuid)).flatten();
        let entry = profile.map_or_else(|| e.uuid.to_string(), str::to_owned);
        let own_name = custom_name(e).filter(|n| !is_empty_component(n));
        let visible_flag = matches!(e.data.get(&3), Some(EntityDataValue::Boolean(true)));
        let name = match (profile, own_name) {
            (Some(p), _) => Component::text(p),
            (None, Some(n)) if visible_flag || crosshair == Some(id) => n.clone(),
            _ => continue,
        };
        let d: f64 = (0..3)
            .map(|i| (pos[i] - camera[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        let limit = if living {
            attribute(e, tag_distance, PLAIN_DISTANCE)
        } else {
            PLAIN_DISTANCE
        };
        if d >= limit || (living && sneaking && d >= SNEAK_DISTANCE) {
            continue;
        }
        if living && !living_shows(state, e, &entry, hud_hidden) {
            continue;
        }
        let anchor = [pos[0], pos[1] + tag_height(state, e) + 0.5, pos[2]];
        let board = &state.scoreboard;
        let team = board.team_of(&entry).and_then(|t| board.teams.get(t));
        let score = (is_player && d < attribute(e, below_distance, SCORE_DISTANCE))
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

/// `Options#getBackgroundOpacity(0.25)`.
pub fn background_opacity(chat_only: bool, text_background_opacity: f64) -> f64 {
    if chat_only {
        0.25
    } else {
        text_background_opacity.clamp(0.0, 1.0)
    }
}

/// The ARGB of the dimmed text pass: `max((background + 0.75) / 2, 0.5)` alpha.
pub fn dim_color(background: f64) -> u32 {
    let alpha = ((background.clamp(0.0, 1.0) + 0.75) * 0.5).max(0.5);
    ((alpha * 255.0) as u32) << 24 | 0xFF_FFFF
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
            dim_color(p.opacity)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_text_follows_the_background_opacity() {
        assert_eq!(background_opacity(true, 0.9), 0.25);
        assert_eq!(background_opacity(false, 0.9), 0.9);
        assert_eq!(background_opacity(false, 7.0), 1.0);
        assert_eq!(dim_color(0.25), 0x7FFF_FFFF);
        assert_eq!(dim_color(0.0) >> 24, 0x7F);
        assert_eq!(dim_color(1.0) >> 24, 0xDF);
    }
}
