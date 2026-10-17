//! Where a passenger sits: the vehicle's passenger attachment minus the passenger's own vehicle attachment.

use mcr_data::Version;
use mcr_protocol::types::entity_data::EntityDataValue;

use crate::entity::Entity;

const BABY_INDEX: u8 = 16;
const PIGLIN_BABY_INDEX: u8 = 17;
const CAMEL_LAST_POSE_INDEX: u8 = 20;
const DEFAULT_AGE_SCALE: f32 = 0.5;
const CAMEL_BABY_SCALE: f32 = 0.6;
const HAPPY_GHAST_BABY_SCALE: f32 = 0.2375;
const PLAYER_VEHICLE_ATTACHMENT: f32 = 0.6;
const MAX_SCALE: f32 = 16.0;

type Point = [f32; 3];

const fn up(y: f32) -> Point {
    [0.0, y, 0.0]
}

const MINECART: &[Point] = &[up(0.1875)];

/// Adult passenger attachments from the entity types; types not listed attach at their full height.
const PASSENGER: &[(&str, &[Point])] = &[
    ("cat", &[up(0.5125)]),
    ("chest_minecart", MINECART),
    ("command_block_minecart", MINECART),
    ("furnace_minecart", MINECART),
    ("hopper_minecart", MINECART),
    ("minecart", MINECART),
    ("spawner_minecart", MINECART),
    ("tnt_minecart", MINECART),
    ("chicken", &[[0.0, 0.7, -0.1]]),
    ("cow", &[up(1.36875)]),
    ("mooshroom", &[up(1.36875)]),
    ("donkey", &[up(1.1125)]),
    ("goat", &[up(1.1125)]),
    ("horse", &[up(1.44375)]),
    ("mule", &[up(1.2125)]),
    ("skeleton_horse", &[up(1.31875)]),
    ("zombie_horse", &[up(1.31875)]),
    ("llama", &[[0.0, 1.37, -0.3]]),
    ("trader_llama", &[[0.0, 1.37, -0.3]]),
    ("pig", &[up(0.86875)]),
    ("sheep", &[up(1.2375)]),
    ("hoglin", &[up(1.49375)]),
    ("zoglin", &[up(1.49375)]),
    ("fox", &[[0.0, 0.6375, -0.25]]),
    ("frog", &[[0.0, 0.375, -0.25]]),
    ("ocelot", &[up(0.6375)]),
    ("parrot", &[up(0.4625)]),
    ("spider", &[up(0.765)]),
    ("turtle", &[[0.0, 0.55625, -0.25]]),
    ("wolf", &[[0.0, 0.81875, -0.0625]]),
    ("sniffer", &[up(2.09375)]),
    ("ravager", &[[0.0, 2.2625, -0.0625]]),
    ("nautilus", &[up(1.1375)]),
    ("zombie_nautilus", &[up(1.1375)]),
    ("enderman", &[up(2.80625)]),
    ("ender_dragon", &[up(3.0)]),
    ("ghast", &[up(4.0625)]),
    (
        "happy_ghast",
        &[
            [0.0, 4.0, 1.7],
            [-1.7, 4.0, 0.0],
            [0.0, 4.0, -1.7],
            [1.7, 4.0, 0.0],
        ],
    ),
    ("warden", &[up(3.15)]),
    ("witch", &[up(2.2625)]),
    ("guardian", &[up(0.975)]),
    ("elder_guardian", &[up(2.350625)]),
    ("endermite", &[up(0.2375)]),
    ("silverfish", &[up(0.2375)]),
    ("vex", &[up(0.7375)]),
    ("phantom", &[up(0.3375)]),
    ("drowned", &[up(2.0125)]),
    ("husk", &[up(2.075)]),
    ("zombie", &[up(2.0125)]),
    ("zombie_villager", &[up(2.125)]),
    ("piglin", &[up(2.0125)]),
    ("piglin_brute", &[up(2.0125)]),
    ("zombified_piglin", &[up(2.0)]),
    ("evoker", &[up(2.0)]),
    ("illusioner", &[up(2.0)]),
    ("pillager", &[up(2.0)]),
    ("vindicator", &[up(2.0)]),
];

