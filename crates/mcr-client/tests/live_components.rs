//! `MCR_LIVE_ADDR=localhost:25610 MCR_LIVE_CONSOLE=<stdin file> MCR_LIVE_LOG=<server log> \
//!  cargo test -p mcr-client --test live_components -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_protocol::play::serverbound::ContainerInput as I;

const ITEMS: &[&str] = &[
    "minecraft:elytra[enchantments={unbreaking:3,mending:1},damage=10]",
    "minecraft:diamond_chestplate[trim={material:\"minecraft:gold\",pattern:\"minecraft:coast\"}]",
    "minecraft:apple[food={nutrition:5,saturation:2.0f,can_always_eat:true},consumable={consume_seconds:2.0f,animation:\"drink\",on_consume_effects:[{type:\"minecraft:apply_effects\",effects:[{id:\"minecraft:speed\",duration:100,amplifier:1}],probability:0.5f},{type:\"minecraft:clear_all_effects\"}]}]",
    "minecraft:stick[tool={rules:[{blocks:\"#minecraft:mineable/pickaxe\",speed:4.0f,correct_for_drops:true}],default_mining_speed:1.5f,damage_per_block:2},weapon={item_damage_per_attack:2,disable_blocking_for_seconds:1.5f},attack_range={max_reach:4.5f},use_effects={can_sprint:true,speed_multiplier:0.5f}]",
    "minecraft:written_book[written_book_content={title:\"T\",author:\"A\",pages:['{\"text\":\"hi\",\"color\":\"red\",\"bold\":true,\"click_event\":{\"action\":\"open_url\",\"url\":\"https://example.com\"},\"hover_event\":{\"action\":\"show_text\",\"value\":\"tip\"}}','{\"nbt\":\"a.b\",\"storage\":\"minecraft:x\",\"interpret\":true}']}]",
    "minecraft:white_banner[banner_patterns=[{pattern:\"minecraft:border\",color:\"red\"},{pattern:\"minecraft:stripe_top\",color:\"blue\"}]]",
    "minecraft:firework_rocket[fireworks={flight_duration:2,explosions:[{shape:\"star\",colors:[I;16711680],fade_colors:[I;255],has_trail:true,has_twinkle:true}]}]",
    "minecraft:diamond_helmet[equippable={slot:\"head\",asset_id:\"minecraft:iron\"},glider={}]",
    "minecraft:shield[blocks_attacks={block_delay_seconds:0.5f},death_protection={death_effects:[{type:\"minecraft:clear_all_effects\"}]}]",
    "minecraft:iron_sword[kinetic_weapon={delay_ticks:3,forward_movement:0.5f},piercing_weapon={dismounts:true}]",
    "minecraft:potion[potion_contents={potion:\"minecraft:swiftness\",custom_effects:[{id:\"minecraft:speed\",amplifier:2,duration:200,show_icon:false}],custom_color:255}]",
    "minecraft:stone[can_break=[{blocks:\"minecraft:stone\"}],can_place_on=[{blocks:\"minecraft:dirt\",state:{snowy:\"false\"}},{blocks:\"#minecraft:logs\"}]]",
    "minecraft:goat_horn[instrument=\"minecraft:ponder_goat_horn\"]",
    "minecraft:stick[jukebox_playable=\"minecraft:cat\"]",
    "minecraft:compass[lodestone_tracker={target:{pos:[I;1,2,3],dimension:\"minecraft:overworld\"},tracked:false}]",
    "minecraft:stick[custom_model_data={floats:[1.5f],flags:[true],strings:[\"s\"],colors:[255]},tooltip_display={hide_tooltip:false,hidden_components:[\"minecraft:enchantments\"]},rarity=\"epic\",enchantment_glint_override=true]",
    "minecraft:leather_chestplate[dyed_color=16711680,lore=['{\"text\":\"l1\",\"italic\":false}',\"l2\"],custom_name='{\"text\":\"N\",\"color\":\"gold\"}',attribute_modifiers=[{type:\"minecraft:armor\",id:\"minecraft:x\",amount:2.5,operation:\"add_value\",slot:\"chest\"}]]",
    "minecraft:bundle[bundle_contents=[{id:\"minecraft:stone\",count:5},{id:\"minecraft:apple\"}]]",
    "minecraft:crossbow[charged_projectiles=[{id:\"minecraft:arrow\"}]]",
    "minecraft:shulker_box[container=[{slot:0,item:{id:\"minecraft:stone\",count:3}}],custom_name='\"Box\"']",
    "minecraft:decorated_pot[pot_decorations={back:{id:\"minecraft:brick\"},left:{id:\"minecraft:sherd\"}}]",
    "minecraft:stick[equippable={slot:\"saddle\",equip_sound:\"minecraft:item.armor.equip_chain\",allowed_entities:[\"minecraft:pig\"],dispensable:false,can_be_sheared:true},repair_cost=3,max_damage=100,damage=7,max_stack_size=1,unbreakable={}]",
    "minecraft:suspicious_stew[suspicious_stew_effects=[{id:\"minecraft:night_vision\",duration:100}]]",
    "minecraft:writable_book[writable_book_content={pages:[\"p1\",{raw:\"p2\",filtered:\"f2\"}]}]",
    "minecraft:stick[food={nutrition:1,saturation:0.5f},use_cooldown={seconds:2.0f,cooldown_group:\"minecraft:g\"},use_remainder={id:\"minecraft:bowl\"},enchantable={value:10},repairable={items:\"#minecraft:planks\"},damage_resistant={types:\"#minecraft:is_fire\"},tooltip_style=\"minecraft:t\",item_model=\"minecraft:diamond\",item_name='\"IN\"']",
    "minecraft:player_head[profile=\"Notch\"]",
    "minecraft:stick[wolf/variant=\"minecraft:ashen\",wolf/collar=\"red\",cat/collar=\"blue\",sheep/color=\"pink\",tropical_fish/pattern=\"flopper\",rabbit/variant=\"evil\",fox/variant=\"snow\",parrot/variant=\"gray\",horse/variant=\"dark_brown\",llama/variant=\"gray\",axolotl/variant=\"blue\",mooshroom/variant=\"brown\",salmon/size=\"large\"]",
    "minecraft:stick[map_id=3,potion_duration_scale=0.5f,minimum_attack_charge=0.5f,note_block_sound=\"minecraft:x\",ominous_bottle_amplifier=2,custom_data={a:1b,b:[1,2],c:{d:\"e\"}}]",
    "minecraft:stick[block_state={facing:\"north\"},block_entity_data={id:\"minecraft:chest\",Lock:\"k\"},entity_data={id:\"minecraft:pig\",Health:5.0f},bucket_entity_data={NoAI:1b}]",
];

