use std::sync::OnceLock;

const SIN_SCALE: f64 = 10430.378350470453;

fn table() -> &'static [f32; 65536] {
    static TABLE: OnceLock<Box<[f32; 65536]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = Box::new([0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = (i as f64 / SIN_SCALE).sin() as f32;
        }
        t
    })
}

pub fn sin(x: f64) -> f32 {
    table()[((x * SIN_SCALE) as i64 & 65535) as usize]
}

pub fn cos(x: f64) -> f32 {
    table()[((x * SIN_SCALE + 16384.0) as i64 & 65535) as usize]
}

pub const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_endpoints() {
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), table()[16384]);
        assert!((sin(std::f64::consts::FRAC_PI_2) - 1.0).abs() < 1e-4);
        assert!((cos(std::f64::consts::PI) + 1.0).abs() < 1e-4);
    }
}
