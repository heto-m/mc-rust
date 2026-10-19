//! Translucent resorting: which sections need their quads reordered for the camera, and a worker thread
//! that sorts them off the render thread.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::quad::{ModelQuad, sort_back_to_front};

/// Vanilla's `TranslucencyPointOfView`: where the camera sits relative to a section, per axis
/// clamped to -1..=1.
pub(crate) type PointOfView = [i8; 3];

/// Never equal to a real point of view, so a mesh without a sort camera is sorted when first seen.
pub(crate) const UNSORTED: PointOfView = [i8::MAX; 3];

pub(crate) fn point_of_view(camera_section: [i32; 3], section: [i32; 3]) -> PointOfView {
    std::array::from_fn(|a| {
        (i64::from(camera_section[a]) - i64::from(section[a])).clamp(-1, 1) as i8
    })
}

pub(crate) fn axis_aligned(pov: PointOfView) -> bool {
    pov.contains(&0)
}

/// The camera's section, from world coordinates.
pub(crate) fn section_of(position: [f64; 3]) -> [i32; 3] {
    position.map(|v| (v.floor() as i32) >> 4)
}

/// What the renderer keeps of a section's translucent quads to sort them again.
#[derive(Debug)]
pub(crate) struct TranslucentQuads {
    pub quads: Arc<[ModelQuad]>,
    /// First model arena slot the quads occupy.
    pub base: u32,
    /// Changes with every mesh, so a result for replaced quads is dropped.
    pub generation: u64,
    pub pov: PointOfView,
    pub pending: bool,
}

pub(crate) struct SortJob {
    pub key: [i32; 3],
    pub generation: u64,
    pub quads: Arc<[ModelQuad]>,
    /// Camera position relative to the section's minimum corner.
    pub eye: [f32; 3],
}

pub(crate) struct Sorted {
    pub key: [i32; 3],
    pub generation: u64,
    pub quads: Vec<ModelQuad>,
}

/// One background thread; it exits when the renderer drops its sender.
pub(crate) struct Sorter {
    jobs: Sender<SortJob>,
    done: Receiver<Sorted>,
}

impl std::fmt::Debug for Sorter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sorter").finish_non_exhaustive()
    }
}

impl Sorter {
    pub fn spawn() -> Self {
        let (jobs, inbox) = channel::<SortJob>();
        let (outbox, done) = channel();
        std::thread::Builder::new()
            .name("translucent sort".into())
            .spawn(move || {
                let mut scratch = Vec::new();
                for job in inbox {
                    let mut quads = job.quads.to_vec();
                    sort_back_to_front(&mut quads, job.eye, &mut scratch);
                    let sorted = Sorted {
                        key: job.key,
                        generation: job.generation,
                        quads,
                    };
                    if outbox.send(sorted).is_err() {
                        return;
                    }
                }
            })
            .expect("spawn translucent sort thread");
        Self { jobs, done }
    }

    pub fn submit(&self, job: SortJob) {
        let _ = self.jobs.send(job);
    }

    pub fn finished(&self) -> impl Iterator<Item = Sorted> + '_ {
        self.done.try_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_of_view_clamps_per_axis() {
        assert_eq!(point_of_view([5, 0, -9], [0, 0, 0]), [1, 0, -1]);
        assert!(axis_aligned([1, 0, -1]));
        assert!(!axis_aligned([1, 1, -1]));
        assert_eq!(section_of([-0.5, 15.9, 16.0]), [-1, 0, 1]);
        assert_eq!(point_of_view([i32::MAX; 3], [i32::MIN; 3]), [1; 3]);
        assert_eq!(
            section_of([f64::MAX, f64::NAN, -1e300]),
            [i32::MAX >> 4, 0, i32::MIN >> 4]
        );
    }
}
