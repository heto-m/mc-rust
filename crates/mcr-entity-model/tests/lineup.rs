mod support;

use mcr_buf::IdOr;
use mcr_entity_model::EntityView;
use mcr_entity_model::glam::{DVec3, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::component::{ArmorTrim, DataComponent, EquipmentSlot};
use mcr_entity_model::mcr_protocol::types::entity_data::{
    DyeColor, EntityDataValue as V, Pose, VillagerData,
};
use support::*;

#[derive(Default)]
struct Spec {
    ty: &'static str,
    data: Data,
    eq: Option<Box<[ItemStack; 8]>>,
    walk: f32,
    attack: f32,
    hurt: bool,
    death: f32,
    uuid: u128,
    obj: i32,
    id: i32,
    name: Option<&'static str>,
}

fn spec(ty: &'static str) -> Spec {
    Spec {
        ty,
        walk: 1.0,
        ..Spec::default()
    }
}

impl Spec {
    fn d(mut self, i: u8, v: V) -> Self {
        self.data = self.data.with(i, v);
        self
    }

    fn equip(mut self, slot: usize, stack: ItemStack) -> Self {
        self.eq.get_or_insert_with(Default::default)[slot] = stack;
        self
    }

    fn baby(self) -> Self {
        let i = if self.ty == "minecraft:piglin" {
            17
        } else {
            16
        };
        self.d(i, V::Boolean(true))
    }
}

fn row(h: &mut Harness, specs: &[Spec], name: &str, spacing: f64, _world_h: f64) -> Shot {
    const COLS: usize = 5;
    let registries = TestRegistries::vanilla();
    h.scene
        .set_block_top(Some(Box::new(|_, y, _| (y < 0).then_some(1.0))));
    let (width, height) = (1800u32, 750u32);
    let mut all = Vec::new();
    let mut rows = 0;
    let mut covered = 0;
    for (ci, chunk) in specs.chunks(COLS).enumerate() {
        let n = chunk.len() as f64;
        let views: Vec<EntityView> = chunk
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut v = EntityView::named(s.ty, &s.data.0);
                v.pos = [(i as f64 - (COLS as f64 - 1.0) / 2.0) * spacing, 0.0, 0.0];
                v.body_yaw = 20.0;
                v.yaw = 20.0;
                v.head_yaw = 28.0;
                v.pitch = 8.0;
                v.age_ticks = 40.0;
                v.limb_swing = 2.0 + i as f32 * 0.7;
                v.limb_amount = 0.8 * s.walk;
                v.attack_time = s.attack;
                v.hurt = s.hurt;
                v.death_time = s.death;
                v.uuid = uuid::Uuid::from_u128(s.uuid);
                v.id = s.id + (ci * COLS + i) as i32;
                v.custom_name = s.name;
                v.object_data = s.obj;
                v.equipment = s.eq.as_deref();
                v
            })
            .collect();
        let _ = n;
        let world_w = COLS as f64 * spacing + 0.4;
        let dist = world_w * 0.95;
        h.scene.set_camera([0.0, 1.5, dist], 180.0, 0.0);
        h.scene.update(&registries, views.into_iter());
        let cam = Camera {
            position: DVec3::new(0.0, 1.5, dist),
            target: Vec3::new(0.0, 1.5, 0.0),
            fov_deg: 30.0,
        };
        let shot = h.render(&cam.frame(width, height), width, height);
        covered += shot.covered();
        all.extend(shot.pixels);
        rows += 1;
    }
    let shot = Shot {
        pixels: all,
        width,
        height: height * rows,
    };
    println!("{}", shot.save(name).display());
    let _ = covered;
    shot
}

fn dye(c: DyeColor) -> i32 {
    c.id()
}

#[test]
fn farm_animals() {
    let Some(mut h) = Harness::new() else { return };
    let specs = [
        spec("minecraft:pig"),
        spec("minecraft:pig").d(19, V::PigVariant(0)),
        spec("minecraft:cow"),
        spec("minecraft:cow").baby(),
        spec("minecraft:sheep").d(18, V::Byte(14)),
        spec("minecraft:sheep").d(18, V::Byte(11)),
        spec("minecraft:sheep").d(18, V::Byte(0x10)),
        spec("minecraft:chicken"),
        spec("minecraft:mooshroom").d(18, V::Int(1)),
        spec("minecraft:rabbit").d(18, V::Int(3)),
    ];
    let shot = row(&mut h, &specs, "farm", 1.5, 2.0);
    assert!(shot.covered() > 8000);
}

