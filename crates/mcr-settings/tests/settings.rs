use mcr_settings::servers::ResourcePacks;
use mcr_settings::value::{CloudStatus, ParticleStatus};
use mcr_settings::{
    HeldState, InputKey, KeyBindings, MAPPINGS, Options, OptionsFile, ServerEntry, ServerList,
};

const VANILLA_IMPORT: &str = "version:4325\n\
renderDistance:16\n\
particles:1\n\
fov:0.5\n\
maxFps:999\n\
renderClouds:\"true\"\n\
mainHand:\"left\"\n\
chatVisibility:2\n\
lang:de_de\n\
resourcePacks:[\"vanilla\",\"file/pack.zip\"]\n\
soundCategory_music:0.25\n\
modelPart_hat:false\n\
key_key.jump:key.keyboard.space\n\
key_key.forward:key.keyboard.nonsense\n\
key_key.attack:key.mouse.left\n\
someModKey:foo:bar\n\
not an option line\n\
";

#[test]
fn defaults_match_vanilla() {
    let o = Options::default();
    assert_eq!(o.render_distance, 32);
    assert_eq!(o.max_fps, 120);
    assert_eq!(o.fov.0, 70);
    assert_eq!(o.gui_scale, 0);
    assert_eq!(o.mouse_sensitivity, 0.5);
    assert_eq!(o.chat_opacity, 1.0);
    assert_eq!(o.main_hand.name(), "right");
    assert_eq!(o.sound_volume("music"), Some(1.0));
    assert!(o.model_parts.iter().all(|&p| p));
}

#[test]
fn vanilla_import_reads_typed_values() {
    let f = OptionsFile::parse(VANILLA_IMPORT);
    let o = &f.options;
    assert_eq!(o.render_distance, 16);
    assert_eq!(o.particles, ParticleStatus::Decreased);
    assert_eq!(o.render_clouds, CloudStatus::Fancy);
    assert_eq!(o.fov.0, 90);
    assert_eq!(o.max_fps, 260);
    assert_eq!(o.main_hand.name(), "left");
    assert_eq!(o.chat_visibility.id(), 2);
    assert_eq!(o.language, "de_de");
    assert_eq!(o.resource_packs.0, vec!["vanilla", "file/pack.zip"]);
    assert_eq!(o.sound_volume("music"), Some(0.25));
    assert!(!o.model_parts[6]);
    let jump = MAPPINGS.iter().position(|m| m.name == "key.jump").unwrap();
    assert_eq!(f.bindings.input(jump), InputKey::Keyboard(44));
    let attack = MAPPINGS
        .iter()
        .position(|m| m.name == "key.attack")
        .unwrap();
    assert_eq!(f.bindings.input(attack), InputKey::Mouse(1));
    let forward = MAPPINGS
        .iter()
        .position(|m| m.name == "key.forward")
        .unwrap();
    assert_eq!(f.bindings.input(forward), InputKey::Unbound);
}

#[test]
fn unknown_keys_survive_round_trip() {
    let f = OptionsFile::parse(VANILLA_IMPORT);
    let text = f.to_text();
    assert!(text.contains("someModKey:foo:bar\n"));
    assert!(!text.contains("not an option line"));
    let extra: Vec<_> = f.extra().collect();
    assert!(extra.contains(&("version", "4325")));
    assert!(extra.contains(&("someModKey", "foo:bar")));
}

#[test]
fn round_trip_is_stable() {
    let first = OptionsFile::parse(VANILLA_IMPORT).to_text();
    let second = OptionsFile::parse(&first).to_text();
    assert_eq!(first, second);
    assert_eq!(OptionsFile::parse(&first), OptionsFile::parse(&second));
}

#[test]
fn written_formats_match_vanilla() {
    let text = OptionsFile::default().to_text();
    let line = |k: &str| text.lines().find(|l| l.starts_with(k)).unwrap().to_owned();
    assert_eq!(line("renderDistance:"), "renderDistance:32");
    assert_eq!(line("particles:"), "particles:0");
    assert_eq!(line("renderClouds:"), "renderClouds:\"true\"");
    assert_eq!(line("mainHand:"), "mainHand:\"right\"");
    assert_eq!(line("fov:"), "fov:0.0");
    assert_eq!(line("gamma:"), "gamma:0.5");
    assert_eq!(line("key_key.forward:"), "key_key.forward:key.keyboard.w");
    assert_eq!(line("key_key.attack:"), "key_key.attack:key.mouse.left");
    assert_eq!(
        line("key_key.smoothCamera:"),
        "key_key.smoothCamera:key.keyboard.unknown"
    );
    assert_eq!(line("soundCategory_ui:"), "soundCategory_ui:1.0");
    assert_eq!(line("modelPart_cape:"), "modelPart_cape:true");
}

#[test]
fn invalid_values_fall_back_and_clamp() {
    let f =
        OptionsFile::parse("renderDistance:far\nmaxFps:999\nmaxFpsLow:1\nchatOpacity:-3\nfov:-9\n");
    assert_eq!(f.options.render_distance, 32);
    assert_eq!(f.options.max_fps, 260);
    assert_eq!(f.options.chat_opacity, 0.0);
    assert_eq!(f.options.fov.0, 30);
    assert!(f.extra().any(|kv| kv == ("maxFpsLow", "1")));
}

