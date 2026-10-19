//! 3D pictures in screens: the local player in the inventory turning toward the mouse, the smithing
//! armour stand and the enchanting book.

use std::sync::Arc;

use mcr_block_entity::{BlockEntityFrame, BlockEntityScene};
use mcr_entity_model::glam::{Mat4, Quat, Vec3, Vec4};
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    EntityScene, EntityView, FrameParams, LightInfo, SceneAssets, TargetFormats,
};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_render::OverlayCtx;
use rustc_hash::FxHashMap;

use crate::entities::SyncedRegistries;

const MODEL_SCALE_PX: f32 = 30.0;
const CENTER_OFFSET: f32 = 0.0625;
const PLAYER_HEIGHT: f32 = 1.8;
const DEPTH_RANGE: f32 = 10.0;
const STAND_SCALE_PX: f32 = 25.0;
const STAND_BODY_YAW: f32 = 30.0;
const STAND_TILT_DEG: f32 = 25.0;
const STAND_SHOW_ARMS_NO_BASEPLATE: i8 = 4 | 8;
const BOOK_SCALE_PX: f32 = 40.0;
const BOOK_TOP_PX: f32 = 17.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Entity,
    Book,
}

pub struct Preview {
    scene: EntityScene,
    book: Option<BlockEntityScene>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    formats: TargetFormats,
    assets: Arc<SceneAssets>,
    stand_data: FxHashMap<u8, EntityDataValue>,
    stand_equipment: [ItemStack; 8],
    scissor: Option<([u32; 4], Shown)>,
}

impl std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preview").finish_non_exhaustive()
    }
}

/// Orthographic projection of model units onto the framebuffer, `scale_px` GUI pixels per block
/// with the model origin at `centre` (GUI pixels).
fn gui_ortho(centre: (f32, f32), scale_px: f32, gui_scale: u32, fb: (u32, u32)) -> Mat4 {
    let gs = gui_scale as f32;
    let (w, h) = (fb.0.max(1) as f32, fb.1.max(1) as f32);
    let s = scale_px * gs;
    Mat4::from_cols(
        Vec4::new(s / (w / 2.0), 0.0, 0.0, 0.0),
        Vec4::new(0.0, s / (h / 2.0), 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0 / DEPTH_RANGE, 0.0),
        Vec4::new(
            (centre.0 * gs - w / 2.0) / (w / 2.0),
            -(centre.1 * gs - h / 2.0) / (h / 2.0),
            0.5,
            1.0,
        ),
    )
}

fn rect_centre(rect: [i32; 4]) -> (f32, f32) {
    (
        rect[0] as f32 + rect[2] as f32 / 2.0,
        rect[1] as f32 + rect[3] as f32 / 2.0,
    )
}

/// Orthographic matrix placing a feet-origin model in `rect` (GUI pixels), tilted like vanilla's
/// `renderEntityInInventoryFollowsMouse`.
pub fn preview_matrix(
    rect: [i32; 4],
    gui_scale: u32,
    mouse: (f32, f32),
    fb: (u32, u32),
) -> (Mat4, f32, f32) {
    let (cx, cy) = rect_centre(rect);
    let ax = ((cx - mouse.0) / 40.0).atan();
    let ay = ((cy - mouse.1) / 40.0).atan();
    let ortho = gui_ortho((cx, cy), MODEL_SCALE_PX, gui_scale, fb);
    let centre = Vec3::new(0.0, PLAYER_HEIGHT / 2.0 + CENTER_OFFSET, 0.0);
    let tilt = Mat4::from_rotation_x(-ay * 20f32.to_radians());
    (ortho * tilt * Mat4::from_translation(-centre), ax, ay)
}

/// `SmithingScreen`'s armour stand: one block up from the rect centre to the feet, tilted toward the viewer.
pub fn armor_stand_matrix(rect: [i32; 4], gui_scale: u32, fb: (u32, u32)) -> Mat4 {
    let ortho = gui_ortho(rect_centre(rect), STAND_SCALE_PX, gui_scale, fb);
    ortho
        * Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0))
        * Mat4::from_rotation_x(STAND_TILT_DEG.to_radians())
}

