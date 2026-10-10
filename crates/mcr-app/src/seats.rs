//! Where a passenger sits on its vehicle: the vehicle's passenger attachment minus the passenger's own ride offset.

/// `(vehicle type, attachment height, attachment depth)`; types not listed attach at their full height.
const ATTACHMENTS: &[(&str, f32, f32)] = &[
    ("cat", 0.5125, 0.0),
    ("chest_minecart", 0.1875, 0.0),
    ("command_block_minecart", 0.1875, 0.0),
    ("furnace_minecart", 0.1875, 0.0),
    ("hopper_minecart", 0.1875, 0.0),
    ("minecart", 0.1875, 0.0),
    ("spawner_minecart", 0.1875, 0.0),
    ("tnt_minecart", 0.1875, 0.0),
    ("chicken", 0.7, -0.1),
    ("cow", 1.36875, 0.0),
    ("mooshroom", 1.36875, 0.0),
    ("donkey", 1.1125, 0.0),
    ("goat", 1.1125, 0.0),
    ("horse", 1.44375, 0.0),
    ("mule", 1.2125, 0.0),
    ("skeleton_horse", 1.31875, 0.0),
    ("zombie_horse", 1.31875, 0.0),
    ("llama", 1.37, -0.3),
    ("trader_llama", 1.37, -0.3),
    ("pig", 0.86875, 0.0),
    ("sheep", 1.2375, 0.0),
    ("hoglin", 1.49375, 0.0),
    ("zoglin", 1.49375, 0.0),
    ("fox", 0.6375, -0.25),
    ("frog", 0.375, -0.25),
    ("ocelot", 0.6375, 0.0),
    ("parrot", 0.4625, 0.0),
    ("spider", 0.765, 0.0),
    ("turtle", 0.55625, -0.25),
    ("wolf", 0.81875, -0.0625),
    ("sniffer", 2.09375, 0.0),
    ("ravager", 2.2625, -0.0625),
    ("nautilus", 1.1375, 0.0),
    ("zombie_nautilus", 1.1375, 0.0),
    ("enderman", 2.80625, 0.0),
    ("ender_dragon", 3.0, 0.0),
    ("ghast", 4.0625, 0.0),
    ("warden", 3.15, 0.0),
    ("witch", 2.2625, 0.0),
    ("guardian", 0.975, 0.0),
    ("elder_guardian", 2.350625, 0.0),
    ("endermite", 0.2375, 0.0),
    ("silverfish", 0.2375, 0.0),
    ("vex", 0.7375, 0.0),
    ("phantom", 0.3375, 0.0),
];

/// `(passenger type, height the passenger's hips sit above its feet)`.
const RIDE: &[(&str, f32)] = &[
    ("player", 0.6),
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
    ("evoker", 0.6),
    ("illusioner", 0.6),
    ("pillager", 0.6),
    ("vindicator", 0.6),
    ("chicken", 0.0),
    ("phantom", 0.125),
    ("allay", -0.04),
    ("vex", -0.04),
];

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn is_boat(name: &str) -> bool {
    name.ends_with("_boat") || name.ends_with("_raft")
}

/// Offset of the passenger's feet from the vehicle position, in world axes.
pub fn offset(vehicle: &str, vehicle_height: f32, vehicle_yaw: f32, passenger: &str) -> [f64; 3] {
    let (vehicle, passenger) = (short(vehicle), short(passenger));
    let (mut y, mut depth) = ATTACHMENTS
        .iter()
        .find(|(n, _, _)| *n == vehicle)
        .map_or((vehicle_height, 0.0), |(_, y, z)| (*y, *z));
    if is_boat(vehicle) {
        depth = if vehicle.contains("chest") { 0.15 } else { 0.0 };
    }
    let ride = RIDE
        .iter()
        .find(|(n, _)| *n == passenger)
        .map_or(0.0, |(_, h)| *h);
    y -= ride;
    let yaw = vehicle_yaw.to_radians();
    [
        f64::from(-depth * yaw.sin()),
        f64::from(y),
        f64::from(depth * yaw.cos()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_on_a_pig_sits_below_the_attachment() {
        let o = offset("minecraft:pig", 0.9, 0.0, "minecraft:player");
        assert!((o[1] - (0.86875 - 0.6)).abs() < 1e-5);
    }

    #[test]
    fn boats_use_their_full_height_and_unknown_types_are_safe() {
        let o = offset("minecraft:oak_boat", 0.5625, 0.0, "minecraft:player");
        assert!((o[1] - (0.5625 - 0.6)).abs() < 1e-5);
        let o = offset("minecraft:nope", f32::NAN, f32::INFINITY, "");
        assert!(o.iter().all(|v| v.is_nan() || v.is_finite()));
    }

    #[test]
    fn depth_follows_the_vehicle_yaw() {
        let o = offset("minecraft:llama", 1.87, 90.0, "minecraft:player");
        assert!((o[0] - 0.3).abs() < 1e-5 && o[2].abs() < 1e-5);
    }
}
