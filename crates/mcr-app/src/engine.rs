use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use mcr_render::glam::DVec3;
use mcr_render::{Camera, FrameStats, Renderer};

use crate::assets::Assets;
use crate::game::Game;
use crate::render_system::{PipelineStats, RenderSystem};
use crate::source::{AnvilSource, WorldSource};

const LOADER_THREADS: usize = 3;

/// One game state plus the system drawing it; a network client plugs in a different `WorldSource`.
#[derive(Debug)]
pub struct Engine {
    pub game: Game,
    pub render: RenderSystem,
}

impl Engine {
    pub fn new(
        renderer: Renderer,
        assets: Arc<Assets>,
        source: Box<dyn WorldSource>,
        camera: Option<Camera>,
        render_distance: i32,
    ) -> Self {
        let camera = camera.unwrap_or_else(|| {
            let [x, y, z] = source.spawn().unwrap_or([0.0, 100.0, 0.0]);
            Camera {
                position: DVec3::new(x, y + 24.0, z),
                pitch: -0.3,
                ..Camera::default()
            }
        });
        let game = Game::new(source, camera, render_distance);
        let render = RenderSystem::new(renderer, assets, &game);
        Self { game, render }
    }

    pub fn open_anvil(path: &Path) -> Result<Box<dyn WorldSource>> {
        Ok(Box::new(AnvilSource::open(path, LOADER_THREADS)?))
    }

    pub fn tick(&mut self) -> Result<()> {
        self.game.update();
        self.render.update(&mut self.game)
    }

    pub fn draw(&mut self) -> Option<FrameStats> {
        self.render.draw(&self.game.camera, 1.0)
    }

    pub fn settled(&self) -> bool {
        self.render.settled(&self.game)
    }

    pub fn pipeline_stats(&self) -> PipelineStats {
        self.render.stats()
    }
}