/// `GuiBookModelRenderer`'s pose for a book `open` in 0..=1, in our y-up, z-toward-viewer space.
pub fn book_matrix(rect: [i32; 4], gui_scale: u32, fb: (u32, u32), open: f32) -> Mat4 {
    let centre = (
        rect[0] as f32 + rect[2] as f32 / 2.0,
        rect[1] as f32 + BOOK_TOP_PX,
    );
    let shut = 1.0 - open;
    gui_ortho(centre, BOOK_SCALE_PX, gui_scale, fb)
        * Mat4::from_rotation_z(std::f32::consts::PI)
        * Mat4::from_rotation_x(25f32.to_radians())
        * Mat4::from_translation(Vec3::new(shut * 0.2, shut * 0.1, shut * 0.25))
        * Mat4::from_rotation_y((-shut * 90.0 - 90.0).to_radians())
        * Mat4::from_rotation_x(std::f32::consts::PI)
}

fn frac(x: f32) -> f32 {
    x - x.floor()
}

/// Which armour-stand equipment slot the smithing result goes to (`SmithingScreen#updateArmorStandPreview`).
pub fn stand_slot(stack: &ItemStack) -> usize {
    use mcr_entity_model::slot;
    use mcr_protocol::types::component::{DataComponent, EquipmentSlot, component_type_id};
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::Equippable(e) => Some(Some(e.slot)),
        _ => None,
    });
    let removed = component_type_id("minecraft:equippable")
        .is_some_and(|id| stack.components.removed.contains(&id));
    let equip = patched.unwrap_or_else(|| {
        let name = u32::try_from(stack.item)
            .ok()
            .filter(|_| !removed)
            .and_then(|id| mcr_data::v26_3().item_equip_slot(id));
        match name {
            Some("head") => Some(EquipmentSlot::Head),
            Some("chest") => Some(EquipmentSlot::Chest),
            Some("legs") => Some(EquipmentSlot::Legs),
            Some("feet") => Some(EquipmentSlot::Feet),
            _ => None,
        }
    });
    match equip {
        Some(EquipmentSlot::Head) => slot::HEAD,
        Some(EquipmentSlot::Chest) => slot::CHEST,
        Some(EquipmentSlot::Legs) => slot::LEGS,
        Some(EquipmentSlot::Feet) => slot::FEET,
        _ => slot::OFFHAND,
    }
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
        let scene = EntityScene::new(device, queue, formats, assets.clone())
            .map_err(|e| eprintln!("preview: {e}"))
            .ok()?;
        let mut stand_data = FxHashMap::default();
        stand_data.insert(
            mcr_entity_model::idx::ARMOR_STAND_FLAGS,
            EntityDataValue::Byte(STAND_SHOW_ARMS_NO_BASEPLATE),
        );
        Some(Self {
            scene,
            book: None,
            device: device.clone(),
            queue: queue.clone(),
            formats,
            assets,
            stand_data,
            stand_equipment: Default::default(),
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
        self.scissor = scissor(rect, gui_scale, fb).map(|r| (r, Shown::Entity));
    }

    /// The smithing result, if any, on a posed armour stand.
    pub fn prepare_armor_stand(
        &mut self,
        stand_type: i32,
        result: Option<(&ItemStack, usize)>,
        registries: &mcr_client::Registries,
        rect: [i32; 4],
        gui_scale: u32,
        fb: (u32, u32),
    ) {
        self.stand_equipment = Default::default();
        if let Some((stack, slot)) = result
            && let Some(dst) = self.stand_equipment.get_mut(slot)
        {
            *dst = stack.clone();
        }
        let mut view = EntityView::new(stand_type, &self.stand_data);
        view.equipment = Some(&self.stand_equipment);
        view.body_yaw = STAND_BODY_YAW;
        view.yaw = STAND_BODY_YAW;
        view.head_yaw = STAND_BODY_YAW;
        view.light = [15, 15];
        self.scene
            .update(&SyncedRegistries(registries), std::iter::once(view));
        self.scene.prepare(&FrameParams {
            view_proj: armor_stand_matrix(rect, gui_scale, fb),
            camera: [0.0; 3],
            light: LightInfo::with_brightness(1.0),
        });
        self.scissor = scissor(rect, gui_scale, fb).map(|r| (r, Shown::Entity));
    }

    /// The enchanting book; `open` and `flip` are already interpolated to this frame.
    pub fn prepare_book(
        &mut self,
        rect: [i32; 4],
        gui_scale: u32,
        fb: (u32, u32),
        open: f32,
        flip: f32,
    ) {
        if self.book.is_none() {
            self.book =
                BlockEntityScene::new(&self.device, &self.queue, self.formats, self.assets.clone())
                    .map_err(|e| eprintln!("book preview: {e}"))
                    .ok();
        }
        let Some(book) = &mut self.book else {
            self.scissor = None;
            return;
        };
        let open = open.clamp(0.0, 1.0);
        let flip1 = (frac(flip + 0.25) * 1.6 - 0.3).clamp(0.0, 1.0);
        let flip2 = (frac(flip + 0.75) * 1.6 - 0.3).clamp(0.0, 1.0);
        let view_proj = book_matrix(rect, gui_scale, fb, open);
        book.update_book(Mat4::IDENTITY, 1.25 * open, flip1, flip2);
        book.prepare(&BlockEntityFrame {
            params: FrameParams {
                view_proj,
                camera: [0.0; 3],
                light: LightInfo::with_brightness(1.0),
            },
            game_time: 0,
            partial_ticks: 0.0,
            camera_rotation: Quat::IDENTITY,
            player: None,
            render_distance_blocks: 0.0,
        });
        self.scissor = scissor(rect, gui_scale, fb).map(|r| (r, Shown::Book));
    }

    pub fn record(&self, ctx: &mut OverlayCtx<'_>) {
        let Some((r, shown)) = self.scissor else {
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
        match (shown, &self.book) {
            (Shown::Entity, _) => self.scene.draw(&mut pass),
            (Shown::Book, Some(book)) => book.draw(&mut pass),
            (Shown::Book, None) => {}
        }
    }
}

fn scissor(rect: [i32; 4], gui_scale: u32, fb: (u32, u32)) -> Option<[u32; 4]> {
    let gs = gui_scale as i32;
    let clamp = |v: i32, max: u32| v.clamp(0, max as i32) as u32;
    let (x0, y0) = (clamp(rect[0] * gs, fb.0), clamp(rect[1] * gs, fb.1));
    let (x1, y1) = (
        clamp((rect[0] + rect[2]) * gs, fb.0),
        clamp((rect[1] + rect[3]) * gs, fb.1),
    );
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
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
mod screen_previews {
    use super::*;

    fn to_px(m: Mat4, p: Vec3, fb: (f32, f32)) -> (f32, f32, f32) {
        let c = m * p.extend(1.0);
        ((c.x * 0.5 + 0.5) * fb.0, (0.5 - c.y * 0.5) * fb.1, c.z)
    }

    #[test]
    fn armor_stand_stands_in_its_rect_seen_from_above() {
        let rect = [121, 20, 40, 60];
        let m = armor_stand_matrix(rect, 1, (320, 240));
        let (x, y, _) = to_px(m, Vec3::new(0.0, 0.0, 0.0), (320.0, 240.0));
        assert!((x - 141.0).abs() < 0.5);
        assert!((y - 75.0).abs() < 0.5, "{y}");
        let (_, top_y, top_z) = to_px(m, Vec3::new(0.0, 2.0, 0.0), (320.0, 240.0));
        assert!(top_y > 20.0 && top_y < 50.0 + 0.0, "{top_y}");
        let (_, _, feet_z) = to_px(m, Vec3::ZERO, (320.0, 240.0));
        assert!(top_z > feet_z);
    }

    #[test]
    fn book_sits_near_the_top_of_its_rect() {
        let rect = [14, 14, 38, 31];
        for open in [0.0, 0.5, 1.0] {
            let m = book_matrix(rect, 2, (640, 480), open);
            let (x, y, z) = to_px(m, Vec3::ZERO, (640.0, 480.0));
            assert!(x > 14.0 * 2.0 && x < 52.0 * 2.0, "{x}");
            assert!(y > 14.0 * 2.0 && y < 45.0 * 2.0, "{y}");
            assert!((0.0..=1.0).contains(&z));
        }
    }

    #[test]
    fn smithing_result_slot_follows_equippable() {
        use mcr_entity_model::slot;
        let item = |n| ItemStack::new(mcr_data::v26_3().item_by_name(n).unwrap() as i32, 1);
        assert_eq!(stand_slot(&item("minecraft:netherite_helmet")), slot::HEAD);
        assert_eq!(stand_slot(&item("minecraft:diamond_boots")), slot::FEET);
        assert_eq!(
            stand_slot(&item("minecraft:netherite_sword")),
            slot::OFFHAND
        );
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