#[test]
fn pets() {
    let Some(mut h) = Harness::new() else { return };
    let specs = [
        spec("minecraft:cat").d(20, V::CatVariant(7)),
        spec("minecraft:cat").d(20, V::CatVariant(1)),
        spec("minecraft:cat")
            .d(20, V::CatVariant(9))
            .d(18, V::Byte(4))
            .d(23, V::Int(dye(DyeColor::Red))),
        spec("minecraft:wolf").d(23, V::WolfVariant(3)),
        spec("minecraft:wolf")
            .d(23, V::WolfVariant(8))
            .d(18, V::Byte(4))
            .d(21, V::Int(dye(DyeColor::Blue))),
        spec("minecraft:wolf").d(23, V::WolfVariant(3)).baby(),
        spec("minecraft:fox").d(18, V::Int(1)),
        spec("minecraft:parrot").d(20, V::Int(2)),
        spec("minecraft:horse").d(19, V::Int(0x0302)),
        spec("minecraft:llama").d(21, V::Int(2)),
    ];
    let shot = row(&mut h, &specs, "pets", 1.6, 2.4);
    assert!(shot.covered() > 8000);
}

#[test]
fn villagers_and_hostiles() {
    let Some(mut h) = Harness::new() else { return };
    let vd = |t, p, l| {
        V::VillagerData(VillagerData {
            kind: t,
            profession: p,
            level: l,
        })
    };
    let specs = [
        spec("minecraft:villager").d(19, vd(2, 5, 1)),
        spec("minecraft:villager").d(19, vd(0, 9, 3)),
        spec("minecraft:villager").d(19, vd(4, 2, 5)),
        spec("minecraft:zombie_villager").d(20, vd(1, 8, 2)),
        spec("minecraft:witch"),
        spec("minecraft:pillager"),
        spec("minecraft:iron_golem"),
        spec("minecraft:enderman"),
        spec("minecraft:skeleton"),
        spec("minecraft:creeper"),
        spec("minecraft:slime").d(18, V::Int(2)),
        spec("minecraft:spider"),
    ];
    let shot = row(&mut h, &specs, "villagers", 1.7, 3.0);
    assert!(shot.covered() > 8000);
}

#[test]
fn armored_zombie_and_players() {
    let Some(mut h) = Harness::new() else { return };
    let trim = |mat, pat| {
        let mut s = item("minecraft:diamond_chestplate");
        s.components.added.push(DataComponent::Trim(ArmorTrim {
            material: IdOr::Id(mat),
            pattern: IdOr::Id(pat),
        }));
        s
    };
    let mut leather = item("minecraft:leather_chestplate");
    leather
        .components
        .added
        .push(DataComponent::DyedColor(0x3355FF));
    let mut legs = item("minecraft:diamond_leggings");
    legs.components.added.push(DataComponent::Trim(ArmorTrim {
        material: IdOr::Id(5),
        pattern: IdOr::Id(8),
    }));
    let _ = EquipmentSlot::Head;
    let specs = [
        spec("minecraft:zombie")
            .equip(4, item("minecraft:diamond_helmet"))
            .equip(3, trim(5, 8))
            .equip(2, legs)
            .equip(1, item("minecraft:diamond_boots"))
            .equip(0, item("minecraft:diamond_sword")),
        spec("minecraft:zombie")
            .equip(3, leather)
            .equip(0, item("minecraft:iron_axe")),
        spec("minecraft:skeleton")
            .equip(0, item("minecraft:bow"))
            .equip(4, item("minecraft:iron_helmet")),
        spec("minecraft:player")
            .d(6, V::Pose(Pose::Crouching))
            .equip(0, item("minecraft:diamond_pickaxe")),
        spec("minecraft:player")
            .uuid(0x1234_5678_9abc_def0_1111_2222_3333_4444)
            .equip(4, item("minecraft:golden_helmet")),
        spec("minecraft:player").uuid(77),
        spec("minecraft:armor_stand")
            .equip(4, item("minecraft:iron_helmet"))
            .equip(3, item("minecraft:iron_chestplate")),
        spec("minecraft:piglin").equip(0, item("minecraft:golden_sword")),
    ];
    let shot = row(&mut h, &specs, "armor", 1.6, 2.4);
    assert!(shot.covered() > 8000);
}

impl Spec {
    fn uuid(mut self, u: u128) -> Self {
        self.uuid = u;
        self
    }
}

