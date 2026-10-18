mod common;

use mcr_protocol::types::text::Component;
use mcr_ui::{ChatHud, ChatLineTag, ChatOptions};

fn modified() -> ChatLineTag {
    ChatLineTag::Modified {
        original: "hi".into(),
    }
}

#[test]
fn tags_follow_their_lines_and_deletion_waits_sixty_ticks() {
    let Some(mut ui) = common::ui() else {
        return;
    };
    let options = ChatOptions::default();
    let mut chat = ChatHud::default();
    chat.add_tagged(
        &mut ui,
        &Component::text("<Alex> hello"),
        0,
        &options,
        Some(ChatLineTag::NotSecure),
        Some([1; 256]),
    );
    chat.add_tagged(
        &mut ui,
        &Component::text("<Steve> hi"),
        10,
        &options,
        None,
        Some([2; 256]),
    );
    chat.add_text(&mut ui, "joined", 20, &options);
    let lines = chat.line_texts();
    assert_eq!(lines[1], ("<Steve> hi".into(), None));
    assert_eq!(lines[2].1, Some(ChatLineTag::NotSecure));

    chat.delete(&mut ui, &[2; 256], 30, &options);
    assert_eq!(
        chat.line_texts()[1].0,
        "<Steve> hi",
        "too young to delete yet"
    );
    chat.process_deletions(&mut ui, 69, &options);
    assert_eq!(chat.line_texts()[1].0, "<Steve> hi");
    chat.process_deletions(&mut ui, 70, &options);
    let lines = chat.line_texts();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[1].1, Some(ChatLineTag::System));
    assert_ne!(lines[1].0, "<Steve> hi");
    assert_eq!(lines[0].0, "joined");

    chat.delete(&mut ui, &[1; 256], 100, &options);
    assert_eq!(chat.line_texts()[2].1, Some(ChatLineTag::System));
    chat.delete(&mut ui, &[9; 256], 100, &options);
    assert_eq!(chat.len(), 3);
}

#[test]
fn modified_lines_leave_room_for_the_icon() {
    let Some(mut ui) = common::ui() else {
        return;
    };
    let options = ChatOptions {
        width: 60,
        ..ChatOptions::default()
    };
    let text = Component::text("aaaa bbbb cccc dddd");
    let mut plain = ChatHud::default();
    plain.add(&mut ui, &text, 0, &options);
    let mut tagged = ChatHud::default();
    tagged.add_tagged(&mut ui, &text, 0, &options, Some(modified()), None);
    assert!(tagged.len() >= plain.len());
}

#[test]
fn hovering_the_bar_shows_the_tag_text() {
    let Some(mut ui) = common::ui() else {
        return;
    };
    let options = ChatOptions {
        focused: true,
        ..ChatOptions::default()
    };
    let mut chat = ChatHud::default();
    chat.add_tagged(
        &mut ui,
        &Component::text("hello"),
        0,
        &options,
        Some(modified()),
        None,
    );
    let mut frame = ui.frame(640, 480);
    let (_, h) = frame.gui_size();
    let row_y = (h - 40 - 4) as f32;
    let tip = chat
        .hover(&mut frame, (1.0, row_y), &options)
        .expect("tooltip");
    assert_eq!(tip.len(), 2);
    assert_eq!(tip.first_plain(), "chat.tag.modified");
    assert!(chat.hover(&mut frame, (200.0, row_y), &options).is_none());
    assert!(
        chat.hover(&mut frame, (1.0, (h - 80) as f32), &options)
            .is_none()
    );
    assert_eq!(ChatLineTag::Error.indicator_color(), 0xFF_5555);
    assert_eq!(
        ChatLineTag::NotSecure.tooltip().first_plain(),
        "chat.tag.not_secure"
    );
}
