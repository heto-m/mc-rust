mod common;

use mcr_protocol::types::text::Component;
use mcr_ui::{ChatClick, ChatHud, ChatLineTag, ChatOptions};

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

fn link(text: &str) -> Component {
    Component::text(text).styled(mcr_protocol::types::text::Style {
        click_event: Some(Box::new(mcr_protocol::types::text::ClickEvent::RunCommand(
            "/trigger menu".into(),
        ))),
        insertion: Some("Alex".into()),
        ..Default::default()
    })
}

#[test]
fn scale_and_spacing_move_lines_like_vanilla() {
    assert_eq!(ChatOptions::lines_for_height(180, 0.0), 20);
    assert_eq!(ChatOptions::lines_for_height(180, 1.0), 10);
    assert_eq!(
        ChatOptions {
            line_spacing: 0.5,
            ..ChatOptions::default()
        }
        .line_height(),
        13
    );
    let Some(mut ui) = common::ui() else {
        return;
    };
    let wide = ui.text_width("clickable text here");
    let plain = ChatOptions {
        focused: true,
        ..ChatOptions::default()
    };
    let scaled = ChatOptions {
        scale: 0.5,
        line_spacing: 1.0,
        ..plain
    };
    let mut chat = ChatHud::default();
    chat.add(&mut ui, &link("clickable text here"), 0, &plain);
    let far_x = 4.0 + wide * 0.75;
    assert!(chat.click_event_at(far_x, 196.0, 240, &plain).is_some());
    assert!(chat.click_event_at(far_x, 196.0, 240, &scaled).is_none());
    // Scaled chat space: bottom 400, line 18 high, text from 388 to 397, so GUI y 194..198.5.
    assert!(chat.click_event_at(5.0, 196.0, 240, &scaled).is_some());
    assert!(chat.click_event_at(5.0, 199.0, 240, &scaled).is_none());
    assert!(chat.click_event_at(5.0, 192.5, 240, &scaled).is_none());
    let hidden = ChatOptions {
        scale: 0.0,
        ..plain
    };
    assert!(chat.click_event_at(5.0, 196.0, 240, &hidden).is_none());
}

#[test]
fn shift_click_inserts_and_plain_click_runs() {
    let Some(mut ui) = common::ui() else {
        return;
    };
    let options = ChatOptions {
        focused: true,
        ..ChatOptions::default()
    };
    let mut chat = ChatHud::default();
    chat.add(&mut ui, &link("name"), 0, &options);
    assert_eq!(
        chat.click_at((6.0, 196.0), 240, &options, true, 0.0),
        Some(ChatClick::Insert("Alex".into()))
    );
    assert!(matches!(
        chat.click_at((6.0, 196.0), 240, &options, false, 0.0),
        Some(ChatClick::Event(_))
    ));
}

#[test]
fn delayed_messages_wait_for_the_interval_or_a_click() {
    let Some(mut ui) = common::ui() else {
        return;
    };
    let options = ChatOptions::default();
    let mut chat = ChatHud::default();
    let msg = Component::text("<Alex> hi");
    chat.add_delayed(&mut ui, &msg, 0, &options, None, Some([3; 256]), (0, 2000));
    chat.add_delayed(&mut ui, &msg, 0, &options, None, None, (0, 2000));
    assert_eq!((chat.len(), chat.queue_len()), (0, 2));
    chat.tick_queue(&mut ui, 1, &options, (1500, 2000));
    assert_eq!((chat.len(), chat.queue_len()), (0, 2));
    chat.tick_queue(&mut ui, 2, &options, (2000, 2000));
    assert_eq!((chat.len(), chat.queue_len()), (1, 1));
    chat.tick_queue(&mut ui, 3, &options, (3000, 2000));
    assert_eq!(chat.queue_len(), 1);
    chat.accept_next(&mut ui, 4, &options, 3000);
    assert_eq!((chat.len(), chat.queue_len()), (2, 0));
    chat.add_delayed(
        &mut ui,
        &msg,
        5,
        &options,
        None,
        Some([4; 256]),
        (3000, 2000),
    );
    assert!(chat.remove_queued(&[4; 256]));
    assert!(!chat.remove_queued(&[4; 256]));
    chat.add_delayed(&mut ui, &msg, 5, &options, None, None, (3000, 2000));
    chat.tick_queue(&mut ui, 6, &options, (3001, 0));
    assert_eq!((chat.len(), chat.queue_len()), (3, 0));
    let queued = ChatOptions {
        focused: true,
        ..options
    };
    chat.add_delayed(&mut ui, &msg, 7, &options, None, None, (4000, 2000));
    let width = chat.queue_width(&mut ui);
    assert!(width > 0.0);
    assert_eq!(
        chat.click_at((6.0, 202.0), 240, &queued, false, width),
        Some(ChatClick::ExpandQueue)
    );
}

#[test]
fn shown_delayed_messages_are_released_for_acknowledgement() {
    let Some(mut ui) = common::ui() else {
        return;
    };
    let options = ChatOptions::default();
    let mut chat = ChatHud::default();
    let msg = Component::text("<Alex> hi");
    chat.add_delayed(&mut ui, &msg, 0, &options, None, Some([3; 256]), (0, 2000));
    chat.add_delayed(&mut ui, &msg, 0, &options, None, Some([4; 256]), (0, 2000));
    chat.tick_queue(&mut ui, 1, &options, (1000, 2000));
    assert_eq!(chat.take_released(), (vec![], false));
    chat.tick_queue(&mut ui, 2, &options, (2000, 2000));
    assert_eq!(chat.take_released(), (vec![[3; 256]], false));
    chat.accept_next(&mut ui, 3, &options, 2500);
    assert_eq!(chat.take_released(), (vec![[4; 256]], false));
    chat.tick_queue(&mut ui, 4, &options, (4000, 2000));
    assert_eq!(chat.take_released(), (vec![], false), "not due yet");
    chat.tick_queue(&mut ui, 5, &options, (4500, 2000));
    assert_eq!(chat.take_released(), (vec![], true), "polled empty");
}

#[test]
fn commands_persist_in_the_history() {
    let mut chat = ChatHud::default();
    chat.load_command_history("/one\n/two\n");
    assert_eq!(chat.recent(), ["/one", "/two"]);
    chat.add_recent("hello");
    assert!(chat.take_command_history().is_none());
    chat.add_recent("/three");
    chat.add_recent("/three");
    assert_eq!(chat.take_command_history().unwrap(), "/one\n/two\n/three\n");
    for i in 0..60 {
        chat.add_recent(&format!("/c{i}"));
    }
    let saved = chat.take_command_history().unwrap();
    assert_eq!(saved.lines().count(), 50);
    assert!(saved.ends_with("/c59\n"));
}
