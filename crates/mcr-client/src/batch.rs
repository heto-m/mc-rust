use std::time::Instant;

const MAX_OLD_SAMPLES_WEIGHT: u32 = 49;
const CLAMP_COEFFICIENT: f64 = 3.0;
const INITIAL_NANOS_PER_CHUNK: f64 = 2_000_000.0;
const TARGET_NANOS_PER_TICK: f64 = 7_000_000.0;

/// Paces the server's chunk stream like vanilla: an exponential average of nanoseconds per chunk.
#[derive(Debug, Clone)]
pub struct ChunkBatchSizeCalculator {
    aggregated_nanos_per_chunk: f64,
    old_samples_weight: u32,
    batch_start: Option<Instant>,
}

impl Default for ChunkBatchSizeCalculator {
    fn default() -> Self {
        Self {
            aggregated_nanos_per_chunk: INITIAL_NANOS_PER_CHUNK,
            old_samples_weight: 1,
            batch_start: None,
        }
    }
}

impl ChunkBatchSizeCalculator {
    pub fn on_batch_start(&mut self, now: Instant) {
        self.batch_start = Some(now);
    }

    pub fn on_batch_finished(&mut self, batch_size: i32, now: Instant) {
        let (Some(start), true) = (self.batch_start.take(), batch_size > 0) else {
            return;
        };
        let nanos = now.saturating_duration_since(start).as_nanos() as f64 / f64::from(batch_size);
        self.add_sample(nanos);
    }

    fn add_sample(&mut self, nanos_per_chunk: f64) {
        let avg = self.aggregated_nanos_per_chunk;
        let clamped = nanos_per_chunk.clamp(avg / CLAMP_COEFFICIENT, avg * CLAMP_COEFFICIENT);
        let weight = f64::from(self.old_samples_weight);
        self.aggregated_nanos_per_chunk = (avg * weight + clamped) / (weight + 1.0);
        self.old_samples_weight = (self.old_samples_weight + 1).min(MAX_OLD_SAMPLES_WEIGHT);
    }

    pub fn desired_chunks_per_tick(&self) -> f32 {
        (TARGET_NANOS_PER_TICK / self.aggregated_nanos_per_chunk) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn initial_rate_is_three_and_a_half() {
        let calc = ChunkBatchSizeCalculator::default();
        assert!((calc.desired_chunks_per_tick() - 3.5).abs() < 1e-6);
    }

    #[test]
    fn fast_batches_raise_the_rate_within_clamp() {
        let mut calc = ChunkBatchSizeCalculator::default();
        let t0 = Instant::now();
        calc.on_batch_start(t0);
        calc.on_batch_finished(10, t0 + Duration::from_micros(10));
        let expected = 7_000_000.0 / ((2_000_000.0 + 2_000_000.0 / 3.0) / 2.0);
        assert!((f64::from(calc.desired_chunks_per_tick()) - expected).abs() < 1e-3);
    }

    #[test]
    fn empty_batch_is_ignored() {
        let mut calc = ChunkBatchSizeCalculator::default();
        let t0 = Instant::now();
        calc.on_batch_start(t0);
        calc.on_batch_finished(0, t0 + Duration::from_secs(1));
        assert!((calc.desired_chunks_per_tick() - 3.5).abs() < 1e-6);
    }

    #[test]
    fn rate_is_always_finite_and_positive() {
        let mut calc = ChunkBatchSizeCalculator::default();
        for _ in 0..200 {
            calc.add_sample(0.0);
        }
        assert!(calc.desired_chunks_per_tick().is_finite() && calc.desired_chunks_per_tick() > 0.0);
    }
}
