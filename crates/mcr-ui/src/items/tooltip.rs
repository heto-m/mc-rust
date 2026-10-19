//! Vanilla's item tooltip lines (`ItemStack#getTooltipLines`) built from a stack's patch, the item's
//! default components and the server's registries.

use mcr_buf::IdOr;
use mcr_nbt::{Compound, List, Value};
use mcr_protocol::types::component::{
    AdventureModePredicate, AttributeDisplay, AttributeOperation, BannerPatternLayer,
    DataComponent, DataComponentPatch, DyeColor, EquipmentSlotGroup, FireworkExplosion,
    FireworkShape, HolderSet, ItemEnchantments, MobEffectInstance, ProfileSource, Rarity, SignText,
    component_type_id,
};
use mcr_protocol::types::item::{ItemStack, ItemStackTemplate};
use mcr_protocol::types::text::{Arg, Color, Component, NamedColor, Style};

use super::defaults::{ItemDefaults, SLOT_GROUPS, item_defaults, slot_group, slot_group_name};
use super::effects::{EffectCategory, effect_info, potion_effects, potion_name};

/// Vanilla caps lore at 256 lines; a tooltip taller than the screen is useless anyway.
pub const MAX_TOOLTIP_LINES: usize = 64;
const MAX_CONTAINER_LINES: usize = 5;
const MAX_BANNER_LINES: usize = 6;
const MAX_BLOCK_NAMES: usize = 32;
const MAX_NESTED_DEPTH: u8 = 2;
/// Server lists (enchantments, modifiers, effects) are only scanned this far.
const MAX_SCANNED: usize = 256;
const MAX_ORDER_TAG: usize = 1024;
const PLAYER_BASE_ATTACK_DAMAGE: f64 = 1.0;
const PLAYER_BASE_ATTACK_SPEED: f64 = 4.0;

/// An entry of a server-synced registry with its (possibly built-in) data.
#[derive(Clone, Copy, Debug)]
pub struct RegistryRef<'a> {
    pub id: i32,
    pub name: &'a str,
    pub data: Option<&'a Compound>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapInfo {
    pub scale: i8,
    pub locked: bool,
}

/// What tooltips read from the game beyond the stack itself.
pub trait TooltipSource {
    fn entry(&self, _registry: &str, _id: i32) -> Option<RegistryRef<'_>> {
        None
    }

    fn entry_named(&self, _registry: &str, _name: &str) -> Option<RegistryRef<'_>> {
        None
    }

    fn tag(&self, _registry: &str, _tag: &str) -> Option<&[i32]> {
        None
    }

    fn map_info(&self, _id: i32) -> Option<MapInfo> {
        None
    }

    fn tick_rate(&self) -> f32 {
        20.0
    }
}

#[derive(Debug, Default)]
pub struct NoTooltipSource;

impl TooltipSource for NoTooltipSource {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TooltipFlags {
    /// F3+H.
    pub advanced: bool,
    pub creative: bool,
}

/// A stack or stack template seen with its item's defaults.
#[derive(Clone, Copy, Debug)]
pub struct ItemRef<'a> {
    pub item: i32,
    pub count: i32,
    pub patch: &'a DataComponentPatch,
    name: &'static str,
    defaults: &'static ItemDefaults,
}

impl<'a> ItemRef<'a> {
    pub fn new(item: i32, count: i32, patch: &'a DataComponentPatch) -> Self {
        let name = u32::try_from(item)
            .ok()
            .and_then(|id| mcr_data::v26_3().item_name(id))
            .unwrap_or("minecraft:air");
        Self {
            item,
            count,
            patch,
            name,
            defaults: item_defaults(name),
        }
    }

    pub fn of_stack(stack: &'a ItemStack) -> Self {
        Self::new(stack.item, stack.count, &stack.components)
    }

    pub fn of_template(t: &'a ItemStackTemplate) -> Self {
        Self::new(t.item, t.count, &t.components)
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn path(&self) -> &'static str {
        self.name.split_once(':').map_or(self.name, |p| p.1)
    }

    pub fn get(&self, component: &str) -> Option<&'a DataComponent> {
        self.patch.get(component_type_id(component)?)
    }

    fn removed(&self, component: &str) -> bool {
        component_type_id(component).is_some_and(|id| self.patch.removed.contains(&id))
    }

    fn default_has(&self, component: &str) -> bool {
        let Some(id) = component_type_id(component).and_then(|id| u32::try_from(id).ok()) else {
            return false;
        };
        u32::try_from(self.item)
            .is_ok_and(|item| mcr_data::v26_3().item_has_default_component(item, id))
    }

    pub fn has(&self, component: &str) -> bool {
        self.get(component).is_some() || (self.default_has(component) && !self.removed(component))
    }

    /// A default value applies only when the patch neither replaces nor removes the component.
    fn default_value<T>(&self, component: &str, value: Option<T>) -> Option<T> {
        value.filter(|_| self.get(component).is_none() && !self.removed(component))
    }

    fn enchantments(&self) -> Option<&'a ItemEnchantments> {
        match self.get("enchantments") {
            Some(DataComponent::Enchantments(e)) => Some(e),
            _ => None,
        }
    }

    pub fn is_enchanted(&self) -> bool {
        self.enchantments().is_some_and(|e| !e.0.is_empty())
    }

    /// Size of the full component map, as the advanced tooltip counts it.
    pub fn component_count(&self) -> usize {
        let defaults = u32::try_from(self.item).map_or(0, |item| {
            (0..128)
                .filter(|&c| mcr_data::v26_3().item_has_default_component(item, c))
                .count()
        });
        let item = u32::try_from(self.item).ok();
        let is_default = |id: i32| {
            item.zip(u32::try_from(id).ok())
                .is_some_and(|(i, c)| mcr_data::v26_3().item_has_default_component(i, c))
        };
        let added = self
            .patch
            .added
            .iter()
            .filter(|c| !is_default(c.type_id()))
            .count();
        let removed = self
            .patch
            .removed
            .iter()
            .filter(|&&id| is_default(id))
            .count();
        (defaults + added).saturating_sub(removed)
    }

    pub fn rarity(&self) -> Rarity {
        let base = match self.get("rarity") {
            Some(DataComponent::Rarity(r)) => *r,
            _ if self.removed("rarity") => Rarity::Common,
            _ => self.defaults.rarity,
        };
        if !self.is_enchanted() {
            return base;
        }
        match base {
            Rarity::Common | Rarity::Uncommon => Rarity::Rare,
            Rarity::Rare | Rarity::Epic => Rarity::Epic,
        }
    }

    fn display(&self) -> Display<'a> {
        match self.get("tooltip_display") {
            Some(DataComponent::TooltipDisplay(d)) => Display {
                hide: d.hide_tooltip,
                hidden: &d.hidden_components,
            },
            _ => Display {
                hide: false,
                hidden: &[],
            },
        }
    }

    /// `Item#getDescriptionId`.
    fn description_id(&self) -> String {
        let (namespace, path) = self
            .name
            .split_once(':')
            .unwrap_or(("minecraft", self.name));
        let kind = if self.defaults.block_name {
            "block"
        } else {
            "item"
        };
        format!("{kind}.{namespace}.{}", path.replace('/', "."))
    }
}

#[derive(Clone, Copy, Debug)]
struct Display<'a> {
    hide: bool,
    hidden: &'a [mcr_buf::VarInt],
}

impl Display<'_> {
    fn shows(&self, component: &str) -> bool {
        !self.hide
            && component_type_id(component).is_none_or(|id| !self.hidden.iter().any(|h| h.0 == id))
    }
}

fn tr(key: impl Into<String>, args: Vec<Arg>) -> Component {
    Component::translatable(key, args)
}

fn colored(mut c: Component, color: NamedColor) -> Component {
    c.style.color = Some(Color::Named(color));
    c
}

fn gray(c: Component) -> Component {
    colored(c, NamedColor::Gray)
}

fn italic(mut c: Component) -> Component {
    c.style.italic = Some(true);
    c
}

/// `Component.empty().append(c)` with a style, so `c`'s own style still wins.
fn wrap(c: Component, style: Style) -> Component {
    Component::text("").styled(style).with(c)
}

/// `ComponentUtils.mergeStyles`: the component's own style over `color`.
fn with_default_color(mut c: Component, color: NamedColor) -> Component {
    if c.style.color.is_none() {
        c.style.color = Some(Color::Named(color));
    }
    c
}

fn prefixed(prefix: &str, c: Component) -> Component {
    Component::text(prefix).with(c)
}