#[test]
fn items_frames_paintings_blocks() {
    let Some(mut h) = Harness::new() else { return };
    let mut stack = item("minecraft:diamond_sword");
    stack.count = 40;
    let frame_item = item("minecraft:apple");
    let specs = [
        spec("minecraft:item").d(8, V::ItemStack(stack)),
        spec("minecraft:item").d(8, V::ItemStack(item("minecraft:stone"))),
        spec("minecraft:item_frame")
            .d(
                8,
                V::Direction(mcr_entity_model::mcr_protocol::types::entity_data::Direction::South),
            )
            .d(9, V::ItemStack(frame_item)),
        spec("minecraft:painting")
            .d(
                8,
                V::Direction(mcr_entity_model::mcr_protocol::types::entity_data::Direction::South),
            )
            .d(9, V::PaintingVariant(IdOr::Id(24))),
        spec("minecraft:falling_block").obj(
            mcr_data::v26_3()
                .block_by_name("minecraft:sand")
                .and_then(|b| mcr_data::v26_3().block_info(b))
                .map_or(0, |i| i.default_state as i32),
        ),
        spec("minecraft:oak_boat"),
        spec("minecraft:minecart"),
        spec("minecraft:arrow"),
        spec("minecraft:tnt"),
        spec("minecraft:end_crystal"),
    ];
    let shot = row(&mut h, &specs, "items", 1.5, 2.0);
    assert!(shot.covered() > 4000);
}

#[test]
fn more_mobs() {
    let Some(mut h) = Harness::new() else { return };
    let specs = [
        spec("minecraft:bat"),
        spec("minecraft:bee"),
        spec("minecraft:blaze"),
        spec("minecraft:enderman").baby(),
        spec("minecraft:squid"),
        spec("minecraft:axolotl").d(18, V::Int(3)),
        spec("minecraft:frog").d(18, V::FrogVariant(2)),
        spec("minecraft:tropical_fish").d(17, V::Int(0x0501_0101)),
        spec("minecraft:salmon"),
        spec("minecraft:turtle"),
        spec("minecraft:panda"),
        spec("minecraft:polar_bear"),
        spec("minecraft:goat"),
        spec("minecraft:camel"),
        spec("minecraft:sniffer"),
        spec("minecraft:warden"),
        spec("minecraft:phantom"),
        spec("minecraft:vex"),
        spec("minecraft:wither"),
        spec("minecraft:shulker"),
    ];
    let shot = row(&mut h, &specs, "more", 2.2, 3.0);
    assert!(shot.covered() > 8000);
}

#[test]
fn death_and_hurt() {
    let Some(mut h) = Harness::new() else { return };
    let mut hurt = spec("minecraft:zombie");
    hurt.hurt = true;
    let mut dead = spec("minecraft:pig");
    dead.death = 20.0;
    let specs = [
        hurt,
        dead,
        spec("minecraft:zombie").baby(),
        spec("minecraft:slime").d(18, V::Int(3)),
        spec("minecraft:giant"),
    ];
    let shot = row(&mut h, &specs, "hurt", 2.0, 3.0);
    assert!(shot.covered() > 4000);
}

impl Spec {
    fn obj(mut self, o: i32) -> Self {
        self.obj = o;
        self
    }
}

#[test]
fn name_tags_project_to_the_screen() {
    let Some(mut h) = Harness::new() else { return };
    let reg = TestRegistries::vanilla();
    let data = Data::default();
    let mut v = EntityView::named("minecraft:pig", &data.0);
    v.custom_name = Some("Bacon");
    v.pos = [0.0, 0.0, 0.0];
    h.scene.update(&reg, std::iter::once(v));
    let cam = Camera {
        position: DVec3::new(0.0, 1.0, 5.0),
        target: Vec3::new(0.0, 1.0, 0.0),
        fov_deg: 70.0,
    };
    let frame = cam.frame(800, 600);
    let tags = h.scene.name_tags(&frame, [800.0, 600.0]);
    assert_eq!(tags.len(), 1);
    assert!((tags[0].screen[0] - 400.0).abs() < 2.0);
    let behind = Camera {
        position: DVec3::new(0.0, 1.0, 5.0),
        target: Vec3::new(0.0, 1.0, 20.0),
        ..cam
    };
    assert!(
        h.scene
            .name_tags(&behind.frame(800, 600), [800.0, 600.0])
            .is_empty()
    );
}