/// Babies whose dimensions carry their own attachments: `(type, points, scale applied to them)`.
/// An empty point list means the adult points; a type missing here shrinks the adult points by 0.5.
const BABY_PASSENGER: &[(&str, &[Point], f32)] = &[
    ("horse", &[up(1.475)], 0.7),
    ("donkey", &[[0.0, 1.53125, -0.3125]], 0.5),
    ("mule", &[[0.0, 1.63125, -0.3125]], 0.5),
    ("skeleton_horse", &[up(1.35)], 0.7),
    ("zombie_horse", &[up(1.35)], 0.7),
    ("llama", &[[0.0, 1.62, -0.3]], 0.5),
    ("trader_llama", &[[0.0, 1.62, -0.3]], 0.5),
    ("camel", &[], CAMEL_BABY_SCALE),
    ("camel_husk", &[], CAMEL_BABY_SCALE),
    ("happy_ghast", &[], HAPPY_GHAST_BABY_SCALE),
    ("pig", &[up(0.5)], 1.0),
    ("cow", &[up(0.75)], 1.0),
    ("mooshroom", &[up(0.75)], 1.0),
    ("sheep", &[up(0.5625)], 1.0),
    ("goat", &[up(0.53125)], 1.0),
    ("chicken", &[up(0.375)], 1.0),
    ("cat", &[up(0.3125)], 1.0),
    ("ocelot", &[up(0.3125)], 1.0),
    ("wolf", &[up(0.4375)], 1.0),
    ("fox", &[up(0.375)], 1.0),
    ("strider", &[up(0.65625)], 1.0),
    ("hoglin", &[up(0.875)], 1.0),
    ("zoglin", &[up(0.875)], 1.0),
    ("nautilus", &[up(0.5)], 1.0),
    ("turtle", &[[0.0, 0.4, -0.25]], 0.3),
];

/// Passenger types whose feet hang below the attachment: the vehicle attachment is this high above the feet.
const RIDE: &[(&str, f32)] = &[
    ("player", PLAYER_VEHICLE_ATTACHMENT),
    ("zombie", 0.7),
    ("husk", 0.7),
    ("drowned", 0.7),
    ("skeleton", 0.7),
    ("stray", 0.7),
    ("bogged", 0.7),
    ("parched", 0.7),
    ("piglin", 0.7),
    ("piglin_brute", 0.7),
    ("zombified_piglin", 0.7),
    ("zombie_villager", 0.7),
    ("zoglin", 0.7),
    ("wither_skeleton", 0.875),
    ("giant", 3.75),
    ("evoker", 0.6),
    ("illusioner", 0.6),
    ("pillager", 0.6),
    ("vindicator", 0.6),
    ("chicken", 0.0),
    ("phantom", 0.125),
    ("allay", -0.04),
    ("vex", -0.04),
    ("ghast", -0.5),
    ("happy_ghast", -0.5),
];

/// Babies of the zombie family sit lower: `(type, vehicle attachment height)`.
const BABY_RIDE: &[(&str, f32)] = &[
    ("zombie", 0.1875),
    ("husk", 0.1875),
    ("drowned", 0.1875),
    ("piglin", 0.1875),
    ("zombified_piglin", 0.1875),
    ("zombie_villager", 0.125),
];

#[derive(Debug, Clone, Copy)]
pub struct Vehicle<'a> {
    pub name: &'a str,
    pub height: f32,
    pub baby: bool,
    pub scale: f32,
    pub yaw: f32,
    pub passengers: usize,
    pub camel_sitting: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Rider<'a> {
    pub name: &'a str,
    pub baby: bool,
    pub scale: f32,
    pub animal: bool,
}

impl<'a> Vehicle<'a> {
    pub fn of(data: &'a Version, e: &Entity, scale: f32) -> Self {
        let name = type_name(data, e.kind);
        let height = u32::try_from(e.kind)
            .ok()
            .and_then(|k| data.entity_type_info(k))
            .map_or(1.0, |i| i.height);
        Self {
            name,
            height,
            baby: is_baby(e, name),
            scale,
            yaw: e.yaw,
            passengers: e.passengers.len(),
            camel_sitting: matches!(
                e.data.get(&CAMEL_LAST_POSE_INDEX),
                Some(EntityDataValue::Long(v)) if *v < 0
            ),
        }
    }
}

impl<'a> Rider<'a> {
    pub fn of(data: &'a Version, e: &Entity, scale: f32) -> Self {
        let name = type_name(data, e.kind);
        let category = u32::try_from(e.kind)
            .ok()
            .and_then(|k| data.entity_type_info(k))
            .map_or("", |i| i.category);
        Self {
            name,
            baby: is_baby(e, name),
            scale,
            animal: category == "creature" && name != "wandering_trader",
        }
    }

    pub fn player(scale: f32) -> Self {
        Self {
            name: "player",
            baby: false,
            scale,
            animal: false,
        }
    }
}

