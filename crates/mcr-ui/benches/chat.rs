use std::path::PathBuf;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use mcr_protocol::types::text::{Arg, Color, Component, Content, NamedColor, Style};
use mcr_ui::{ChatHud, ChatOptions, Ui, vanilla_resources};

fn jar() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::env::var_os("MCR_CLIENT_JAR")
        .map(PathBuf::from)
        .or_else(|| {
            manifest
                .ancestors()
                .map(|d| d.join(".cache/versions/26.3/client.jar"))
                .find(|p| p.exists())
        })
}

fn messages() -> Vec<Component> {
    (0..200)
        .map(|i| {
            let name = Component::text(format!("Player{i}")).styled(Style {
                color: Some(Color::Named(NamedColor::Aqua)),
                ..Default::default()
            });
            Component::from_content(Content::Translatable {
                key: "chat.type.text".into(),
                fallback: None,
                args: vec![
                    Arg::Component(name),
                    Arg::Str(format!(
                        "message {i}: the quick brown fox jumps over the lazy dog and keeps running until the line wraps"
                    )),
                ],
            })
        })
        .collect()
}

fn bench(c: &mut Criterion) {
    let Some(jar) = jar() else {
        eprintln!("skipping: no client jar");
        return;
    };
    let mut ui = Ui::load(&vanilla_resources(&jar, None).unwrap()).unwrap();
    let options = ChatOptions {
        visible_lines: 400,
        ..Default::default()
    };
    let messages = messages();

    c.bench_function("chat_layout_200_messages", |b| {
        b.iter(|| {
            let mut chat = ChatHud::default();
            for (tick, m) in messages.iter().enumerate() {
                chat.add(&mut ui, m, tick as u32, &options);
            }
            black_box(chat.len())
        })
    });

    let mut chat = ChatHud::default();
    for m in &messages {
        chat.add(&mut ui, m, 0, &options);
    }
    c.bench_function("chat_batch_visible_lines", |b| {
        b.iter(|| {
            let mut frame = ui.frame(1920, 1080);
            chat.draw(&mut frame, 0, &options);
        });
    });

    c.bench_function("chat_layout_and_batch_200_messages", |b| {
        b.iter(|| {
            let mut chat = ChatHud::default();
            for (tick, m) in messages.iter().enumerate() {
                chat.add(&mut ui, m, tick as u32, &options);
            }
            let mut frame = ui.frame(1920, 1080);
            chat.draw(&mut frame, 0, &options);
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
