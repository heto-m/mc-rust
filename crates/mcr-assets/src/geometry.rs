#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Face {
    pub const ALL: [Face; 6] = [
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
        Face::West,
        Face::East,
    ];

    pub fn from_name(name: &str) -> Option<Face> {
        Some(match name {
            "down" => Face::Down,
            "up" => Face::Up,
            "north" => Face::North,
            "south" => Face::South,
            "west" => Face::West,
            "east" => Face::East,
            _ => return None,
        })
    }

    pub const fn normal(self) -> [i8; 3] {
        match self {
            Face::Down => [0, -1, 0],
            Face::Up => [0, 1, 0],
            Face::North => [0, 0, -1],
            Face::South => [0, 0, 1],
            Face::West => [-1, 0, 0],
            Face::East => [1, 0, 0],
        }
    }

    pub fn nearest(v: [f32; 3]) -> Face {
        let mut best = Face::North;
        let mut best_dot = f32::MIN_POSITIVE;
        for face in Face::ALL {
            let n = face.normal();
            let dot = v[0] * f32::from(n[0]) + v[1] * f32::from(n[1]) + v[2] * f32::from(n[2]);
            if dot > best_dot {
                best_dot = dot;
                best = face;
            }
        }
        best
    }
}

/// Signed-permutation matrix (a rotation of the cube), applied as `M * v`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rot([[i8; 3]; 3]);

impl Rot {
    pub const IDENTITY: Rot = Rot([[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
    const X1: Rot = Rot([[1, 0, 0], [0, 0, 1], [0, -1, 0]]);
    const Y1: Rot = Rot([[0, 0, -1], [0, 1, 0], [1, 0, 0]]);
    const Z1: Rot = Rot([[0, 1, 0], [-1, 0, 0], [0, 0, 1]]);

    fn pow(self, quarter_turns: u8) -> Rot {
        (0..quarter_turns % 4).fold(Rot::IDENTITY, |acc, _| self.mul(acc))
    }

    /// Variant rotations: x first, then y, then z, each a clockwise quarter-turn count.
    pub fn from_variant(x: u8, y: u8, z: u8) -> Rot {
        Rot::Z1.pow(z).mul(Rot::Y1.pow(y)).mul(Rot::X1.pow(x))
    }

    pub fn is_identity(self) -> bool {
        self == Rot::IDENTITY
    }

    pub fn mul(self, rhs: Rot) -> Rot {
        let mut out = [[0i8; 3]; 3];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.0[i][k] * rhs.0[k][j]).sum();
            }
        }
        Rot(out)
    }

    pub fn transpose(self) -> Rot {
        let m = self.0;
        Rot([
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ])
    }

    pub fn apply_int(self, v: [i8; 3]) -> [i8; 3] {
        let m = self.0;
        std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
    }

    pub fn apply(self, v: [f32; 3]) -> [f32; 3] {
        let m = self.0;
        std::array::from_fn(|i| {
            f32::from(m[i][0]) * v[0] + f32::from(m[i][1]) * v[1] + f32::from(m[i][2]) * v[2]
        })
    }

    pub fn face(self, face: Face) -> Face {
        let n = self.apply_int(face.normal());
        Face::nearest([f32::from(n[0]), f32::from(n[1]), f32::from(n[2])])
    }

    /// Maps the face-local (south-facing) frame into world space.
    fn local_to_global(face: Face) -> Rot {
        match face {
            Face::South => Rot::IDENTITY,
            Face::East => Rot::Y1.pow(3),
            Face::West => Rot::Y1,
            Face::North => Rot::Y1.pow(2),
            Face::Up => Rot::X1,
            Face::Down => Rot::X1.pow(3),
        }
    }

    /// Linear map taking centered texture coordinates of `face` back to their unrotated place.
    pub fn uv_inverse(self, face: Face) -> Rot {
        if self.is_identity() {
            return self;
        }
        let action = self.mul(Rot::local_to_global(face));
        let new_face = Face::nearest(action.apply([0.0, 0.0, 1.0]));
        Rot::local_to_global(new_face)
            .transpose()
            .mul(action)
            .transpose()
    }

    pub fn apply_uv(self, u: f32, v: f32) -> [f32; 2] {
        let (a, b) = (u - 0.5, v - 0.5);
        let m = self.0;
        [
            f32::from(m[0][0]) * a + f32::from(m[0][1]) * b + 0.5,
            f32::from(m[1][0]) * a + f32::from(m[1][1]) * b + 0.5,
        ]
    }
}

pub type Mat3 = [[f32; 3]; 3];

pub fn mat3_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

pub fn mat3_apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

pub fn rotation_axis_angle(axis: usize, degrees: f32) -> Mat3 {
    let (s, c) = degrees.to_radians().sin_cos();
    match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
        1 => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
        _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
    }
}

pub fn is_identity_f(m: &Mat3) -> bool {
    (0..3).all(|i| (0..3).all(|j| (m[i][j] - f32::from(u8::from(i == j))).abs() < 1e-6))
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y90_turns_north_to_east() {
        let r = Rot::from_variant(0, 1, 0);
        assert_eq!(r.face(Face::North), Face::East);
        assert_eq!(r.face(Face::Up), Face::Up);
    }

    #[test]
    fn x90_turns_up_to_north() {
        let r = Rot::from_variant(1, 0, 0);
        assert_eq!(r.face(Face::Up), Face::North);
    }

    #[test]
    fn four_quarter_turns_is_identity() {
        assert!(Rot::from_variant(4, 4, 4).is_identity());
        assert_eq!(
            Rot::from_variant(0, 1, 0).transpose(),
            Rot::from_variant(0, 3, 0)
        );
    }

    #[test]
    fn uvlock_on_top_face_counter_rotates() {
        let r = Rot::from_variant(0, 1, 0);
        let m = r.uv_inverse(Face::Up);
        let [u, v] = m.apply_uv(1.0, 0.5);
        assert!(
            (u - 0.5).abs() < 1e-6 && (v - 0.0).abs() < 1e-6
                || (u - 0.5).abs() < 1e-6 && (v - 1.0).abs() < 1e-6
        );
    }
}
