//! The local player model in the inventory screen, turning toward the mouse.

use std::sync::Arc;

use mcr_entity_model::glam::{Mat4, Vec3, Vec4};
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    EntityScene, EntityView, FrameParams, LightInfo, SceneAssets, TargetFormats,
};
use mcr_render::OverlayCtx;

use crate::entities::SyncedRegistries;

const MODEL_SCALE_PX: f32 = 30.0;
const CENTER_OFFSET: f32 = 0.0625;
const PLAYER_HEIGHT: f32 = 1.8;
const DEPTH_RANGE: f32 = 10.0;

pub struct Preview {
    scene: EntityScene,
    scissor: Option<[u32; 4]>,
}

impl std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preview").finish_non_exhaustive()
    }
}

/// Orthographic matrix placing a feet-origin model in `rect` (GUI pixels), tilted like vanilla's
/// `renderEntityInInventoryFollowsMouse`.
pub fn preview_matrix(
    rect: [i32; 4],
    gui_scale: u32,
    mouse: (f32, f32),
    fb: (u32, u32),
) -> (Mat4, f32, f32) {
    let gs = gui_scale as f32;
    let cx = rect[0] as f32 + rect[2] as f32 / 2.0;
    let cy = rect[1] as f32 + rect[3] as f32 / 2.0;
    let ax = ((cx - mouse.0) / 40.0).atan();
    let ay = ((cy - mouse.1) / 40.0).atan();
    let (w, h) = (fb.0.max(1) as f32, fb.1.max(1) as f32);
    let s = MODEL_SCALE_PX * gs;
    let ortho = Mat4::from_cols(
        Vec4::new(s / (w / 2.0), 0.0, 0.0, 0.0),
        Vec4::new(0.0, s / (h / 2.0), 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0 / DEPTH_RANGE, 0.0),
        Vec4::new(
            (cx * gs - w / 2.0) / (w / 2.0),
            -(cy * gs - h / 2.0) / (h / 2.0),
            0.5,
            1.0,
        ),
    );
    let centre = Vec3::new(0.0, PLAYER_HEIGHT / 2.0 + CENTER_OFFSET, 0.0);
    let tilt = Mat4::from_rotation_x(-ay * 20f32.to_radians());
    (ortho * tilt * Mat4::from_translation(-centre), ax, ay)
}

impl Preview {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: wgpu::TextureFormat,
        assets: Arc<SceneAssets>,
    ) -> Option<Self> {
        let formats = TargetFormats {
            color,
            depth: wgpu::TextureFormat::Depth32Float,
            reverse_z: true,
            sample_count: 1,
        };
        let scene = EntityScene::new(device, queue, formats, assets)
            .map_err(|e| eprintln!("preview: {e}"))
            .ok()?;
        Some(Self {
            scene,
            scissor: None,
        })
    }

    pub fn hide(&mut self) {
        self.scissor = None;
    }

    pub fn prepare(
        &mut self,
        mut view: EntityView<'_>,
        registries: &mcr_client::Registries,
        rect: [i32; 4],
        gui_scale: u32,
        mouse: (f32, f32),
        fb: (u32, u32),
    ) {
        let (matrix, ax, ay) = preview_matrix(rect, gui_scale, mouse, fb);
        view.pos = [0.0; 3];
        view.body_yaw = ax * 20.0;
        view.yaw = ax * 40.0;
        view.head_yaw = view.yaw;
        view.pitch = -ay * 20.0;
        view.light = [15, 15];
        view.limb_swing = 0.0;
        view.limb_amount = 0.0;
        self.scene
            .update(&SyncedRegistries(registries), std::iter::once(view));
        self.scene.prepare(&FrameParams {
            view_proj: matrix,
            camera: [0.0; 3],
            light: LightInfo::with_brightness(1.0),
        });
        let gs = gui_scale as i32;
        let clamp = |v: i32, max: u32| v.clamp(0, max as i32) as u32;
        let (x0, y0) = (clamp(rect[0] * gs, fb.0), clamp(rect[1] * gs, fb.1));
        let (x1, y1) = (
            clamp((rect[0] + rect[2]) * gs, fb.0),
            clamp((rect[1] + rect[3]) * gs, fb.1),
        );
        self.scissor = (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0]);
    }

    pub fn record(&self, ctx: &mut OverlayCtx<'_>) {
        let Some(r) = self.scissor else {
            return;
        };
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("inventory-player"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_scissor_rect(r[0], r[1], r[2], r[3]);
        self.scene.draw(&mut pass);
    }
}

#[cfg(test)]
mod follow_tests {
    use super::*;

    #[test]
    fn model_centre_lands_in_rect_centre() {
        let rect = [26, 8, 49, 70];
        let (m, _, _) = preview_matrix(rect, 2, (50.0, 43.0), (1000, 800));
        let clip = m * Vec4::new(0.0, 0.9625, 0.0, 1.0);
        let (px, py) = ((clip.x * 0.5 + 0.5) * 1000.0, (0.5 - clip.y * 0.5) * 800.0);
        assert!((px - (26.0 + 24.5) * 2.0).abs() < 0.5, "{px}");
        assert!((py - (8.0 + 35.0) * 2.0).abs() < 0.5, "{py}");
        assert!((0.0..=1.0).contains(&clip.z));
    }

    #[test]
    fn head_is_above_feet_on_screen() {
        let (m, _, _) = preview_matrix([0, 0, 50, 70], 1, (25.0, 35.0), (320, 240));
        let feet = m * Vec4::new(0.0, 0.0, 0.0, 1.0);
        let head = m * Vec4::new(0.0, 1.8, 0.0, 1.0);
        assert!(head.y > feet.y);
    }
}

#[cfg(test)]
mod follow_bounds {
    use super::*;

    #[test]
    fn follow_angles_are_bounded_like_vanilla() {
        for mouse in [(0.0, 0.0), (1.0e9, -1.0e9), (f32::MAX, f32::MIN)] {
            let (_, ax, ay) = preview_matrix([0, 0, 50, 70], 2, mouse, (1280, 720));
            assert!((180.0 + ax * 40.0 - 180.0).abs() <= 63.0);
            assert!((ay * 20.0).abs() <= 32.0);
        }
    }
}
