use crate::data::DyeColor;

/// Block faces in vanilla `get3DDataValue` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dir {
    Down,
    Up,
    North,
    #[default]
    South,
    West,
    East,
}

impl Dir {
    pub fn from_name(s: &str) -> Option<Dir> {
        Some(match s {
            "down" => Dir::Down,
            "up" => Dir::Up,
            "north" => Dir::North,
            "south" => Dir::South,
            "west" => Dir::West,
            "east" => Dir::East,
            _ => return None,
        })
    }

    pub fn from_data3d(v: u8) -> Dir {
        [
            Dir::Down,
            Dir::Up,
            Dir::North,
            Dir::South,
            Dir::West,
            Dir::East,
        ][usize::from(v.min(5))]
    }

    /// Vanilla `Direction.toYRot` (south 0, west 90, north 180, east 270).
    pub fn yrot(self) -> f32 {
        match self {
            Dir::South | Dir::Up | Dir::Down => 0.0,
            Dir::West => 90.0,
            Dir::North => 180.0,
            Dir::East => 270.0,
        }
    }

    pub fn step(self) -> [i32; 3] {
        match self {
            Dir::Down => [0, -1, 0],
            Dir::Up => [0, 1, 0],
            Dir::North => [0, 0, -1],
            Dir::South => [0, 0, 1],
            Dir::West => [-1, 0, 0],
            Dir::East => [1, 0, 0],
        }
    }

    pub fn opposite(self) -> Dir {
        match self {
            Dir::Down => Dir::Up,
            Dir::Up => Dir::Down,
            Dir::North => Dir::South,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
            Dir::East => Dir::West,
        }
    }

    pub fn data2d(self) -> i32 {
        match self {
            Dir::South | Dir::Up | Dir::Down => 0,
            Dir::West => 1,
            Dir::North => 2,
            Dir::East => 3,
        }
    }

    pub fn from_data2d(v: i32) -> Dir {
        [Dir::South, Dir::West, Dir::North, Dir::East][v.rem_euclid(4) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChestType {
    #[default]
    Single,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Oxidation {
    #[default]
    None,
    Exposed,
    Weathered,
    Oxidized,
}

/// Block state facts the renderers need, resolved once when an entry is set.
#[derive(Debug, Clone, Default)]
pub struct Placement {
    pub block: Box<str>,
    pub facing: Dir,
    pub rotation: u8,
    pub wall: bool,
    pub chest_type: ChestType,
    pub color: Option<DyeColor>,
    pub powered: bool,
    pub has_book: bool,
    pub dusted: u8,
    pub pose: Box<str>,
    pub oxidation: Oxidation,
}

fn dye_prefix(name: &str) -> Option<DyeColor> {
    (0..16u8)
        .map(DyeColor)
        .filter(|c| {
            name.strip_prefix(c.name())
                .is_some_and(|rest| rest.starts_with('_'))
        })
        .max_by_key(|c| c.name().len())
}

fn oxidation(name: &str) -> Oxidation {
    let name = name.strip_prefix("waxed_").unwrap_or(name);
    if name.starts_with("exposed_") {
        Oxidation::Exposed
    } else if name.starts_with("weathered_") {
        Oxidation::Weathered
    } else if name.starts_with("oxidized_") {
        Oxidation::Oxidized
    } else {
        Oxidation::None
    }
}

impl Placement {
    pub fn from_state(state: u32) -> Placement {
        let version = mcr_data::v26_3();
        let name = version
            .state_block_name(state)
            .map_or("air", |n| n.strip_prefix("minecraft:").unwrap_or(n));
        let mut p = Placement {
            block: name.into(),
            wall: name.contains("wall"),
            color: dye_prefix(name),
            oxidation: oxidation(name),
            ..Placement::default()
        };
        for (key, value) in version.state_properties(state) {
            match key {
                "facing" => p.facing = Dir::from_name(value).unwrap_or_default(),
                "rotation" => p.rotation = value.parse::<u8>().unwrap_or(0) & 15,
                "type" => {
                    p.chest_type = match value {
                        "left" => ChestType::Left,
                        "right" => ChestType::Right,
                        _ => ChestType::Single,
                    }
                }
                "powered" => p.powered = value == "true",
                "has_book" => p.has_book = value == "true",
                "dusted" => p.dusted = value.parse().unwrap_or(0),
                "copper_golem_pose" => p.pose = value.into(),
                _ => {}
            }
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dye_prefix_prefers_longest() {
        assert_eq!(
            dye_prefix("light_blue_banner"),
            DyeColor::from_name("light_blue")
        );
        assert_eq!(dye_prefix("blue_shulker_box"), DyeColor::from_name("blue"));
        assert_eq!(dye_prefix("shulker_box"), None);
    }

    #[test]
    fn chest_state_properties() {
        let v = mcr_data::v26_3();
        let chest = v.block_by_name("minecraft:chest").unwrap();
        let (first, end) = v.block_info(chest).unwrap().states;
        let left = (first..end)
            .find(|&s| {
                v.state_properties(s)
                    .any(|(k, val)| k == "type" && val == "left")
            })
            .unwrap();
        assert_eq!(Placement::from_state(left).chest_type, ChestType::Left);
    }
}
