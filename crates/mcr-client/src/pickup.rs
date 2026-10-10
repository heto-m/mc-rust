use crate::entity::Entity;

pub const PICKUP_TICKS: u8 = 3;
pub const MAX_PICKUPS: usize = 256;

/// A collected entity flying to its collector, as vanilla's `ItemPickupParticle`.
#[derive(Debug, Clone)]
pub struct Pickup {
    pub entity: Entity,
    pub collector: i32,
    pub life: u8,
    start: [f64; 3],
    target: [f64; 3],
    target_old: [f64; 3],
}

impl Pickup {
    pub(crate) fn new(entity: Entity, collector: i32, target: [f64; 3]) -> Self {
        Self {
            start: entity.pos,
            entity,
            collector,
            life: 0,
            target,
            target_old: target,
        }
    }

    pub(crate) fn advance(&mut self, target: Option<[f64; 3]>) {
        self.life += 1;
        self.target_old = self.target;
        if let Some(t) = target {
            self.target = t;
        }
    }

    pub(crate) fn finished(&self) -> bool {
        self.life >= PICKUP_TICKS
    }

    pub fn position(&self, partial: f32) -> [f64; 3] {
        let partial = f64::from(partial);
        let time = (f64::from(self.life) + partial) / f64::from(PICKUP_TICKS);
        let time = time * time;
        std::array::from_fn(|i| {
            let target = self.target_old[i] + (self.target[i] - self.target_old[i]) * partial;
            self.start[i] + (target - self.start[i]) * time
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn item_flies_to_target_in_three_ticks() {
        let e = Entity::new(1, Uuid::nil(), 0, [0.0, 0.0, 0.0]);
        let mut p = Pickup::new(e, 2, [3.0, 0.0, 0.0]);
        assert_eq!(p.position(0.0), [0.0; 3]);
        p.advance(None);
        let x = p.position(0.0)[0];
        assert!((x - 3.0 / 9.0).abs() < 1e-12);
        p.advance(None);
        assert!((p.position(0.0)[0] - 3.0 * 4.0 / 9.0).abs() < 1e-12);
        p.advance(None);
        assert!(p.finished());
        assert!((p.position(0.0)[0] - 3.0).abs() < 1e-12);
    }
}
