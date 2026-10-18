use std::io::Cursor;
use std::path::Path;

use mcr_settings::ModSettings;

use super::fixture::{Jar, png, temp_dir};
use super::scan::{self, ModKind, Side};
use super::*;

fn scan_one(jar: &Jar) -> ModInfo {
    scan::scan_reader(Path::new("x.jar"), "x.jar", Cursor::new(jar.build()))
}

#[test]
fn reads_fabric_metadata_without_loading_classes() {
    let icon = png(8, 4, [10, 20, 30, 255]);
    let jar = Jar::fabric_with(
        "demo",
        "1.2.3+mc26.3",
        r#""description":"A demo","environment":"client","icon":"assets/demo/icon.png",
           "depends":{"fabricloader":">=0.19","fabric-api":["1.x","2.x"],"minecraft":"26.3"},
           "recommends":{"modmenu":"*"},"breaks":{"sodium":"*"},"provides":["demo-alias"],
           "jars":[{"file":"META-INF/jars/inner.jar"}]"#,
    )
    .file("assets/demo/icon.png", &icon)
    .file("com/example/Broken.class", b"not a class")
    .nested(
        "META-INF/jars/inner.jar",
        Jar::fabric_with("inner", "0.5.0", r#""provides":["inner-old"]"#),
    );
    let m = scan_one(&jar);
    assert_eq!(m.kind, ModKind::Fabric);
    assert_eq!(
        (m.id.as_str(), m.version.as_str()),
        ("demo", "1.2.3+mc26.3")
    );
    assert_eq!(m.name, "demo mod");
    assert_eq!(m.description, "A demo");
    assert_eq!(m.side, Side::Client);
    assert_eq!(m.provides, ["demo-alias"]);
    assert_eq!(m.breaks[0].id, "sodium");
    assert_eq!(m.recommends[0].id, "modmenu");
    let api = m.depends.iter().find(|d| d.id == "fabric-api").unwrap();
    assert_eq!(api.range, ["1.x", "2.x"]);
    assert_eq!(m.nested.len(), 1);
    assert_eq!(m.nested[0].id, "inner");
    assert_eq!(m.nested[0].provides, ["inner-old"]);
    let icon = m.icon.expect("icon decoded");
    assert_eq!((icon.width, icon.height), (8, 4));
    assert_eq!(&icon.rgba[..4], &[10, 20, 30, 255]);
}

#[test]
fn picks_the_largest_icon_within_the_limit() {
    let jar = Jar::fabric_with(
        "demo",
        "1",
        r#""icon":{"16":"i16.png","128":"i128.png","1024":"i1024.png"}"#,
    )
    .file("i16.png", &png(16, 16, [1, 1, 1, 255]))
    .file("i128.png", &png(128, 128, [2, 2, 2, 255]))
    .file("i1024.png", &png(300, 300, [3, 3, 3, 255]));
    let icon = scan_one(&jar).icon.unwrap();
    assert_eq!(icon.width, 128);
}

#[test]
fn oversized_or_broken_icons_are_dropped_not_fatal() {
    for payload in [
        png(300, 300, [0, 0, 0, 255]),
        b"not a png".to_vec(),
        Vec::new(),
    ] {
        let jar = Jar::fabric_with("demo", "1", r#""icon":"i.png""#).file("i.png", &payload);
        let m = scan_one(&jar);
        assert_eq!(m.kind, ModKind::Fabric);
        assert!(m.icon.is_none());
    }
}

#[test]
fn neoforge_sets_resolve_with_jar_in_jar_and_loader_ids() {
    let dir = temp_dir("neoforge-set");
    let dep = |owner: &str, id: &str, range: &str| {
        format!(
            "[[dependencies.{owner}]]\nmodId=\"{id}\"\ntype=\"required\"\nversionRange=\"{range}\"\n"
        )
    };
    let lib = Jar::neoforge("mezz_config", "0.6.6");
    Jar::neoforge_with(
        "jei",
        "31.15.0.82",
        &(dep("jei", "mezz_config", "[0.6.3,1.0.0)")
            + &dep("jei", "neoforge", "[26.3.0.1-beta,)")
            + &dep("jei", "minecraft", "[26.3]")),
    )
    .nested("META-INF/jarjar/mezz.jar", lib)
    .file(
        "META-INF/jarjar/metadata.json",
        br#"{"jars":[{"path":"META-INF/jarjar/mezz.jar"}]}"#,
    )
    .write(&dir, "jei.jar");
    Jar::neoforge_with(
        "jade",
        "${file.jarVersion}",
        &dep("jade", "neoforge", "[26.3-beta, )"),
    )
    .file(
        "META-INF/MANIFEST.MF",
        b"Manifest-Version: 1.0\r\nImplementation-Version: 26.3.5+neoforge\r\n",
    )
    .write(&dir, "jade.jar");
    Jar::neoforge_with("needy", "1.0", &dep("needy", "missing_lib", "[1,)"))
        .write(&dir, "needy.jar");

    Jar::new()
        .file(
            "META-INF/MANIFEST.MF",
            b"Manifest-Version: 1.0\nSpecification-Title: MezzConfig\nImplementation-Version: 0.6.8\n\
              FMLModType: GAMELIBRARY\n",
        )
        .write(&dir, "mezz-standalone.jar");

    let report = Manager::new(&dir, &dir).scan(&ModSettings::default());
    assert_eq!(status(&report, "jei"), &ModStatus::Active);
    let lib = report
        .entries
        .iter()
        .find(|e| e.info.file_name == "mezz-standalone.jar")
        .unwrap();
    assert_eq!(lib.status, ModStatus::Active);
    assert_eq!(
        (lib.info.name.as_str(), lib.info.version.as_str()),
        ("MezzConfig", "0.6.8")
    );
    assert_eq!(status(&report, "jade"), &ModStatus::Active);
    let jade = report.entries.iter().find(|e| e.info.id == "jade").unwrap();
    assert_eq!(jade.info.version, "26.3.5+neoforge");
    assert_eq!(
        status(&report, "needy"),
        &ModStatus::Excluded("missing dependency `missing_lib`".into())
    );
    assert_eq!(report.loader(), Some(crate::Loader::NeoForge));
    assert!(report.mixed.is_none());

    Jar::fabric("modmenu", "21.0.0").write(&dir, "modmenu.jar");
    let mixed = Manager::new(&dir, &dir).scan(&ModSettings::default());
    let err = mixed.mixed.clone().expect("a mixed set");
    assert_eq!(err.fabric, "`modmenu` (modmenu.jar)");
    assert!(err.neoforge.contains("jade"), "{err}");
    assert!(mixed.jars().is_empty(), "a mixed set loads nothing");
    assert_eq!(mixed.loader(), None);
    assert_eq!(
        status(&mixed, "modmenu"),
        &ModStatus::Excluded(err.to_string())
    );
}

#[test]
fn recognises_other_loaders_and_unreadable_jars() {
    let neo = Jar::new().file(
        "META-INF/neoforge.mods.toml",
        b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"cool\"\nversion=\"2.0\"\ndisplayName=\"Cool Neo\"\n",
    );
    let m = scan_one(&neo);
    assert_eq!(m.kind, ModKind::NeoForge);
    assert_eq!((m.name.as_str(), m.version.as_str()), ("Cool Neo", "2.0"));
    assert_eq!(
        scan_one(&Jar::new().file("META-INF/mods.toml", b"")).kind,
        ModKind::Forge
    );
    assert_eq!(
        scan_one(&Jar::new().file("quilt.mod.json", b"{}")).kind,
        ModKind::Quilt
    );
    assert!(matches!(
        scan_one(&Jar::new().file("a.txt", b"x")).kind,
        ModKind::Invalid(_)
    ));
    let garbage = scan::scan_reader(
        Path::new("g.jar"),
        "g.jar",
        Cursor::new(b"garbage".to_vec()),
    );
    assert!(matches!(garbage.kind, ModKind::Invalid(_)));
}

#[test]
fn bad_manifests_are_invalid_with_a_reason() {
    for json in [
        "{",
        r#"{"id":"demo"}"#,
        r#"{"id":"Bad Id","version":"1"}"#,
        r#"{"id":"x","version":"1"}"#,
        r#"{"version":"1"}"#,
    ] {
        let m = scan_one(&Jar::new().manifest(json));
        assert!(matches!(m.kind, ModKind::Invalid(_)), "{json}");
    }
}

#[test]
fn raw_newlines_in_strings_are_tolerated_like_fabric() {
    let jar = Jar::new().manifest(
        "{\"schemaVersion\":1,\"id\":\"demo\",\"version\":\"1.0\",\"description\":\"line one\nline two\"}",
    );
    let m = scan_one(&jar);
    assert!(!matches!(m.kind, ModKind::Invalid(_)), "{:?}", m.kind);
    assert_eq!(m.id, "demo");
    assert_eq!(m.description, "line one line two");
}

#[test]
fn text_fields_are_sanitised_and_bounded() {
    let jar = Jar::fabric_with(
        "demo",
        "1",
        &format!(
            r#""name":"§cRed\u0007 {}","description":"{}""#,
            "n".repeat(500),
            "d".repeat(5000)
        ),
    );
    let m = scan_one(&jar);
    assert!(m.name.starts_with("cRed "), "{}", m.name);
    assert!(!m.name.contains(['§', '\u{7}']));
    assert!(m.name.chars().count() <= 64);
    assert!(m.description.chars().count() <= 512);
}

#[test]
fn hostile_manifest_sizes_are_refused() {
    let big = format!(
        r#"{{"id":"demo","version":"1","pad":"{}"}}"#,
        "x".repeat(2 << 20)
    );
    let m = scan_one(&Jar::new().manifest(&big));
    let ModKind::Invalid(why) = m.kind else {
        panic!("{:?}", m.kind);
    };
    assert!(why.contains("limit"), "{why}");
}

#[test]
fn deep_json_does_not_overflow_the_stack() {
    let deep = format!(
        r#"{{"id":"demo","version":"1","x":{}{}}}"#,
        "[".repeat(5000),
        "]".repeat(5000)
    );
    let m = scan_one(&Jar::new().manifest(&deep));
    assert!(matches!(m.kind, ModKind::Invalid(_)));
}

#[test]
fn nested_jars_are_bounded_and_two_levels_deep() {
    let too_deep = Jar::fabric("level3", "1.0");
    let deep = Jar::fabric_with(
        "level2",
        "1.0",
        r#""jars":[{"file":"META-INF/jars/deeper.jar"}]"#,
    )
    .nested("META-INF/jars/deeper.jar", too_deep);
    let inner = Jar::fabric_with(
        "level1",
        "1.0",
        r#""jars":[{"file":"META-INF/jars/deep.jar"}]"#,
    )
    .nested("META-INF/jars/deep.jar", deep);
    let jar = Jar::fabric_with(
        "outer",
        "1",
        r#""jars":[{"file":"META-INF/jars/inner.jar"},{"file":"META-INF/jars/missing.jar"},{"file":"META-INF/jars/junk.jar"}]"#,
    )
    .nested("META-INF/jars/inner.jar", inner)
    .file("META-INF/jars/junk.jar", b"junk");
    let m = scan_one(&jar);
    let ids: Vec<&str> = m.nested.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["level1", "level2"]);

    let many: String = (0..scan::MAX_NESTED + 40)
        .map(|i| format!(r#"{{"file":"j{i}.jar"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let m = scan_one(&Jar::fabric_with(
        "outer",
        "1",
        &format!(r#""jars":[{many}]"#),
    ));
    assert!(m.nested.is_empty(), "absent entries read nothing");
}

#[test]
fn a_zip_bomb_entry_is_not_inflated() {
    // 8 MiB of zeros deflates to a few KiB; the declared size alone must refuse it.
    let bomb = vec![0u8; 8 << 20];
    let jar = Jar::new().manifest(std::str::from_utf8(&vec![b' '; 2 << 20]).unwrap());
    let m = scan_one(&jar);
    assert!(matches!(m.kind, ModKind::Invalid(_)));
    let icon_bomb = Jar::fabric_with("demo", "1", r#""icon":"i.png""#).file("i.png", &bomb);
    assert!(scan_one(&icon_bomb).icon.is_none());
}

#[test]
fn scan_dir_lists_jars_in_order_and_skips_the_rest() {
    let dir = temp_dir("scan");
    Jar::fabric("beta", "1").write(&dir, "b.jar");
    Jar::fabric("alpha", "1").write(&dir, "A.JAR");
    std::fs::write(dir.join("notes.txt"), "hi").unwrap();
    std::fs::create_dir_all(dir.join("sub.jar")).unwrap();
    std::fs::write(dir.join("broken.jar"), "nope").unwrap();
    let mods = scan::scan_dir(&dir);
    let names: Vec<&str> = mods.iter().map(|m| m.file_name.as_str()).collect();
    assert_eq!(names, ["A.JAR", "b.jar", "broken.jar"]);
    assert!(matches!(mods[2].kind, ModKind::Invalid(_)));
    assert!(scan::scan_dir(&dir.join("absent")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

fn status<'a>(report: &'a Report, id: &str) -> &'a ModStatus {
    &report
        .entries
        .iter()
        .find(|e| e.info.id == id)
        .unwrap_or_else(|| panic!("{id} not listed"))
        .status
}

#[test]
fn acceptance_five_jars() {
    let dir = temp_dir("accept");
    Jar::fabric_with(
        "fabric-api",
        "0.162.0+26.3",
        r#""depends":{"minecraft":"26.3"}"#,
    )
    .write(&dir, "fabric-api.jar");
    Jar::fabric_with(
        "modmenu",
        "21.0.0",
        r#""depends":{"fabric-api":"*","minecraft":">=26.3"}"#,
    )
    .write(&dir, "modmenu.jar");
    Jar::fabric("sodium", "0.9.0").write(&dir, "sodium.jar");
    Jar::new()
        .file("META-INF/neoforge.mods.toml", b"modId=\"neo\"\n")
        .write(&dir, "neo.jar");
    Jar::fabric_with("needy", "1.0.0", r#""depends":{"foo":"*"}"#).write(&dir, "needy.jar");

    let manager = Manager::new(&dir, &dir);
    let report = manager.scan(&ModSettings::default());
    assert_eq!(report.entries.len(), 5);
    assert_eq!(status(&report, "fabric-api"), &ModStatus::Active);
    assert_eq!(status(&report, "modmenu"), &ModStatus::Active);
    let ModStatus::Blocked(reason) = status(&report, "sodium") else {
        panic!("sodium not blocked");
    };
    assert!(reason.contains("OpenGL"), "{reason}");
    assert_eq!(
        status(&report, "needy"),
        &ModStatus::Excluded("missing dependency `foo`".into())
    );
    let neo = report
        .entries
        .iter()
        .find(|e| e.info.file_name == "neo.jar")
        .unwrap();
    let ModStatus::Unsupported(why) = &neo.status else {
        panic!("a NeoForge jar without [[mods]] loaded: {:?}", neo.status);
    };
    assert!(why.contains("no [[mods]] entry"), "{why}");
    assert_eq!(report.active_ids(), ["fabric-api", "modmenu"]);
    assert_eq!(report.loader(), Some(crate::Loader::Fabric));
    let jars = report.jars();
    assert_eq!(jars.len(), 2);
    assert!(jars.iter().all(|j| j.starts_with(&dir)));

    let mut settings = ModSettings::default();
    settings.set_enabled("modmenu", false);
    let report = manager.scan(&settings);
    assert_eq!(report.active_ids(), ["fabric-api"]);
    assert_eq!(status(&report, "modmenu"), &ModStatus::Disabled);

    settings.set_enabled("fabric-api", false);
    let report = manager.scan(&settings);
    assert!(report.jars().is_empty(), "nothing enabled starts no JVM");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn disabling_a_library_cascades_with_a_clear_reason() {
    let dir = temp_dir("cascade");
    Jar::fabric("lib", "1.0").write(&dir, "lib.jar");
    Jar::fabric_with("app", "1.0", r#""depends":{"lib":"*"}"#).write(&dir, "app.jar");
    let manager = Manager::new(&dir, &dir);
    let mut settings = ModSettings::default();
    settings.set_enabled("lib", false);
    let report = manager.scan(&settings);
    assert_eq!(
        status(&report, "app"),
        &ModStatus::Excluded("requires `lib`, which is disabled".into())
    );
    assert!(report.jars().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unsupported_policy_mods_need_explicit_enabling() {
    let dir = temp_dir("unsupported");
    Jar::fabric("voicechat", "1.0").write(&dir, "voicechat.jar");
    Jar::fabric("continuity", "1.0").write(&dir, "continuity.jar");
    let manager = Manager::new(&dir, &dir);
    let mut settings = ModSettings::default();
    let report = manager.scan(&settings);
    assert_eq!(status(&report, "voicechat"), &ModStatus::Disabled);
    assert_eq!(status(&report, "continuity"), &ModStatus::Active);
    settings.set_enabled("voicechat", true);
    let report = manager.scan(&settings);
    assert_eq!(status(&report, "voicechat"), &ModStatus::Active);
    let voicechat = report
        .entries
        .iter()
        .find(|e| e.info.id == "voicechat")
        .unwrap();
    assert_eq!(
        voicechat.policy.as_ref().unwrap().status,
        PolicyStatus::Unsupported
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn blocked_mods_cannot_be_forced_on() {
    let dir = temp_dir("blocked");
    Jar::fabric("iris", "1.0").write(&dir, "iris.jar");
    let manager = Manager::new(&dir, &dir);
    let mut settings = ModSettings::default();
    settings.set_enabled("iris", true);
    let report = manager.scan(&settings);
    assert!(matches!(status(&report, "iris"), ModStatus::Blocked(_)));
    assert!(report.jars().is_empty());
    assert!(!report.entries[0].toggleable());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn server_only_mods_are_listed_not_loaded() {
    let dir = temp_dir("server");
    Jar::fabric_with("srv", "1.0", r#""environment":"server""#).write(&dir, "srv.jar");
    let report = Manager::new(&dir, &dir).scan(&ModSettings::default());
    assert_eq!(status(&report, "srv"), &ModStatus::ServerOnly);
    assert!(report.jars().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn disabled_all_keeps_choices_but_loads_nothing() {
    let dir = temp_dir("disabled-all");
    Jar::fabric("lib", "1.0").write(&dir, "lib.jar");
    let manager = Manager::new(&dir, &dir);
    let mut settings = ModSettings::default();
    settings.disabled_all = true;
    let report = manager.scan(&settings);
    assert!(report.jars().is_empty());
    assert!(report.entries[0].enabled);
    assert_eq!(report.entries[0].status, ModStatus::Disabled);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settings_round_trip_through_the_manager_path() {
    let dir = temp_dir("settings");
    let manager = Manager::new(&dir, &dir);
    let mut s = manager.load_settings();
    s.set_enabled("a", false);
    s.save(&manager.settings_path()).unwrap();
    assert!(!manager.load_settings().is_enabled("a", true));
    let _ = std::fs::remove_dir_all(&dir);
}