fn num(v: i64) -> Arg {
    Arg::Str(v.to_string())
}

pub fn rarity_color(r: Rarity) -> NamedColor {
    match r {
        Rarity::Common => NamedColor::White,
        Rarity::Uncommon => NamedColor::Yellow,
        Rarity::Rare => NamedColor::Aqua,
        Rarity::Epic => NamedColor::LightPurple,
    }
}

pub fn dye_name(c: DyeColor) -> &'static str {
    use DyeColor as D;
    match c {
        D::White => "white",
        D::Orange => "orange",
        D::Magenta => "magenta",
        D::LightBlue => "light_blue",
        D::Yellow => "yellow",
        D::Lime => "lime",
        D::Pink => "pink",
        D::Gray => "gray",
        D::LightGray => "light_gray",
        D::Cyan => "cyan",
        D::Purple => "purple",
        D::Blue => "blue",
        D::Brown => "brown",
        D::Green => "green",
        D::Red => "red",
        D::Black => "black",
    }
}

/// `DyeColor#fireworkColor` in declaration order.
const FIREWORK_COLORS: [(i32, &str); 16] = [
    (15790320, "white"),
    (15435844, "orange"),
    (12801229, "magenta"),
    (6719955, "light_blue"),
    (14602026, "yellow"),
    (4312372, "lime"),
    (14188952, "pink"),
    (4408131, "gray"),
    (11250603, "light_gray"),
    (2651799, "cyan"),
    (8073150, "purple"),
    (2437522, "blue"),
    (5320730, "brown"),
    (3887386, "green"),
    (11743532, "red"),
    (1973019, "black"),
];

fn shape_name(s: FireworkShape) -> &'static str {
    match s {
        FireworkShape::SmallBall => "small_ball",
        FireworkShape::LargeBall => "large_ball",
        FireworkShape::Star => "star",
        FireworkShape::Creeper => "creeper",
        FireworkShape::Burst => "burst",
    }
}

/// `ItemAttributeModifiers.ATTRIBUTE_MODIFIER_FORMAT`: `#.##`, half-even.
pub fn format_amount(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_owned()
}

/// `StringUtil#formatTickDuration`.
pub fn format_ticks(ticks: i32, tick_rate: f32) -> String {
    let seconds = (ticks.max(0) as f32 / tick_rate.max(1.0)).floor() as i64;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn path_of(name: &str) -> &str {
    name.split_once(':').map_or(name, |p| p.1)
}

fn description_key(prefix: &str, name: &str) -> String {
    let (namespace, path) = name.split_once(':').unwrap_or(("minecraft", name));
    format!("{prefix}.{namespace}.{}", path.replace('/', "."))
}

fn static_name(registry: &str, id: i32) -> Option<&'static str> {
    mcr_data::v26_3()
        .registry(registry)?
        .name_of(u32::try_from(id).ok()?)
}

fn nbt_component(data: Option<&Compound>, key: &str) -> Option<Component> {
    Some(Component::from_nbt_lenient(data?.get(key)?))
}

fn nbt_number(v: &Value) -> Option<f64> {
    Some(match v {
        Value::Byte(n) => f64::from(*n),
        Value::Short(n) => f64::from(*n),
        Value::Int(n) => f64::from(*n),
        Value::Long(n) => *n as f64,
        Value::Float(n) => f64::from(*n),
        Value::Double(n) => *n,
        _ => return None,
    })
}

fn attribute_sentiment_color(attribute: &str, increase: bool) -> NamedColor {
    match path_of(attribute) {
        "burning_time" | "fall_damage_multiplier" => {
            if increase {
                NamedColor::Red
            } else {
                NamedColor::Blue
            }
        }
        "gravity" | "scale" | "waypoint_transmit_range" | "waypoint_receive_range" => {
            NamedColor::Gray
        }
        _ => {
            if increase {
                NamedColor::Blue
            } else {
                NamedColor::Red
            }
        }
    }
}

fn attribute_name(attribute: &str) -> Component {
    tr(format!("attribute.name.{}", path_of(attribute)), Vec::new())
}

fn op_id(op: AttributeOperation) -> i32 {
    match op {
        AttributeOperation::AddValue => 0,
        AttributeOperation::AddMultipliedBase => 1,
        AttributeOperation::AddMultipliedTotal => 2,
    }
}

fn percent(op: AttributeOperation) -> bool {
    op != AttributeOperation::AddValue
}

/// `ItemAttributeModifiers.Display.Default#apply` for a player holding the item.
fn modifier_line(
    attribute: &str,
    id: &str,
    amount: f64,
    op: AttributeOperation,
) -> Option<Component> {
    let mut amount = amount;
    let mut with_base = false;
    match path_of(id) {
        "base_attack_damage" => {
            amount += PLAYER_BASE_ATTACK_DAMAGE;
            with_base = true;
        }
        "base_attack_speed" => {
            amount += PLAYER_BASE_ATTACK_SPEED;
            with_base = true;
        }
        _ => {}
    }
    let display = if percent(op) {
        amount * 100.0
    } else if path_of(attribute) == "knockback_resistance" {
        amount * 10.0
    } else {
        amount
    };
    let name = Arg::Component(attribute_name(attribute));
    let op = op_id(op);
    if with_base {
        let line = tr(
            format!("attribute.modifier.equals.{op}"),
            vec![Arg::Str(format_amount(display)), name],
        );
        return Some(colored(prefixed(" ", line), NamedColor::DarkGreen));
    }
    if amount > 0.0 {
        Some(colored(
            tr(
                format!("attribute.modifier.plus.{op}"),
                vec![Arg::Str(format_amount(display)), name],
            ),
            attribute_sentiment_color(attribute, true),
        ))
    } else if amount < 0.0 {
        Some(colored(
            tr(
                format!("attribute.modifier.take.{op}"),
                vec![Arg::Str(format_amount(-display)), name],
            ),
            attribute_sentiment_color(attribute, false),
        ))
    } else {
        None
    }
}

struct Lines {
    out: Vec<Component>,
}

impl Lines {
    fn push(&mut self, c: Component) {
        if self.out.len() < MAX_TOOLTIP_LINES {
            self.out.push(c);
        }
    }

    fn empty(&mut self) {
        self.push(Component::text(""));
    }

    fn full(&self) -> bool {
        self.out.len() >= MAX_TOOLTIP_LINES
    }
}

/// `ItemStack#getHoverName`.
pub fn hover_name(item: &ItemRef<'_>, src: &dyn TooltipSource) -> Component {
    if let Some(DataComponent::CustomName(name)) = item.get("custom_name") {
        return name.clone();
    }
    if let Some(DataComponent::WrittenBookContent(book)) = item.get("written_book_content")
        && !book.title.raw.0.trim().is_empty()
    {
        return Component::text(book.title.raw.0.clone());
    }
    item_name(item, src)
}

/// `Item#getName`, with the overrides of potions, tipped arrows, player heads, shields and compasses.
pub fn item_name(item: &ItemRef<'_>, src: &dyn TooltipSource) -> Component {
    let _ = src;
    let description = item.description_id();
    match item.path() {
        "potion" | "splash_potion" | "lingering_potion" | "tipped_arrow"
            if item.has("potion_contents") =>
        {
            let suffix = match item.get("potion_contents") {
                Some(DataComponent::PotionContents(p)) => p
                    .custom_name
                    .clone()
                    .or_else(|| {
                        p.potion
                            .and_then(|id| static_name("minecraft:potion", id.0))
                            .map(|n| potion_name(n).to_owned())
                    })
                    .unwrap_or_else(|| "empty".to_owned()),
                _ => "empty".to_owned(),
            };
            return tr(format!("{description}.effect.{suffix}"), Vec::new());
        }
        "player_head" => {
            if let Some(DataComponent::Profile(profile)) = item.get("profile") {
                let name = match &profile.source {
                    ProfileSource::Full(p) => Some(p.name.clone()),
                    ProfileSource::Partial(p) => p.name.as_ref().map(|n| n.0.clone()),
                };
                if let Some(name) = name {
                    return tr(format!("{description}.named"), vec![Arg::Str(name)]);
                }
            }
        }
        "shield" => {
            if let Some(DataComponent::BaseColor(c)) = item.get("base_color") {
                return tr(format!("{description}.{}", dye_name(*c)), Vec::new());
            }
        }
        "compass" if item.get("lodestone_tracker").is_some() => {
            return tr("item.minecraft.lodestone_compass", Vec::new());
        }
        _ => {}
    }
    match item.get("item_name") {
        Some(DataComponent::ItemName(name)) => name.clone(),
        _ if item.removed("item_name") => Component::text(""),
        _ => tr(description, Vec::new()),
    }
}