#[test]
fn unknown_type_draws_hitbox() {
    let Some(mut h) = Harness::new() else { return };
    let reg = TestRegistries::vanilla();
    let data = Data::default();
    let mut v = EntityView::new(9999, &data.0);
    v.pos = [0.0, 0.0, 0.0];
    h.scene.update(&reg, std::iter::once(v));
    let cam = Camera {
        position: DVec3::new(0.0, 0.5, 4.0),
        target: Vec3::new(0.0, 0.5, 0.0),
        fov_deg: 60.0,
    };
    let shot = h.render(&cam.frame(400, 300), 400, 300);
    assert!(shot.covered() > 500);
}

#[test]
fn aquatic() {
    let Some(mut h) = Harness::new() else { return };
    let mut hold = spec("minecraft:dolphin");
    hold = hold.equip(0, item("minecraft:cod"));
    let specs = [
        spec("minecraft:squid"),
        spec("minecraft:glow_squid"),
        spec("minecraft:cod"),
        spec("minecraft:salmon"),
        spec("minecraft:tropical_fish").d(17, V::Int(0x0501_0101)),
        spec("minecraft:pufferfish").d(17, V::Int(2)),
        spec("minecraft:pufferfish").d(17, V::Int(0)),
        hold,
        spec("minecraft:guardian"),
        spec("minecraft:elder_guardian"),
        spec("minecraft:turtle"),
        spec("minecraft:axolotl"),
    ];
    let shot = row(&mut h, &specs, "aquatic", 2.4, 3.0);
    assert!(shot.covered() > 4000);
}

#[test]
fn flying() {
    let Some(mut h) = Harness::new() else { return };
    let mut vexy = spec("minecraft:vex").equip(0, item("minecraft:iron_sword"));
    vexy.walk = 0.5;
    let specs = [
        spec("minecraft:bat"),
        spec("minecraft:bee"),
        spec("minecraft:phantom"),
        vexy,
        spec("minecraft:allay").equip(0, item("minecraft:apple")),
        spec("minecraft:parrot").d(20, V::Int(1)),
        spec("minecraft:parrot").d(18, V::Byte(1)),
        spec("minecraft:blaze"),
        spec("minecraft:ghast"),
        spec("minecraft:happy_ghast"),
    ];
    let shot = row(&mut h, &specs, "flying", 5.0, 3.0);
    assert!(shot.covered() > 4000);
}

#[test]
fn bosses_and_golems() {
    let Some(mut h) = Harness::new() else { return };
    let block = mcr_data::v26_3()
        .block_by_name("minecraft:dirt")
        .and_then(|b| mcr_data::v26_3().block_info(b))
        .map_or(1, |i| i.default_state as i32);
    let specs = [
        spec("minecraft:ender_dragon"),
        spec("minecraft:wither"),
        spec("minecraft:iron_golem"),
        spec("minecraft:snow_golem"),
        spec("minecraft:ravager"),
        spec("minecraft:hoglin"),
        spec("minecraft:strider"),
        spec("minecraft:shulker").d(17, V::Byte(60)),
        spec("minecraft:enderman").d(16, V::OptionalBlockState(Some(block))),
        spec("minecraft:slime").d(18, V::Int(2)),
        spec("minecraft:magma_cube").d(18, V::Int(2)),
        spec("minecraft:horse").d(18, V::Byte(32)),
        spec("minecraft:llama").d(19, V::Boolean(true)),
        spec("minecraft:wolf")
            .d(18, V::Byte(5))
            .d(23, V::WolfVariant(3)),
        spec("minecraft:mooshroom"),
    ];
    let shot = row(&mut h, &specs, "bosses", 4.0, 6.0);
    assert!(shot.covered() > 4000);
}

#[test]
fn effects() {
    let Some(mut h) = Harness::new() else { return };
    let specs = [
        spec("minecraft:snowball").d(8, V::ItemStack(item("minecraft:snowball"))),
        spec("minecraft:ender_pearl").d(8, V::ItemStack(item("minecraft:ender_pearl"))),
        spec("minecraft:experience_orb").obj(40),
        spec("minecraft:fishing_bobber"),
        spec("minecraft:firework_rocket").d(8, V::ItemStack(item("minecraft:firework_rocket"))),
        spec("minecraft:end_crystal"),
        spec("minecraft:lightning_bolt"),
        spec("minecraft:egg").d(8, V::ItemStack(item("minecraft:egg"))),
    ];
    let shot = row(&mut h, &specs, "effects", 1.5, 2.0);
    assert!(shot.covered() > 1000);
}