fn type_name(data: &Version, kind: i32) -> &str {
    let name = u32::try_from(kind)
        .ok()
        .and_then(|k| data.entity_type_name(k))
        .unwrap_or("");
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn is_baby(e: &Entity, name: &str) -> bool {
    let index = if name.starts_with("piglin") && name != "piglin_brute" {
        PIGLIN_BABY_INDEX
    } else {
        BABY_INDEX
    };
    matches!(e.data.get(&index), Some(EntityDataValue::Boolean(true)))
}

fn sane(scale: f32) -> f32 {
    if scale.is_finite() {
        scale.clamp(0.0, MAX_SCALE)
    } else {
        1.0
    }
}

fn is_boat(name: &str) -> bool {
    name.ends_with("_boat") || name.ends_with("_raft")
}

fn rotate(x: f32, z: f32, yaw_degrees: f32) -> [f64; 2] {
    let (sin, cos) = yaw_degrees.to_radians().sin_cos();
    [f64::from(x * cos - z * sin), f64::from(z * cos + x * sin)]
}

fn passenger_points(v: &Vehicle<'_>) -> (Vec<Point>, f32) {
    let adult = PASSENGER
        .iter()
        .find(|(n, _)| *n == v.name)
        .map_or_else(|| vec![up(v.height)], |(_, p)| p.to_vec());
    if !v.baby {
        return (adult, 1.0);
    }
    match BABY_PASSENGER.iter().find(|(n, _, _)| *n == v.name) {
        Some((_, [], scale)) => (adult, *scale),
        Some((_, points, scale)) => (points.to_vec(), *scale),
        None => (adult, DEFAULT_AGE_SCALE),
    }
}

fn camel_height(v: &Vehicle<'_>, scale: f32) -> f32 {
    let sit = if v.baby { 0.09375 } else { 0.375 };
    let bottom = scale * 0.2;
    let rest = v.height * scale - sit;
    if v.camel_sitting { rest + bottom } else { rest }
}

fn boat_seat(v: &Vehicle<'_>, index: usize, rider: &Rider<'_>) -> Point {
    let y = if v.name.ends_with("_raft") {
        v.height * 0.888_888_9
    } else {
        v.height / 3.0
    };
    let mut z = if v.name.contains("chest") { 0.15 } else { 0.0 };
    if v.passengers > 1 {
        z = if index == 0 { 0.2 } else { -0.6 };
        if rider.animal {
            z += 0.2;
        }
    }
    [0.0, y, z]
}

fn camel_seat(v: &Vehicle<'_>, index: usize, rider: &Rider<'_>, scale: f32) -> Point {
    let mut z = 0.5;
    if v.passengers > 1 {
        if index != 0 {
            z = -0.7;
        }
        if rider.animal {
            z += 0.2;
        }
    }
    [0.0, camel_height(v, scale), z * scale]
}

/// The passenger attachment on the vehicle, rotated into world axes.
fn attachment(v: &Vehicle<'_>, index: usize, rider: &Rider<'_>) -> [f64; 3] {
    let scale = sane(v.scale);
    let age = if v.baby && v.name.starts_with("camel") {
        CAMEL_BABY_SCALE
    } else {
        1.0
    };
    let point = if is_boat(v.name) {
        boat_seat(v, index, rider)
    } else if v.name.starts_with("camel") {
        camel_seat(v, index, rider, scale * age)
    } else if v.name.ends_with("minecart") && matches!(rider.name, "villager" | "wandering_trader")
    {
        [0.0; 3]
    } else {
        let (points, baby_scale) = passenger_points(v);
        let p = points[index.min(points.len().saturating_sub(1))];
        let k = baby_scale * scale;
        [p[0] * k, p[1] * k, p[2] * k]
    };
    let [x, z] = rotate(point[0], point[2], v.yaw);
    [x, f64::from(point[1]), z]
}

fn vehicle_attachment_height(rider: &Rider<'_>) -> f32 {
    let table = if rider.baby {
        BABY_RIDE
            .iter()
            .find(|(n, _)| *n == rider.name)
            .map(|(_, h)| *h)
    } else {
        None
    };
    let adult = || {
        RIDE.iter()
            .find(|(n, _)| *n == rider.name)
            .map_or(0.0, |(_, h)| *h)
    };
    table.unwrap_or_else(|| {
        if rider.baby {
            adult() * DEFAULT_AGE_SCALE
        } else {
            adult()
        }
    }) * sane(rider.scale)
}

/// Offset of the passenger's feet from the vehicle position, in world axes.
pub fn offset(vehicle: &Vehicle<'_>, seat: usize, rider: &Rider<'_>) -> [f64; 3] {
    let mut o = attachment(vehicle, seat, rider);
    o[1] -= f64::from(vehicle_attachment_height(rider));
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle(name: &str, height: f32) -> Vehicle<'_> {
        Vehicle {
            name,
            height,
            baby: false,
            scale: 1.0,
            yaw: 0.0,
            passengers: 1,
            camel_sitting: false,
        }
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-4, "{a} != {b}");
    }

    #[test]
    fn adult_horse_seat() {
        let o = offset(&vehicle("horse", 1.6), 0, &Rider::player(1.0));
        close(o[1], 1.44375 - 0.6);
    }

    #[test]
    fn baby_horse_uses_its_own_attachment_scaled() {
        let v = Vehicle {
            baby: true,
            ..vehicle("horse", 1.6)
        };
        close(offset(&v, 0, &Rider::player(1.0))[1], 1.475 * 0.7 - 0.6);
    }

    #[test]
    fn baby_without_override_shrinks_adult_attachment() {
        let v = Vehicle {
            baby: true,
            ..vehicle("sniffer", 1.75)
        };
        close(offset(&v, 0, &Rider::player(1.0))[1], 2.09375 * 0.5 - 0.6);
    }

    #[test]
    fn boat_single_and_double() {
        let mut b = vehicle("oak_boat", 0.5625);
        close(offset(&b, 0, &Rider::player(1.0))[1], 0.1875 - 0.6);
        close(offset(&b, 0, &Rider::player(1.0))[2], 0.0);
        b.passengers = 2;
        close(offset(&b, 0, &Rider::player(1.0))[2], 0.2);
        close(offset(&b, 1, &Rider::player(1.0))[2], -0.6);
        let pig = Rider {
            name: "pig",
            baby: false,
            scale: 1.0,
            animal: true,
        };
        close(offset(&b, 1, &pig)[2], -0.4);
    }

    #[test]
    fn boat_seats_turn_with_the_yaw() {
        let mut b = vehicle("oak_boat", 0.5625);
        b.passengers = 2;
        b.yaw = 90.0;
        let o = offset(&b, 0, &Rider::player(1.0));
        close(o[0], -0.2);
        close(o[2], 0.0);
    }

    #[test]
    fn raft_and_chest_boat() {
        close(
            offset(&vehicle("bamboo_raft", 0.5625), 0, &Rider::player(1.0))[1],
            0.5625 * 0.888_888_9 - 0.6,
        );
        close(
            offset(&vehicle("oak_chest_boat", 0.5625), 0, &Rider::player(1.0))[2],
            0.15,
        );
    }

    #[test]
    fn camel_two_seats_and_sitting() {
        let mut c = vehicle("camel", 2.375);
        c.passengers = 2;
        let front = offset(&c, 0, &Rider::player(1.0));
        let back = offset(&c, 1, &Rider::player(1.0));
        close(front[1], 2.0 - 0.6);
        close(front[2], 0.5);
        close(back[2], -0.7);
        c.camel_sitting = true;
        close(offset(&c, 0, &Rider::player(1.0))[1], 2.2 - 0.6);
    }

    #[test]
    fn baby_camel_scales_heights() {
        let mut c = vehicle("camel", 2.375);
        c.baby = true;
        close(
            offset(&c, 0, &Rider::player(1.0))[1],
            2.375 * 0.6 - 0.09375 - 0.6,
        );
        close(offset(&c, 0, &Rider::player(1.0))[2], 0.5 * 0.6);
    }

    #[test]
    fn minecart_and_villager() {
        let m = vehicle("minecart", 0.7);
        close(offset(&m, 0, &Rider::player(1.0))[1], 0.1875 - 0.6);
        let villager = Rider {
            name: "villager",
            baby: false,
            scale: 1.0,
            animal: false,
        };
        close(offset(&m, 0, &villager)[1], 0.0);
    }

    #[test]
    fn pig_seat() {
        close(
            offset(&vehicle("pig", 0.9), 0, &Rider::player(1.0))[1],
            0.86875 - 0.6,
        );
    }

    #[test]
    fn happy_ghast_has_four_seats_and_clamps() {
        let mut g = vehicle("happy_ghast", 4.0);
        g.passengers = 4;
        let o = offset(&g, 1, &Rider::player(1.0));
        close(o[0], -1.7);
        close(o[1], 4.0 - 0.6);
        close(offset(&g, 9, &Rider::player(1.0))[0], 1.7);
    }

    #[test]
    fn hostile_scale_and_height_stay_finite() {
        let mut v = vehicle("horse", f32::NAN);
        v.scale = f32::INFINITY;
        v.yaw = f32::NAN;
        let o = offset(&v, usize::MAX, &Rider::player(f32::NAN));
        assert!(o.iter().all(|c| c.is_nan() || c.is_finite()));
    }
}