/// `ItemStack#getStyledHoverName`: rarity colour, italic when renamed.
pub fn styled_hover_name(item: &ItemRef<'_>, src: &dyn TooltipSource) -> Component {
    let mut style = Style {
        color: Some(Color::Named(rarity_color(item.rarity()))),
        ..Style::default()
    };
    if item.get("custom_name").is_some() {
        style.italic = Some(true);
    }
    wrap(hover_name(item, src), style)
}

/// Every tooltip line of a stack, or none when `tooltip_display` hides it outside creative.
pub fn tooltip_lines(
    item: &ItemRef<'_>,
    src: &dyn TooltipSource,
    flags: TooltipFlags,
) -> Vec<Component> {
    let display = item.display();
    if !flags.creative && display.hide {
        return Vec::new();
    }
    let mut lines = Lines { out: Vec::new() };
    lines.push(styled_hover_name(item, src));
    details(item, src, flags, display, &mut lines, 0);
    lines.out
}

fn details(
    item: &ItemRef<'_>,
    src: &dyn TooltipSource,
    flags: TooltipFlags,
    display: Display<'_>,
    lines: &mut Lines,
    depth: u8,
) {
    hover_text(item, src, flags, display, lines);
    let shows = |c: &str| display.shows(c);
    if shows("tropical_fish/pattern")
        && let Some(DataComponent::TropicalFishPattern(_)) = item.get("tropical_fish/pattern")
    {
        tropical_fish(item, lines);
    }
    if shows("instrument") {
        let description = match item.get("instrument") {
            Some(DataComponent::Instrument(IdOr::Inline(i))) => Some(i.description.clone()),
            Some(DataComponent::Instrument(IdOr::Id(id))) => nbt_component(
                src.entry("minecraft:instrument", *id).and_then(|e| e.data),
                "description",
            ),
            _ => item
                .default_value("instrument", item.defaults.instrument.as_deref())
                .and_then(|n| {
                    src.entry_named("minecraft:instrument", &format!("minecraft:{n}"))
                        .and_then(|e| nbt_component(e.data, "description"))
                        .or_else(|| Some(tr(format!("instrument.minecraft.{n}"), Vec::new())))
                }),
        };
        if let Some(d) = description {
            lines.push(with_default_color(d, NamedColor::Gray));
        }
    }
    if shows("map_id")
        && let Some(DataComponent::MapId(id)) = item.get("map_id")
    {
        map_lines(item, src, flags, id.0, lines);
    }
    if shows("bees") {
        let bees = match item.get("bees") {
            Some(DataComponent::Bees(b)) => Some(b.len()),
            _ => item.default_value("bees", item.defaults.bees.then_some(0)),
        };
        if let Some(n) = bees {
            lines.push(gray(tr(
                "container.beehive.bees",
                vec![num(n as i64), num(3)],
            )));
        }
    }
    if shows("container_loot") && item.get("container_loot").is_some() {
        lines.push(tr("item.container.loot_table.unknown", Vec::new()));
    }
    if shows("container")
        && let Some(DataComponent::Container(slots)) = item.get("container")
    {
        container_lines(slots.0.iter().flatten(), src, lines);
    }
    if shows("banner_patterns")
        && let Some(DataComponent::BannerPatterns(layers)) = item.get("banner_patterns")
    {
        for layer in layers.iter().take(MAX_BANNER_LINES) {
            if let Some(line) = banner_layer(layer, src) {
                lines.push(gray(line));
            }
        }
    }
    if shows("pot_decorations")
        && let Some(DataComponent::PotDecorations(p)) = item.get("pot_decorations")
    {
        let sides = [&p.front, &p.left, &p.right, &p.back];
        let all_brick = sides.iter().all(|s| {
            s.as_ref()
                .is_none_or(|t| ItemRef::of_template(t).path() == "brick")
        });
        if !all_brick {
            lines.empty();
            for side in sides.into_iter().flatten() {
                let name = hover_name(&ItemRef::of_template(side), src);
                lines.push(gray(Component::from_content(name.content)));
            }
        }
    }
    if shows("written_book_content")
        && let Some(DataComponent::WrittenBookContent(book)) = item.get("written_book_content")
    {
        if !book.author.trim().is_empty() {
            lines.push(gray(tr(
                "book.byAuthor",
                vec![Arg::Str(book.author.clone())],
            )));
        }
        lines.push(gray(tr(
            format!("book.generation.{}", book.generation.clamp(0, 3)),
            Vec::new(),
        )));
    }
    if shows("charged_projectiles")
        && let Some(DataComponent::ChargedProjectiles(list)) = item.get("charged_projectiles")
    {
        charged_projectiles(&list.0, src, lines, depth);
    }
    if shows("fireworks") {
        match item.get("fireworks") {
            Some(DataComponent::Fireworks(f)) => {
                fireworks(f.flight_duration, &f.explosions.0, lines)
            }
            _ => {
                if let Some(flight) = item.default_value("fireworks", item.defaults.flight) {
                    fireworks(flight, &[], lines);
                }
            }
        }
    }
    if shows("firework_explosion")
        && let Some(DataComponent::FireworkExplosion(e)) = item.get("firework_explosion")
    {
        lines.push(gray(tr(
            format!("item.minecraft.firework_star.shape.{}", shape_name(e.shape)),
            Vec::new(),
        )));
        explosion_details(e, lines, "");
    }
    if shows("potion_contents") {
        let scale = match item.get("potion_duration_scale") {
            Some(DataComponent::PotionDurationScale(s)) => *s,
            _ => item
                .default_value("potion_duration_scale", item.defaults.potion_scale)
                .unwrap_or(1.0),
        };
        match item.get("potion_contents") {
            Some(DataComponent::PotionContents(p)) => {
                let mut effects: Vec<(String, i32, i32)> = p
                    .potion
                    .and_then(|id| static_name("minecraft:potion", id.0))
                    .map(potion_effects)
                    .unwrap_or_default()
                    .iter()
                    .map(|&(e, d, a)| (e.to_owned(), d, a))
                    .collect();
                effects.extend(
                    p.custom_effects
                        .iter()
                        .take(MAX_SCANNED)
                        .filter_map(effect_tuple),
                );
                potion_tooltip(&effects, scale, src.tick_rate(), lines);
            }
            _ => {
                if item
                    .default_value("potion_contents", item.defaults.potion.then_some(()))
                    .is_some()
                {
                    potion_tooltip(&[], scale, src.tick_rate(), lines);
                }
            }
        }
    }
    if shows("jukebox_playable") {
        let description = match item.get("jukebox_playable") {
            Some(DataComponent::JukeboxPlayable(IdOr::Inline(song))) => {
                Some(song.description.clone())
            }
            Some(DataComponent::JukeboxPlayable(IdOr::Id(id))) => nbt_component(
                src.entry("minecraft:jukebox_song", *id)
                    .and_then(|e| e.data),
                "description",
            ),
            _ => item
                .default_value("jukebox_playable", item.defaults.jukebox_song.as_deref())
                .and_then(|n| {
                    src.entry_named("minecraft:jukebox_song", &format!("minecraft:{n}"))
                        .and_then(|e| nbt_component(e.data, "description"))
                        .or_else(|| Some(tr(format!("jukebox_song.minecraft.{n}"), Vec::new())))
                }),
        };
        if let Some(d) = description {
            lines.push(with_default_color(d, NamedColor::Gray));
        }
    }
    if shows("trim")
        && let Some(DataComponent::Trim(trim)) = item.get("trim")
    {
        trim_lines(trim, src, lines);
    }
    if shows("stored_enchantments")
        && let Some(DataComponent::StoredEnchantments(e)) = item.get("stored_enchantments")
    {
        enchantment_lines(e, src, lines);
    }
    if shows("enchantments")
        && let Some(e) = item.enchantments()
    {
        enchantment_lines(e, src, lines);
    }
    if shows("dyed_color")
        && let Some(DataComponent::DyedColor(rgb)) = item.get("dyed_color")
    {
        if flags.advanced {
            lines.push(gray(tr(
                "item.color",
                vec![Arg::Str(format!("#{:06X}", rgb & 0xFF_FFFF))],
            )));
        } else {
            lines.push(italic(gray(tr("item.dyed", Vec::new()))));
        }
    }
    if shows("profile")
        && let Some(DataComponent::Profile(p)) = item.get("profile")
        && matches!(&p.source, ProfileSource::Partial(partial) if partial.id.is_none() && partial.name.is_none() && partial.properties.0.is_empty())
    {
        lines.push(gray(tr("component.profile.dynamic", Vec::new())));
    }
    if shows("lore")
        && let Some(DataComponent::Lore(lore)) = item.get("lore")
    {
        for line in &lore.0.0 {
            if lines.full() {
                break;
            }
            lines.push(wrap(
                line.clone(),
                Style {
                    color: Some(Color::Named(NamedColor::DarkPurple)),
                    italic: Some(true),
                    ..Style::default()
                },
            ));
        }
    }
    if shows("sulfur_cube_content")
        && let Some(DataComponent::SulfurCubeContent(t)) = item.get("sulfur_cube_content")
    {
        let name = hover_name(&ItemRef::of_template(t), src);
        lines.push(italic(gray(tr(
            "entity.minecraft.sulfur_cube.content",
            vec![Arg::Component(name)],
        ))));
    }
    if shows("attribute_modifiers") {
        attribute_lines(item, src, lines);
    }
    if shows("intangible_projectile") && item.has("intangible_projectile") {
        lines.push(gray(tr("item.intangible", Vec::new())));
    }
    if shows("unbreakable") && item.has("unbreakable") {
        lines.push(colored(
            tr("item.unbreakable", Vec::new()),
            NamedColor::Blue,
        ));
    }
    for (component, title) in [
        ("sign_text_front", "sign.front_text"),
        ("sign_text_back", "sign.back_text"),
    ] {
        if !shows(component) {
            continue;
        }
        let text = match item.get(component) {
            Some(DataComponent::SignTextFront(t) | DataComponent::SignTextBack(t)) => t,
            _ => continue,
        };
        if !sign_is_empty(text) {
            lines.push(gray(tr(title, Vec::new())));
            for message in &text.messages {
                lines.push(prefixed("  ", message.clone()));
            }
        }
    }
    if shows("ominous_bottle_amplifier") {
        let amp = match item.get("ominous_bottle_amplifier") {
            Some(DataComponent::OminousBottleAmplifier(a)) => Some(a.0),
            _ => item.default_value("ominous_bottle_amplifier", item.defaults.ominous),
        };
        if let Some(amp) = amp {
            potion_tooltip(
                &[("bad_omen".to_owned(), 120_000, amp.clamp(0, 4))],
                1.0,
                src.tick_rate(),
                lines,
            );
        }
    }
    if shows("suspicious_stew_effects") && flags.creative {
        match item.get("suspicious_stew_effects") {
            Some(DataComponent::SuspiciousStewEffects(list)) => {
                let effects: Vec<_> = list
                    .iter()
                    .take(MAX_SCANNED)
                    .filter_map(|e| {
                        Some((
                            static_name("minecraft:mob_effect", e.effect)?.to_owned(),
                            e.duration,
                            0,
                        ))
                    })
                    .collect();
                potion_tooltip(&effects, 1.0, src.tick_rate(), lines);
            }
            _ => {
                if item
                    .default_value("suspicious_stew_effects", item.defaults.stew.then_some(()))
                    .is_some()
                {
                    potion_tooltip(&[], 1.0, src.tick_rate(), lines);
                }
            }
        }
    }
    if shows("block_state") {
        let honey = match item.get("block_state") {
            Some(DataComponent::BlockState(props)) => props
                .iter()
                .find(|p| p.name == "honey_level")
                .and_then(|p| p.value.parse::<i32>().ok()),
            _ => item.default_value("block_state", item.defaults.honey),
        };
        if let Some(level) = honey {
            lines.push(gray(tr(
                "container.beehive.honey",
                vec![num(i64::from(level)), num(5)],
            )));
        }
    }
    for (component, header) in [
        ("can_break", "item.canBreak"),
        ("can_place_on", "item.canPlace"),
    ] {
        if !shows(component) {
            continue;
        }
        if let Some(DataComponent::CanBreak(p) | DataComponent::CanPlaceOn(p)) = item.get(component)
        {
            lines.empty();
            lines.push(gray(tr(header, Vec::new())));
            predicate_lines(p, src, lines);
        }
    }
    if flags.advanced {
        if shows("damage") {
            let (damage, max) = (damage(item), max_damage(item));
            if damage > 0 && max > 0 {
                lines.push(tr(
                    "item.durability",
                    vec![num(i64::from(max - damage)), num(i64::from(max))],
                ));
            }
        }
        lines.push(colored(Component::text(item.name()), NamedColor::DarkGray));
        let count = item.component_count();
        if count > 0 {
            lines.push(colored(
                tr("item.components", vec![num(count as i64)]),
                NamedColor::DarkGray,
            ));
        }
    }
}