#[test]
fn hostile_input_never_panics() {
    let long = "x".repeat(100_000);
    let inputs = [
        String::new(),
        ":".into(),
        "\u{0}:\u{ffff}".into(),
        format!("maxFps:{long}"),
        "key_key.jump:".into(),
        "gamma:NaN\nchatScale:inf\n".into(),
        "soundCategory_master:1e999\n".into(),
        "resourcePacks:[1,2\n".into(),
    ];
    for input in &inputs {
        let f = OptionsFile::parse(input);
        let _ = f.to_text();
    }
    let bytes = [0xff, 0xfe, b':', 0x80, b'\n', b'a', b':'];
    let _ = OptionsFile::parse_bytes(&bytes).to_text();
    let f = OptionsFile::parse(&inputs[5]);
    assert_eq!(f.options.gamma, 0.5);
    assert_eq!(f.options.chat_scale, 1.0);
}

#[test]
fn every_default_binding_parses_back() {
    let bindings = KeyBindings::default();
    for (i, mapping) in MAPPINGS.iter().enumerate() {
        let text = format!("key_{}:{}\n", mapping.name, bindings.input(i).name());
        let parsed = OptionsFile::parse(&text);
        assert_eq!(
            parsed.bindings.input(i),
            mapping.default,
            "{}",
            mapping.name
        );
        assert_eq!(mapping.default.name(), bindings.input(i).name());
    }
}

#[test]
fn default_bindings_have_no_conflicts() {
    assert!(KeyBindings::default().conflicts().is_empty());
}

#[test]
fn conflict_detected_for_shared_key() {
    let mut b = KeyBindings::default();
    let jump = MAPPINGS.iter().position(|m| m.name == "key.jump").unwrap();
    let forward = MAPPINGS
        .iter()
        .position(|m| m.name == "key.forward")
        .unwrap();
    b.set(forward, InputKey::Keyboard(44));
    let conflicts = b.conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].input, InputKey::Keyboard(44));
    assert!(conflicts[0].mappings.contains(&"key.forward"));
    assert!(conflicts[0].mappings.contains(&"key.jump"));
    assert_eq!(b.lookup(InputKey::Keyboard(44)).len(), 2);
    assert!(b.lookup(InputKey::Keyboard(44)).contains(&jump));
}

#[test]
fn lookup_finds_default_actions() {
    let b = KeyBindings::default();
    let jump = MAPPINGS.iter().position(|m| m.name == "key.jump").unwrap();
    assert_eq!(b.lookup(InputKey::Keyboard(44)), &[jump]);
    assert!(b.lookup(InputKey::Unbound).is_empty());
}

#[test]
fn toggle_sneak_flips_on_press() {
    let options = Options {
        toggle_crouch: true,
        ..Options::default()
    };
    let sneak = MAPPINGS.iter().position(|m| m.name == "key.sneak").unwrap();
    let mut held = HeldState::default();
    assert!(held.update(sneak, true, &options));
    assert!(held.update(sneak, true, &options));
    assert!(held.update(sneak, false, &options));
    assert!(!held.update(sneak, true, &options));
    assert!(!held.is_active(sneak));
}

#[test]
fn hold_sprint_follows_physical_state() {
    let options = Options::default();
    let sprint = MAPPINGS
        .iter()
        .position(|m| m.name == "key.sprint")
        .unwrap();
    let mut held = HeldState::default();
    assert!(held.update(sprint, true, &options));
    assert!(!held.update(sprint, false, &options));
}

#[test]
fn servers_round_trip() {
    let list = ServerList {
        servers: vec![
            ServerEntry {
                name: "Local".into(),
                ip: "127.0.0.1:25565".into(),
                icon: Some("iVBORw0KGgo=".into()),
                resource_packs: ResourcePacks::Enabled,
                accepted_code_of_conduct: 0,
                hidden: false,
            },
            ServerEntry {
                name: "Hidden".into(),
                ip: "example.org".into(),
                icon: None,
                resource_packs: ResourcePacks::Prompt,
                accepted_code_of_conduct: 2,
                hidden: true,
            },
        ],
    };
    let bytes = list.to_bytes().unwrap();
    let back = ServerList::parse(&bytes).unwrap();
    assert_eq!(back, list);
    assert_eq!(back.visible().count(), 1);
    assert_eq!(back.visible().next().unwrap().name, "Local");
}

#[test]
fn servers_missing_fields_default() {
    let mut root = mcr_nbt::Compound::new();
    let mut entry = mcr_nbt::Compound::new();
    entry.insert("ip", "a.example");
    root.insert(
        "servers",
        mcr_nbt::Value::List(mcr_nbt::List::Compound(vec![entry])),
    );
    let bytes = mcr_nbt::write_disk("", &root, mcr_nbt::Compression::Gzip).unwrap();
    let list = ServerList::parse(&bytes).unwrap();
    let s = &list.servers[0];
    assert_eq!(s.name, "");
    assert_eq!(s.resource_packs, ResourcePacks::Prompt);
    assert!(!s.hidden);
}

#[test]
fn servers_hostile_input_is_an_error() {
    assert!(ServerList::parse(&[0x0a, 0x00]).is_err());
    assert!(ServerList::parse(b"not nbt at all").is_err());
    assert!(ServerList::parse(&[]).is_err());
}
