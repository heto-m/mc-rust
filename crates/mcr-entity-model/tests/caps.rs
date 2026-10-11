mod support;

use mcr_entity_model::glam::Mat4;
use mcr_entity_model::{
    Bone34, EntityRenderer, ImageRgba, LightInfo, Model, PartInstance, Pipeline, TargetFormats,
};

fn renderer() -> Option<EntityRenderer> {
    let (device, queue) = support::gpu()?;
    Some(EntityRenderer::new(
        &device,
        &queue,
        TargetFormats {
            color: support::COLOR,
            depth: support::DEPTH,
            reverse_z: true,
            sample_count: 1,
        },
    ))
}

#[test]
fn hostile_instance_and_bone_counts_are_capped() {
    let Some(mut r) = renderer() else { return };
    let model = r.add_model(&Model::unit_cube());
    let tex = r
        .add_texture(&ImageRgba::solid(2, 2, [255; 4]))
        .expect("texture");
    r.begin_frame();
    assert!(r.alloc_bones(usize::MAX).is_none());
    assert!(r.alloc_bones(usize::MAX / 2).is_none());
    for i in 0..(1 << 17) {
        let Some((base, bones)) = r.alloc_bones(4) else {
            break;
        };
        bones.fill(Bone34::IDENTITY);
        r.push(&PartInstance {
            model,
            texture: tex,
            pipeline: Pipeline::Cutout,
            bone_base: base,
            tint: [255; 4],
            hurt: false,
            flash: false,
            fullbright: false,
            block_light: 0,
            sky_light: 15,
            uv_offset: [0.0; 2],
            origin: [i as f64, 0.0, 0.0],
        });
    }
    assert!(r.instance_count() <= mcr_entity_model::MAX_INSTANCES);
    r.prepare(Mat4::IDENTITY, [0.0; 3], LightInfo::default());
}

#[test]
fn hostile_texture_dimensions_are_rejected() {
    let Some(mut r) = renderer() else { return };
    let huge = ImageRgba {
        width: u32::MAX,
        height: u32::MAX,
        rgba: vec![0; 16],
    };
    assert!(r.add_texture(&huge).is_none());
    let lying = ImageRgba {
        width: 1024,
        height: 1024,
        rgba: vec![0; 16],
    };
    assert!(r.add_texture(&lying).is_none());
}

#[test]
fn thousands_of_hostile_item_stacks_stay_bounded() {
    use mcr_entity_model::mcr_protocol::types::component::DataComponent;
    use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue as V;
    use mcr_entity_model::{EntityView, FrameParams};
    let Some(mut h) = support::Harness::new() else {
        return;
    };
    let item_count = mcr_data::v26_3().items().len() as i32;
    let datas: Vec<support::Data> = (0..6000i32)
        .map(|i| {
            let mut stack = mcr_entity_model::mcr_protocol::types::ItemStack::new(
                (i.wrapping_mul(7919)).rem_euclid(item_count + 50) - 10,
                i32::MAX,
            );
            stack
                .components
                .added
                .push(DataComponent::DyedColor(i.wrapping_mul(2654435)));
            support::Data::default().with(8, V::ItemStack(stack))
        })
        .collect();
    let reg = support::TestRegistries::default();
    let frame = FrameParams {
        view_proj: Mat4::IDENTITY,
        camera: [0.0; 3],
        light: LightInfo::default(),
    };
    for chunk in datas.chunks(1000) {
        let views = chunk.iter().enumerate().map(|(i, d)| {
            let mut v = EntityView::named("minecraft:item", &d.0);
            v.id = i as i32;
            v
        });
        h.scene.update(&reg, views);
        h.scene.prepare(&frame);
    }
    let stats = h.scene.stats();
    assert!(stats.models <= 4096, "{stats:?}");
    assert!(stats.parts <= mcr_entity_model::MAX_INSTANCES);
}