fn damage(item: &ItemRef<'_>) -> i32 {
    match item.get("damage") {
        Some(DataComponent::Damage(v)) => v.0,
        _ => 0,
    }
}

fn max_damage(item: &ItemRef<'_>) -> i32 {
    match item.get("max_damage") {
        Some(DataComponent::MaxDamage(v)) => v.0,
        _ => u32::try_from(item.item)
            .ok()
            .and_then(|id| mcr_data::v26_3().item_info(id))
            .map_or(0, |i| i.max_damage as i32),
    }
}

fn sign_is_empty(t: &SignText) -> bool {
    let blank = |m: &[Component; 4]| m.iter().all(|c| *c == Component::text(""));
    blank(&t.messages)
        && t.filtered_messages.as_ref().is_none_or(blank)
        && t.color == DyeColor::Black
        && !t.has_glowing_text
}

/// `Item#appendHoverText` overrides: smithing templates, disc fragments and paintings.
fn hover_text(
    item: &ItemRef<'_>,
    src: &dyn TooltipSource,
    flags: TooltipFlags,
    display: Display<'_>,
    lines: &mut Lines,
) {
    let path = item.path();
    if !item.name().starts_with("minecraft:") {
        return;
    }
    if path.ends_with("_smithing_template") {
        let kind = if path == "netherite_upgrade_smithing_template" {
            "netherite_upgrade"
        } else {
            "armor_trim"
        };
        let blue = |key: String| colored(tr(key, Vec::new()), NamedColor::Blue);
        lines.push(gray(tr("item.minecraft.smithing_template", Vec::new())));
        lines.empty();
        lines.push(gray(tr(
            "item.minecraft.smithing_template.applies_to",
            Vec::new(),
        )));
        lines.push(prefixed(
            " ",
            blue(format!(
                "item.minecraft.smithing_template.{kind}.applies_to"
            )),
        ));
        lines.push(gray(tr(
            "item.minecraft.smithing_template.ingredients",
            Vec::new(),
        )));
        lines.push(prefixed(
            " ",
            blue(format!(
                "item.minecraft.smithing_template.{kind}.ingredients"
            )),
        ));
    } else if path.starts_with("disc_fragment_") {
        lines.push(gray(tr(
            format!("{}.desc", item.description_id()),
            Vec::new(),
        )));
    } else if path == "painting" && display.shows("painting/variant") {
        match item.get("painting/variant") {
            Some(DataComponent::PaintingVariant(variant)) => {
                let (title, author, w, h) = match variant {
                    IdOr::Inline(v) => (v.title.clone(), v.author.clone(), v.width, v.height),
                    IdOr::Id(id) => {
                        let data = src
                            .entry("minecraft:painting_variant", *id)
                            .and_then(|e| e.data);
                        (
                            nbt_component(data, "title"),
                            nbt_component(data, "author"),
                            data.and_then(|d| d.get_i32("width")).unwrap_or(1),
                            data.and_then(|d| d.get_i32("height")).unwrap_or(1),
                        )
                    }
                };
                lines.out.extend(title.into_iter().chain(author).take(2));
                lines.push(tr(
                    "painting.dimensions",
                    vec![num(i64::from(w)), num(i64::from(h))],
                ));
            }
            _ if flags.creative => lines.push(gray(tr("painting.random", Vec::new()))),
            _ => {}
        }
    }
}