fn settle(live: &mut Live) {
    live.idle(8);
}

fn updates(live: &Live) -> u64 {
    live.client.state.inventory.server_updates
}

fn click(live: &mut Live, what: &str, slot: i16, button: i8, input: I) -> bool {
    let before = updates(live);
    live.client.click_container(slot, button, input).unwrap();
    settle(live);
    let ok = before == updates(live);
    if !ok {
        println!("CORRECTED after {what} slot {slot}");
    }
    ok
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_component_item_clicks() {
    let mut live = Live::connect("Comp");
    live.console("op Comp");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Comp");
    live.console("clear Comp");
    live.idle(10);
    live.console("setblock ~ ~ ~ minecraft:air");
    let mut given = Vec::new();
    let only: Option<Vec<usize>> = std::env::var("MCR_ITEMS")
        .ok()
        .map(|v| v.split(',').filter_map(|n| n.parse().ok()).collect());
    for (i, item) in ITEMS.iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.contains(&i)) {
            continue;
        }
        let before = live.log_text().len();
        live.console(&format!("give Comp {item}"));
        live.idle(6);
        let log = live.log_text();
        let new = &log[before.min(log.len())..];
        if new.contains("Unknown") || new.contains("Expected") || new.contains("Incorrect") {
            println!("ITEM {i} rejected: {item}\n{new}");
        } else {
            given.push((i, *item));
        }
    }
    live.idle(10);
    let mut bad = Vec::new();
    let mut tested = 0;
    let slots: Vec<usize> = live
        .client
        .state
        .inventory
        .player
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.is_empty() && !s.components.is_empty())
        .map(|(i, _)| i)
        .collect();
    live.client.state.inventory.corrections = 0;
    for slot in slots {
        let stack = live.client.state.inventory.player.slots[slot].clone();
        let names: Vec<_> = stack
            .components
            .added
            .iter()
            .map(|c| c.name().to_owned())
            .collect();
        let slot = slot as i16;
        println!("components {:?}", stack.components);
        let mut ok = click(&mut live, "pickup", slot, 0, I::Pickup);
        ok &= click(&mut live, "place", slot, 0, I::Pickup);
        ok &= click(&mut live, "quick move", slot, 0, I::QuickMove);
        tested += 1;
        if !ok {
            let now: Vec<_> = live
                .client
                .state
                .inventory
                .player
                .slots
                .iter()
                .filter(|s| !s.is_empty())
                .collect();
            println!("after: {now:?}");
        }
        println!(
            "slot {slot} {} {names:?}",
            if ok { "ok" } else { "CORRECTED" }
        );
        if !ok {
            bad.push((slot, names));
        }
    }
    println!("tested {tested} stacks, given {}", given.len());
    assert!(bad.is_empty(), "server corrected clicks on: {bad:?}");
}
