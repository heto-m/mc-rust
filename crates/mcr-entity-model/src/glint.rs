//! The enchantment glint's scroll and strength, from the `glintSpeed` and `glintStrength` options.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

const DEFAULT_SPEED: f32 = 0.5;
const DEFAULT_STRENGTH: f32 = 0.75;
/// The glint tint's alpha at the default strength.
const DEFAULT_ALPHA: f32 = 150.0;

static SPEED: AtomicU32 = AtomicU32::new(DEFAULT_SPEED.to_bits());
static STRENGTH: AtomicU32 = AtomicU32::new(DEFAULT_STRENGTH.to_bits());
static START: OnceLock<Instant> = OnceLock::new();

/// Like vanilla's `GameRenderer`, every glint in the game follows the options.
pub fn set_options(speed: f32, strength: f32) {
    SPEED.store(speed.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    STRENGTH.store(strength.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

fn load(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}

/// Texture scroll after `millis` of wall-clock time; vanilla animates the glint by time, not age.
pub fn scroll_at(millis: f64, speed: f32) -> [f32; 2] {
    let ticks = millis / 50.0 * f64::from(speed / DEFAULT_SPEED);
    [
        (ticks * 0.01).fract() as f32,
        (ticks * 0.007).fract() as f32,
    ]
}

pub(crate) fn scroll() -> [f32; 2] {
    let millis = START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0;
    scroll_at(millis, load(&SPEED))
}

pub fn tint_for(strength: f32) -> [u8; 4] {
    let alpha = (DEFAULT_ALPHA * strength / DEFAULT_STRENGTH)
        .round()
        .clamp(0.0, 255.0) as u8;
    [160, 100, 255, alpha]
}

pub(crate) fn tint() -> [u8; 4] {
    tint_for(load(&STRENGTH))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_scales_the_scroll_and_strength_the_alpha() {
        assert_eq!(scroll_at(5_000.0, 0.0), [0.0, 0.0]);
        let slow = scroll_at(1_000.0, 0.25);
        let fast = scroll_at(500.0, 0.5);
        assert!((slow[0] - fast[0]).abs() < 1e-6);
        assert_eq!(tint_for(0.75)[3], 150);
        assert_eq!(tint_for(0.0)[3], 0);
        assert_eq!(tint_for(1.0)[3], 200);
    }
}