fn tropical_fish(item: &ItemRef<'_>, lines: &mut Lines) {
    let Some(DataComponent::TropicalFishPattern(pattern)) = item.get("tropical_fish/pattern")
    else {
        return;
    };
    let color = |c: &str, d: DyeColor| match item.get(c) {
        Some(
            DataComponent::TropicalFishBaseColor(c) | DataComponent::TropicalFishPatternColor(c),
        ) => *c,
        _ => d,
    };
    let base = color("tropical_fish/base_color", DyeColor::White);
    let pat = color("tropical_fish/pattern_color", DyeColor::White);
    const PATTERNS: [&str; 12] = [
        "kob",
        "sunstreak",
        "snooper",
        "dasher",
        "brinely",
        "spotty",
        "flopper",
        "stripey",
        "glitter",
        "blockfish",
        "betty",
        "clayfish",
    ];
    let style = Style {
        color: Some(Color::Named(NamedColor::Gray)),
        italic: Some(true),
        ..Style::default()
    };
    if let Some(name) = usize::try_from(pattern.0)
        .ok()
        .and_then(|i| PATTERNS.get(i))
    {
        lines.push(
            tr(
                format!("entity.minecraft.tropical_fish.type.{name}"),
                Vec::new(),
            )
            .styled(style.clone()),
        );
    }
    let mut colors = tr(format!("color.minecraft.{}", dye_name(base)), Vec::new());
    if base != pat {
        colors = colors
            .with(Component::text(", "))
            .with(tr(format!("color.minecraft.{}", dye_name(pat)), Vec::new()));
    }
    lines.push(colors.styled(style));
}

fn map_lines(
    item: &ItemRef<'_>,
    src: &dyn TooltipSource,
    flags: TooltipFlags,
    id: i32,
    lines: &mut Lines,
) {
    let Some(info) = src.map_info(id) else {
        lines.push(gray(tr("filled_map.unknown", Vec::new())));
        return;
    };
    let post = match item.get("map_post_processing") {
        Some(DataComponent::MapPostProcessing(p)) => Some(*p),
        _ => None,
    };
    if item.get("custom_name").is_none() && post.is_none() {
        lines.push(gray(tr("filled_map.id", vec![num(i64::from(id))])));
    }
    use mcr_protocol::types::component::MapPostProcessing as P;
    if info.locked || post == Some(P::Lock) {
        lines.push(gray(tr("filled_map.locked", Vec::new())));
    }
    if flags.advanced {
        let add = i32::from(post == Some(P::Scale));
        let scale = (i32::from(info.scale) + add).clamp(0, 4);
        lines.push(gray(tr("filled_map.scale", vec![num(1 << scale)])));
        lines.push(gray(tr(
            "filled_map.level",
            vec![num(i64::from(scale)), num(4)],
        )));
    }
}

fn container_lines<'a>(
    items: impl Iterator<Item = &'a ItemStackTemplate>,
    src: &dyn TooltipSource,
    lines: &mut Lines,
) {
    let (mut shown, mut total) = (0usize, 0usize);
    for t in items {
        if t.count <= 0 {
            continue;
        }
        total += 1;
        if shown < MAX_CONTAINER_LINES {
            shown += 1;
            let name = hover_name(&ItemRef::of_template(t), src);
            lines.push(tr(
                "item.container.item_count",
                vec![num(i64::from(t.count)), Arg::Component(name)],
            ));
        }
    }
    if total > shown {
        lines.push(italic(tr(
            "item.container.more_items",
            vec![num((total - shown) as i64)],
        )));
    }
}

fn banner_layer(layer: &BannerPatternLayer, src: &dyn TooltipSource) -> Option<Component> {
    let key = match &layer.pattern {
        IdOr::Inline(p) => p.translation_key.clone(),
        IdOr::Id(id) => src
            .entry("minecraft:banner_pattern", *id)?
            .data?
            .get_str("translation_key")?
            .to_owned(),
    };
    Some(tr(format!("{key}.{}", dye_name(layer.color)), Vec::new()))
}

fn charged_projectiles(
    list: &[ItemStackTemplate],
    src: &dyn TooltipSource,
    lines: &mut Lines,
    depth: u8,
) {
    if depth >= MAX_NESTED_DEPTH {
        return;
    }
    let mut i = 0;
    while i < list.len() && !lines.full() {
        let current = &list[i];
        let mut count = 1;
        while i + count < list.len() && list[i + count] == *current {
            count += 1;
        }
        i += count;
        let item = ItemRef::of_template(current);
        if count == 1 {
            lines.push(tr(
                "item.minecraft.crossbow.projectile.single",
                vec![Arg::Component(display_name(&item, src))],
            ));
        } else {
            lines.push(tr(
                "item.minecraft.crossbow.projectile.multiple",
                vec![num(count as i64), Arg::Component(hover_name(&item, src))],
            ));
        }
        let mut nested = Lines { out: Vec::new() };
        details(
            &item,
            src,
            TooltipFlags::default(),
            item.display(),
            &mut nested,
            depth + 1,
        );
        for line in nested.out {
            lines.push(gray(prefixed("  ", line)));
        }
    }
}

/// `ItemStack#getDisplayName`: the styled hover name in brackets.
fn display_name(item: &ItemRef<'_>, src: &dyn TooltipSource) -> Component {
    let inner = Component::text("[")
        .with(hover_name(item, src))
        .with(Component::text("]"));
    wrap(
        inner,
        Style {
            color: Some(Color::Named(rarity_color(item.rarity()))),
            ..Style::default()
        },
    )
}

fn color_name(rgb: i32) -> Component {
    match FIREWORK_COLORS.iter().find(|c| c.0 == rgb) {
        Some((_, name)) => tr(format!("item.minecraft.firework_star.{name}"), Vec::new()),
        None => tr("item.minecraft.firework_star.custom_color", Vec::new()),
    }
}

fn color_list(mut builder: Component, colors: &[i32]) -> Component {
    for (i, &c) in colors.iter().take(16).enumerate() {
        if i > 0 {
            builder = builder.with(Component::text(", "));
        }
        builder = builder.with(color_name(c));
    }
    builder
}

fn explosion_details(e: &FireworkExplosion, lines: &mut Lines, indent: &str) {
    let line = |c: Component| gray(prefixed(indent, c));
    if !e.colors.is_empty() {
        lines.push(line(gray(color_list(Component::text(""), &e.colors))));
    }
    if !e.fade_colors.is_empty() {
        let head =
            tr("item.minecraft.firework_star.fade_to", Vec::new()).with(Component::text(" "));
        lines.push(line(gray(color_list(
            Component::text("").with(head),
            &e.fade_colors,
        ))));
    }
    if e.has_trail {
        lines.push(line(gray(tr(
            "item.minecraft.firework_star.trail",
            Vec::new(),
        ))));
    }
    if e.has_twinkle {
        lines.push(line(gray(tr(
            "item.minecraft.firework_star.flicker",
            Vec::new(),
        ))));
    }
}

fn fireworks(flight: i32, explosions: &[FireworkExplosion], lines: &mut Lines) {
    if flight > 0 {
        lines.push(gray(tr(
            "item.minecraft.firework_rocket.flight_duration",
            vec![num(i64::from(flight))],
        )));
    }
    let mut i = 0;
    while i < explosions.len() && !lines.full() {
        let current = &explosions[i];
        let mut count = 1;
        while i + count < explosions.len() && explosions[i + count] == *current {
            count += 1;
        }
        i += count;
        let shape = Arg::Component(tr(
            format!(
                "item.minecraft.firework_star.shape.{}",
                shape_name(current.shape)
            ),
            Vec::new(),
        ));
        let line = if count == 1 {
            tr("item.minecraft.firework_rocket.single_star", vec![shape])
        } else {
            tr(
                "item.minecraft.firework_rocket.multiple_stars",
                vec![num(count as i64), shape],
            )
        };
        lines.push(gray(line));
        explosion_details(current, lines, "  ");
    }
}

fn effect_tuple(e: &MobEffectInstance) -> Option<(String, i32, i32)> {
    Some((
        static_name("minecraft:mob_effect", e.effect)?.to_owned(),
        e.details.duration,
        e.details.amplifier,
    ))
}

/// `MobEffect#getDisplayName` with the level numeral `PotionContents#getPotionDescription` appends.
pub fn effect_description(effect: &str, amplifier: i32) -> Component {
    let name = tr(description_key("effect", effect), Vec::new());
    if amplifier > 0 {
        tr(
            "potion.withAmplifier",
            vec![
                Arg::Component(name),
                Arg::Component(tr(format!("potion.potency.{amplifier}"), Vec::new())),
            ],
        )
    } else {
        name
    }
}

/// `PotionContents#addPotionTooltip`.
fn potion_tooltip(effects: &[(String, i32, i32)], scale: f32, tick_rate: f32, lines: &mut Lines) {
    let mut modifiers = Vec::new();
    for (effect, duration, amplifier) in effects.iter().take(MAX_TOOLTIP_LINES) {
        let info = effect_info(effect);
        if let Some(m) = info.and_then(|i| i.modifier) {
            modifiers.push((
                m.attribute,
                m.amount * f64::from(amplifier.saturating_add(1)),
                m.operation,
            ));
        }
        let mut line = effect_description(effect, *amplifier);
        let ends_within = *duration != -1 && *duration <= 20;
        if !ends_within {
            let time = if *duration == -1 {
                tr("effect.duration.infinite", Vec::new())
            } else {
                Component::text(format_ticks(
                    (*duration as f32 * scale).floor() as i32,
                    tick_rate,
                ))
            };
            line = tr(
                "potion.withDuration",
                vec![Arg::Component(line), Arg::Component(time)],
            );
        }
        let color = match info.map(|i| i.category) {
            Some(EffectCategory::Harmful) => NamedColor::Red,
            _ => NamedColor::Blue,
        };
        lines.push(colored(line, color));
    }
    if effects.is_empty() {
        lines.push(gray(tr("effect.none", Vec::new())));
    }
    if !modifiers.is_empty() {
        lines.empty();
        lines.push(colored(
            tr("potion.whenDrank", Vec::new()),
            NamedColor::DarkPurple,
        ));
        for (attribute, amount, op) in modifiers {
            let display = if percent(op) { amount * 100.0 } else { amount };
            let name = Arg::Component(attribute_name(attribute));
            let op = op_id(op);
            if amount > 0.0 {
                lines.push(colored(
                    tr(
                        format!("attribute.modifier.plus.{op}"),
                        vec![Arg::Str(format_amount(display)), name],
                    ),
                    NamedColor::Blue,
                ));
            } else if amount < 0.0 {
                lines.push(colored(
                    tr(
                        format!("attribute.modifier.take.{op}"),
                        vec![Arg::Str(format_amount(-display)), name],
                    ),
                    NamedColor::Red,
                ));
            }
        }
    }
}

fn trim_lines(
    trim: &mcr_protocol::types::component::ArmorTrim,
    src: &dyn TooltipSource,
    lines: &mut Lines,
) {
    let material = match &trim.material {
        IdOr::Inline(m) => Some(m.description.clone()),
        IdOr::Id(id) => nbt_component(
            src.entry("minecraft:trim_material", *id)
                .and_then(|e| e.data),
            "description",
        ),
    };
    let pattern = match &trim.pattern {
        IdOr::Inline(p) => Some(p.description.clone()),
        IdOr::Id(id) => nbt_component(
            src.entry("minecraft:trim_pattern", *id)
                .and_then(|e| e.data),
            "description",
        ),
    };
    lines.push(gray(tr(
        "item.minecraft.smithing_template.upgrade",
        Vec::new(),
    )));
    if let Some(mut pattern) = pattern {
        if let Some(m) = &material {
            pattern.style = m.style.clone();
        }
        lines.push(prefixed(" ", pattern));
    }
    if let Some(material) = material {
        lines.push(prefixed(" ", material));
    }
}

/// `Enchantment#getFullname` for each enchantment, `#tooltip_order` first.
fn enchantment_lines(e: &ItemEnchantments, src: &dyn TooltipSource, lines: &mut Lines) {
    const REGISTRY: &str = "minecraft:enchantment";
    let order = src.tag(REGISTRY, "minecraft:tooltip_order").unwrap_or(&[]);
    let order = &order[..order.len().min(MAX_ORDER_TAG)];
    let entries = &e.0[..e.0.len().min(MAX_SCANNED)];
    let mut ordered: Vec<(i32, i32)> = Vec::new();
    for &id in order {
        if let Some(level) = entries
            .iter()
            .find(|l| l.enchantment == id)
            .map(|l| l.level)
            && level > 0
        {
            ordered.push((id, level));
        }
    }
    for l in entries {
        if !order.contains(&l.enchantment) {
            ordered.push((l.enchantment, l.level));
        }
    }
    for (id, level) in ordered {
        if lines.full() {
            break;
        }
        lines.push(enchantment_name(id, level, src));
    }
}

pub fn enchantment_name(id: i32, level: i32, src: &dyn TooltipSource) -> Component {
    const REGISTRY: &str = "minecraft:enchantment";
    let entry = src.entry(REGISTRY, id);
    let description = entry
        .and_then(|e| nbt_component(e.data, "description"))
        .or_else(|| entry.map(|e| tr(description_key("enchantment", e.name), Vec::new())))
        .unwrap_or_else(|| Component::text("?"));
    let curse = src
        .tag(REGISTRY, "minecraft:curse")
        .is_some_and(|t| t.contains(&id));
    let mut line = with_default_color(
        description,
        if curse {
            NamedColor::Red
        } else {
            NamedColor::Gray
        },
    );
    let max_level = entry
        .and_then(|e| e.data)
        .and_then(|d| d.get_i32("max_level"))
        .unwrap_or(1);
    if level != 1 || max_level != 1 {
        line = line
            .with(Component::text(" "))
            .with(tr(format!("enchantment.level.{level}"), Vec::new()));
    }
    line
}

/// Attribute modifiers per slot group, the item's then its enchantments', as `ItemStack#forEachModifier`.
fn attribute_lines(item: &ItemRef<'_>, src: &dyn TooltipSource, lines: &mut Lines) {
    let patched = match item.get("attribute_modifiers") {
        Some(DataComponent::AttributeModifiers(m)) => Some(&m.0),
        _ => None,
    };
    let use_defaults = patched.is_none() && !item.removed("attribute_modifiers");
    for slot in SLOT_GROUPS {
        let mut group: Vec<Component> = Vec::new();
        if let Some(entries) = patched {
            for e in entries.iter().take(MAX_SCANNED).filter(|e| e.slot == slot) {
                let Some(attribute) = static_name("minecraft:attribute", e.attribute) else {
                    continue;
                };
                match &e.display {
                    AttributeDisplay::Hidden => continue,
                    AttributeDisplay::Override(text) => group.push(text.clone()),
                    AttributeDisplay::Default => group.extend(modifier_line(
                        attribute,
                        e.modifier.id.as_str(),
                        e.modifier.amount,
                        e.modifier.operation,
                    )),
                }
            }
        }
        if use_defaults {
            for m in item
                .defaults
                .modifiers
                .iter()
                .filter(|m| m.slot == slot && !m.hidden)
            {
                group.extend(modifier_line(&m.attribute, &m.id, m.amount, m.operation));
            }
        }
        if let Some(e) = item.enchantments() {
            for level in e.0.iter().take(MAX_SCANNED) {
                enchantment_modifiers(level.enchantment, level.level, slot, src, &mut group);
            }
        }
        group.truncate(MAX_TOOLTIP_LINES);
        if !group.is_empty() {
            lines.empty();
            lines.push(gray(tr(
                format!("item.modifiers.{}", slot_group_name(slot)),
                Vec::new(),
            )));
            for line in group {
                lines.push(line);
            }
        }
    }
}

/// `LevelBasedValue` for the constant and linear forms vanilla's enchantments use.
fn level_value(v: &Value, level: i32) -> Option<f64> {
    if let Some(n) = nbt_number(v) {
        return Some(n);
    }
    let Value::Compound(c) = v else {
        return None;
    };
    match c.get_str("type").map(path_of) {
        Some("linear") => {
            let base = nbt_number(c.get("base")?)?;
            let per = nbt_number(c.get("per_level_above_first")?)?;
            Some(base + per * (f64::from(level) - 1.0))
        }
        Some("constant") => nbt_number(c.get("value")?),
        _ => None,
    }
}

fn enchantment_modifiers(
    id: i32,
    level: i32,
    slot: EquipmentSlotGroup,
    src: &dyn TooltipSource,
    out: &mut Vec<Component>,
) {
    let Some(data) = src.entry("minecraft:enchantment", id).and_then(|e| e.data) else {
        return;
    };
    let slots: Vec<EquipmentSlotGroup> = match data.get("slots") {
        Some(Value::List(List::String(s))) => s.iter().filter_map(|s| slot_group(s)).collect(),
        Some(Value::String(s)) => slot_group(s).into_iter().collect(),
        _ => Vec::new(),
    };
    if !slots.contains(&slot) {
        return;
    }
    let Some(Value::List(List::Compound(effects))) = data
        .get_compound("effects")
        .and_then(|e| e.get("minecraft:attributes"))
    else {
        return;
    };
    for effect in effects.iter().take(16) {
        let (Some(attribute), Some(amount)) = (
            effect.get_str("attribute"),
            effect.get("amount").and_then(|v| level_value(v, level)),
        ) else {
            continue;
        };
        let op = match effect.get_str("operation") {
            Some("add_multiplied_base") => AttributeOperation::AddMultipliedBase,
            Some("add_multiplied_total") => AttributeOperation::AddMultipliedTotal,
            _ => AttributeOperation::AddValue,
        };
        out.extend(modifier_line(
            attribute,
            effect.get_str("id").unwrap_or(""),
            amount,
            op,
        ));
    }
}

/// `AdventureModePredicate#computeTooltip`: the distinct block names, or "unknown" for an open predicate.
fn predicate_lines(p: &AdventureModePredicate, src: &dyn TooltipSource, lines: &mut Lines) {
    if p.0.iter().any(|b| b.blocks.is_none()) {
        lines.push(gray(tr("item.canUse.unknown", Vec::new())));
        return;
    }
    let mut seen: Vec<i32> = Vec::new();
    for predicate in &p.0 {
        let ids: &[i32] = match &predicate.blocks {
            Some(HolderSet::Ids(ids)) => ids,
            Some(HolderSet::Tag(tag)) => src.tag("minecraft:block", tag.as_str()).unwrap_or(&[]),
            None => &[],
        };
        for &id in ids {
            if seen.len() >= MAX_BLOCK_NAMES {
                return;
            }
            if !seen.contains(&id) {
                seen.push(id);
                if let Some(name) = static_name("minecraft:block", id) {
                    lines.push(colored(
                        tr(description_key("block", name), Vec::new()),
                        NamedColor::DarkGray,
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::{Identifier, VarInt};
    use mcr_protocol::types::component::{
        AttributeModifier, AttributeModifierEntry, EnchantmentLevel, ItemAttributeModifiers,
        PotionContents, TooltipDisplay,
    };
    use mcr_protocol::types::text::Content;

    use super::*;

    struct Registries {
        enchantments: Vec<(&'static str, Compound)>,
    }

    impl Registries {
        fn new() -> Self {
            let entry = |key: &str, max: i32| {
                let mut c = Compound::new();
                c.insert("description", tr(key, Vec::new()).to_nbt());
                c.insert("max_level", Value::Int(max));
                c
            };
            Self {
                enchantments: vec![
                    (
                        "minecraft:sharpness",
                        entry("enchantment.minecraft.sharpness", 5),
                    ),
                    (
                        "minecraft:binding_curse",
                        entry("enchantment.minecraft.binding_curse", 1),
                    ),
                    (
                        "minecraft:mending",
                        entry("enchantment.minecraft.mending", 1),
                    ),
                ],
            }
        }
    }

    impl TooltipSource for Registries {
        fn entry(&self, registry: &str, id: i32) -> Option<RegistryRef<'_>> {
            if registry != "minecraft:enchantment" {
                return None;
            }
            let (name, data) = self.enchantments.get(usize::try_from(id).ok()?)?;
            Some(RegistryRef {
                id,
                name,
                data: Some(data),
            })
        }

        fn tag(&self, registry: &str, tag: &str) -> Option<&[i32]> {
            match (registry, tag) {
                ("minecraft:enchantment", "minecraft:curse") => Some(&[1]),
                ("minecraft:enchantment", "minecraft:tooltip_order") => Some(&[1, 2, 0]),
                _ => None,
            }
        }
    }

    /// Text and translation keys with their arguments, colours as `<colour>`.
    fn describe(c: &Component) -> String {
        let mut out = String::new();
        if let Some(Color::Named(n)) = c.style.color {
            out.push_str(&format!("<{}>", n.name()));
        }
        if c.style.italic == Some(true) {
            out.push_str("<i>");
        }
        match &c.content {
            Content::Text(t) => out.push_str(t),
            Content::Translatable { key, args, .. } => {
                out.push_str(key);
                if !args.is_empty() {
                    let args: Vec<String> = args
                        .iter()
                        .map(|a| match a {
                            Arg::Str(s) => s.clone(),
                            Arg::Component(c) => describe(c),
                            Arg::Number(v) => format!("{v:?}"),
                        })
                        .collect();
                    out.push_str(&format!("({})", args.join(", ")));
                }
            }
            _ => out.push('?'),
        }
        for e in &c.extra {
            out.push_str(&describe(e));
        }
        out
    }

    fn id(name: &str) -> i32 {
        mcr_data::v26_3().item_by_name(name).unwrap() as i32
    }

    fn stack(name: &str, components: Vec<DataComponent>) -> ItemStack {
        let mut s = ItemStack::new(id(name), 1);
        s.components.added = components;
        s
    }

    fn lines(s: &ItemStack, flags: TooltipFlags) -> Vec<String> {
        tooltip_lines(&ItemRef::of_stack(s), &Registries::new(), flags)
            .iter()
            .map(describe)
            .collect()
    }

    fn type_id(name: &str) -> VarInt {
        VarInt(component_type_id(name).unwrap())
    }

    #[test]
    fn enchanted_sword_lists_ordered_enchantments_then_base_modifiers() {
        let s = stack(
            "minecraft:diamond_sword",
            vec![DataComponent::Enchantments(ItemEnchantments(vec![
                EnchantmentLevel {
                    enchantment: 0,
                    level: 5,
                },
                EnchantmentLevel {
                    enchantment: 1,
                    level: 1,
                },
            ]))],
        );
        assert_eq!(
            lines(&s, TooltipFlags::default()),
            [
                "<aqua>item.minecraft.diamond_sword",
                "<red>enchantment.minecraft.binding_curse",
                "<gray>enchantment.minecraft.sharpness enchantment.level.5",
                "",
                "<gray>item.modifiers.mainhand",
                "<dark_green> attribute.modifier.equals.0(7, attribute.name.attack_damage)",
                "<dark_green> attribute.modifier.equals.0(1.6, attribute.name.attack_speed)",
            ]
        );
    }

    #[test]
    fn enchanted_book_lists_stored_enchantments() {
        let s = stack(
            "minecraft:enchanted_book",
            vec![DataComponent::StoredEnchantments(ItemEnchantments(vec![
                EnchantmentLevel {
                    enchantment: 2,
                    level: 1,
                },
            ]))],
        );
        assert_eq!(
            lines(&s, TooltipFlags::default()),
            [
                "<aqua>item.minecraft.enchanted_book",
                "<gray>enchantment.minecraft.mending",
            ]
        );
    }

    #[test]
    fn potion_names_its_contents_and_lists_effects_with_attributes() {
        let potion = mcr_data::v26_3()
            .registry("minecraft:potion")
            .unwrap()
            .id_of("minecraft:strong_swiftness")
            .unwrap() as i32;
        let s = stack(
            "minecraft:potion",
            vec![DataComponent::PotionContents(PotionContents {
                potion: Some(VarInt(potion)),
                custom_color: None,
                custom_effects: Vec::new(),
                custom_name: None,
            })],
        );
        assert_eq!(
            lines(&s, TooltipFlags::default()),
            [
                "<white>item.minecraft.potion.effect.swiftness",
                "<blue>potion.withDuration(potion.withAmplifier(effect.minecraft.speed, potion.potency.1), 01:30)",
                "",
                "<dark_purple>potion.whenDrank",
                "<blue>attribute.modifier.plus.2(40, attribute.name.movement_speed)",
            ]
        );
        let lingering = stack("minecraft:lingering_potion", Vec::new());
        assert_eq!(
            lines(&lingering, TooltipFlags::default()),
            [
                "<white>item.minecraft.lingering_potion.effect.empty",
                "<gray>effect.none"
            ]
        );
    }

    #[test]
    fn renamed_dyed_armor_is_italic_and_says_dyed() {
        let s = stack(
            "minecraft:leather_chestplate",
            vec![
                DataComponent::CustomName(Component::text("Coat")),
                DataComponent::DyedColor(0x12_34_56),
            ],
        );
        let normal = lines(&s, TooltipFlags::default());
        assert_eq!(normal[0], "<white><i>Coat");
        assert_eq!(normal[1], "<gray><i>item.dyed");
        assert_eq!(normal[2], "");
        assert_eq!(normal[3], "<gray>item.modifiers.chest");
        assert_eq!(
            normal[4],
            "<blue>attribute.modifier.plus.0(3, attribute.name.armor)"
        );
        let advanced = lines(
            &s,
            TooltipFlags {
                advanced: true,
                creative: false,
            },
        );
        assert_eq!(advanced[1], "<gray>item.color(#123456)");
        assert_eq!(
            advanced[advanced.len() - 2],
            "<dark_gray>minecraft:leather_chestplate"
        );
        assert!(
            advanced
                .last()
                .unwrap()
                .starts_with("<dark_gray>item.components(")
        );
    }

    #[test]
    fn tooltip_display_hides_components_and_the_whole_tooltip() {
        let attribute = mcr_data::v26_3()
            .registry("minecraft:attribute")
            .unwrap()
            .id_of("minecraft:max_health")
            .unwrap() as i32;
        let modifiers = DataComponent::AttributeModifiers(ItemAttributeModifiers(vec![
            AttributeModifierEntry {
                attribute,
                modifier: AttributeModifier {
                    id: Identifier::new("minecraft", "extra").unwrap(),
                    amount: -2.0,
                    operation: AttributeOperation::AddValue,
                },
                slot: EquipmentSlotGroup::OffHand,
                display: AttributeDisplay::Default,
            },
        ]));
        let enchants = DataComponent::Enchantments(ItemEnchantments(vec![EnchantmentLevel {
            enchantment: 2,
            level: 1,
        }]));
        let s = stack("minecraft:stick", vec![modifiers.clone(), enchants.clone()]);
        assert_eq!(
            lines(&s, TooltipFlags::default()),
            [
                "<aqua>item.minecraft.stick",
                "<gray>enchantment.minecraft.mending",
                "",
                "<gray>item.modifiers.offhand",
                "<red>attribute.modifier.take.0(2, attribute.name.max_health)",
            ]
        );
        let hidden = DataComponent::TooltipDisplay(TooltipDisplay {
            hide_tooltip: false,
            hidden_components: vec![type_id("enchantments"), type_id("attribute_modifiers")],
        });
        let s = stack("minecraft:stick", vec![modifiers, enchants, hidden]);
        assert_eq!(
            lines(&s, TooltipFlags::default()),
            ["<aqua>item.minecraft.stick"]
        );
        let gone = DataComponent::TooltipDisplay(TooltipDisplay {
            hide_tooltip: true,
            hidden_components: Vec::new(),
        });
        let s = stack("minecraft:stick", vec![gone]);
        assert!(lines(&s, TooltipFlags::default()).is_empty());
        let creative = TooltipFlags {
            advanced: false,
            creative: true,
        };
        assert_eq!(lines(&s, creative), ["<white>item.minecraft.stick"]);
    }

    #[test]
    fn advanced_lines_show_durability_id_and_component_count() {
        let s = stack(
            "minecraft:iron_pickaxe",
            vec![DataComponent::Damage(VarInt(10))],
        );
        let advanced = TooltipFlags {
            advanced: true,
            creative: false,
        };
        let out = lines(&s, advanced);
        let n = out.len();
        assert_eq!(out[n - 3], "item.durability(240, 250)");
        assert_eq!(out[n - 2], "<dark_gray>minecraft:iron_pickaxe");
        let count = ItemRef::of_stack(&s).component_count();
        assert_eq!(out[n - 1], format!("<dark_gray>item.components({count})"));
        assert!(count > 10);
        assert!(
            !lines(&s, TooltipFlags::default())
                .iter()
                .any(|l| l.contains("durability"))
        );
    }

    #[test]
    fn extreme_server_values_neither_panic_nor_overflow_the_line_cap() {
        use mcr_protocol::types::component::{
            Fireworks, MapPostProcessing, MobEffectDetails, SuspiciousStewEffect,
        };
        let details = |amplifier, duration| MobEffectDetails {
            amplifier,
            duration,
            ambient: false,
            show_particles: true,
            show_icon: true,
            hidden_effect: None,
        };
        let effects: Vec<MobEffectInstance> = [i32::MIN, -1, 0, i32::MAX]
            .into_iter()
            .flat_map(|a| {
                [i32::MIN, -2, -1, 0, i32::MAX].map(|d| MobEffectInstance {
                    effect: 0,
                    details: details(a, d),
                })
            })
            .collect();
        let levels: Vec<EnchantmentLevel> = (0..1000)
            .flat_map(|i| {
                [i32::MIN, -1, 0, 1, i32::MAX].map(|level| EnchantmentLevel {
                    enchantment: i % 4 - 1,
                    level,
                })
            })
            .collect();
        let attribute = |amount: f64| AttributeModifierEntry {
            attribute: 0,
            modifier: AttributeModifier {
                id: Identifier::new("minecraft", "base_attack_damage").unwrap(),
                amount,
                operation: AttributeOperation::AddMultipliedTotal,
            },
            slot: EquipmentSlotGroup::Any,
            display: AttributeDisplay::Default,
        };
        let modifiers: Vec<_> = [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            f64::MIN,
        ]
        .into_iter()
        .cycle()
        .take(2000)
        .map(attribute)
        .collect();
        let components = vec![
            DataComponent::Enchantments(ItemEnchantments(levels.clone())),
            DataComponent::StoredEnchantments(ItemEnchantments(levels)),
            DataComponent::PotionContents(PotionContents {
                potion: Some(VarInt(i32::MAX)),
                custom_color: None,
                custom_effects: effects.clone(),
                custom_name: None,
            }),
            DataComponent::PotionDurationScale(f32::NAN),
            DataComponent::OminousBottleAmplifier(VarInt(i32::MIN)),
            DataComponent::SuspiciousStewEffects(
                (0..1000)
                    .map(|i| SuspiciousStewEffect {
                        effect: i,
                        duration: i32::MIN + i,
                    })
                    .collect(),
            ),
            DataComponent::AttributeModifiers(ItemAttributeModifiers(modifiers)),
            DataComponent::Damage(VarInt(i32::MAX)),
            DataComponent::MaxDamage(VarInt(i32::MAX)),
            DataComponent::MapId(VarInt(i32::MIN)),
            DataComponent::MapPostProcessing(MapPostProcessing::Scale),
            DataComponent::Fireworks(Fireworks {
                flight_duration: i32::MIN,
                explosions: Default::default(),
            }),
            DataComponent::TropicalFishPattern(VarInt(i32::MIN)),
        ];
        struct HugeMap(Registries);
        impl TooltipSource for HugeMap {
            fn entry(&self, registry: &str, id: i32) -> Option<RegistryRef<'_>> {
                self.0.entry(registry, id)
            }
            fn tag(&self, registry: &str, tag: &str) -> Option<&[i32]> {
                self.0.tag(registry, tag)
            }
            fn map_info(&self, _id: i32) -> Option<MapInfo> {
                Some(MapInfo {
                    scale: i8::MAX,
                    locked: true,
                })
            }
            fn tick_rate(&self) -> f32 {
                f32::NAN
            }
        }
        let src = HugeMap(Registries::new());
        for name in [
            "minecraft:potion",
            "minecraft:suspicious_stew",
            "minecraft:filled_map",
        ] {
            let s = stack(name, components.clone());
            for flags in [
                TooltipFlags::default(),
                TooltipFlags {
                    advanced: true,
                    creative: true,
                },
            ] {
                let out = tooltip_lines(&ItemRef::of_stack(&s), &src, flags);
                assert!(!out.is_empty() && out.len() <= MAX_TOOLTIP_LINES);
            }
        }
        let extreme = [i32::MIN, -1, 0, i32::MAX];
        for v in extreme {
            let _ = format_ticks(v, f32::NAN);
            let _ = format_ticks(v, f32::INFINITY);
            assert!(level_value(&Value::Int(1), v).is_some());
        }
        let mut linear = Compound::new();
        linear.insert("type", Value::String("minecraft:linear".into()));
        linear.insert("base", Value::Double(f64::MAX));
        linear.insert("per_level_above_first", Value::Double(f64::MAX));
        for v in extreme {
            assert!(level_value(&Value::Compound(linear.clone()), v).is_some());
        }
        assert_eq!(format_amount(f64::NAN), "NaN");
    }

    #[test]
    fn hostile_lore_is_bounded() {
        let lore = mcr_protocol::types::component::ItemLore(mcr_buf::LengthPrefixed(
            (0..256).map(|i| Component::text(i.to_string())).collect(),
        ));
        let s = stack("minecraft:stick", vec![DataComponent::Lore(lore)]);
        assert_eq!(lines(&s, TooltipFlags::default()).len(), MAX_TOOLTIP_LINES);
    }

    #[test]
    fn amounts_format_like_decimal_format() {
        assert_eq!(format_amount(0.15), "0.15");
        assert_eq!(format_amount(1.5999999046325684), "1.6");
        assert_eq!(format_amount(0.125), "0.12");
        assert_eq!(format_amount(20.000000298023224), "20");
        assert_eq!(format_ticks(1800, 20.0), "01:30");
        assert_eq!(format_ticks(72_000 * 2, 20.0), "02:00:00");
    }
}
