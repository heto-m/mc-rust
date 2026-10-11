mod common;

use common::harness::{Harness, SIZE, send, typed};
use mcr_buf::BlockPos;
use mcr_protocol::play::serverbound::{
    CommandBlockMode, JointType, Mirror, Rotation, StructureMode, TestBlockMode,
};
use mcr_protocol::types::text::{ClickEvent, Color, Component, Style};
use mcr_ui::{
    BookEditScreen, BookViewScreen, CommandBlockInfo, CommandBlockScreen, Dialog, DialogScreen,
    JigsawBlockScreen, JigsawInfo, Key, MouseButton, NoView, Screen, ScreenAction, SignEditScreen,
    SignInfo, StructureBlockScreen, StructureInfo, TestBlockScreen, TestInstanceBlockScreen,
    TestInstanceInfo, UiInput,
};

fn sign(hanging: bool, wall: bool) -> SignEditScreen {
    SignEditScreen::new(SignInfo {
        wood: if hanging { "oak" } else { "spruce" }.into(),
        hanging,
        wall,
        lines: [
            "Hello".into(),
            "wide sign text here".into(),
            String::new(),
            "last".into(),
        ],
        text_color: 0xFF00_0000,
    })
}

#[test]
fn sign_shots_and_width_limit() {
    let Some(mut h) = Harness::new() else { return };
    let mut s = sign(false, false);
    s.init(427, 240);
    send(
        &mut s,
        &NoView,
        &[UiInput::key(Key::Down), UiInput::key(Key::Down)],
    );
    send(&mut s, &NoView, &typed("0123456789abcdefghijklmnop"));
    h.shot_inited("edit_sign.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let text = s.text();
    assert!(h.ui.text_width(&text[2]) <= 90.0, "{}", text[2]);
    assert!(text[2].len() > 8);
    let mut s = sign(true, false);
    s.init(427, 240);
    send(&mut s, &NoView, &typed("0123456789abcdefghijklmnop"));
    h.shot_inited("edit_hanging_sign.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    assert!(h.ui.text_width(&s.text()[0]) <= 60.0);
    let mut s = sign(false, true);
    h.shot("edit_wall_sign.png", SIZE, &mut s, &NoView, (0.0, 0.0));
}

#[test]
fn book_shots() {
    let Some(mut h) = Harness::new() else { return };
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\nSecond paragraph with more words to wrap around the page edge.";
    let mut s = BookEditScreen::new("Steve", vec![text.into(), "page two".into()]);
    h.shot("edit_book.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let mut s = BookEditScreen::new("Steve", vec!["abc".into()]);
    s.init(427, 240);
    let (x, y) = ((427 / 2 - 98 - 2 + 40) as f32, 206.0);
    send(
        &mut s,
        &NoView,
        &[
            UiInput::press(x, y, MouseButton::Left),
            UiInput::release(x, y, MouseButton::Left),
        ],
    );
    send(&mut s, &NoView, &typed("My Book"));
    h.shot_inited("edit_book_sign.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let link = Component::text("click me").styled(Style {
        click_event: Some(Box::new(ClickEvent::OpenUrl(
            "https://minecraft.wiki".into(),
        ))),
        color: Some(Color::Rgb(0x0000AA)),
        underlined: Some(true),
        ..Default::default()
    });
    let pages = vec![
        Component::text("Written book page one. It has enough text to wrap onto several lines.\n")
            .with(link),
        Component::text("Page two"),
    ];
    let mut v = BookViewScreen::new(pages);
    h.shot("view_book.png", SIZE, &mut v, &NoView, (0.0, 0.0));
}

#[test]
fn command_block_shots() {
    let Some(mut h) = Harness::new() else { return };
    let mut s = CommandBlockScreen::block(CommandBlockInfo {
        pos: BlockPos::new(1, 2, 3),
        command: "say hello world".into(),
        mode: CommandBlockMode::Auto,
        conditional: true,
        automatic: false,
        track_output: true,
        last_output: "[@] hello world".into(),
    });
    h.shot("edit_command_block.png", SIZE, &mut s, &NoView, (0.0, 0.0));
}

fn structure(mode: StructureMode) -> StructureBlockScreen {
    StructureBlockScreen::new(StructureInfo {
        pos: BlockPos::new(0, 64, 0),
        name: "mystruct".into(),
        mode,
        offset: [0, 1, 0],
        size: [5, 6, 7],
        mirror: Mirror::None,
        rotation: Rotation::Clockwise90,
        data: "marker".into(),
        ignore_entities: true,
        strict: false,
        show_air: false,
        show_bounding_box: true,
        integrity: 0.5,
        seed: 42,
    })
}

#[test]
fn structure_jigsaw_test_shots() {
    let Some(mut h) = Harness::new() else { return };
    for (name, mode) in [
        ("save", StructureMode::Save),
        ("load", StructureMode::Load),
        ("corner", StructureMode::Corner),
        ("data", StructureMode::Data),
    ] {
        let mut s = structure(mode);
        h.shot(
            &format!("edit_structure_{name}.png"),
            SIZE,
            &mut s,
            &NoView,
            (0.0, 0.0),
        );
    }
    let mut s = JigsawBlockScreen::new(JigsawInfo {
        pos: BlockPos::new(0, 0, 0),
        name: "minecraft:bottom".into(),
        target: "minecraft:bottom".into(),
        pool: "minecraft:village/plains/houses".into(),
        final_state: "minecraft:structure_void".into(),
        joint: JointType::Aligned,
        selection_priority: 1,
        placement_priority: 2,
        vertical: true,
    });
    h.shot("edit_jigsaw.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let mut s = TestBlockScreen::new(BlockPos::new(0, 0, 0), TestBlockMode::Log, "message");
    h.shot("edit_test_block.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let mut s = TestInstanceBlockScreen::new(
        TestInstanceInfo {
            pos: BlockPos::new(0, 0, 0),
            test: "minecraft:example".into(),
            size: [3, 3, 3],
            rotation: Rotation::None,
            ignore_entities: false,
            error: Some(Component::text("boom")),
        },
        Default::default(),
    );
    h.shot("edit_test_instance.png", SIZE, &mut s, &NoView, (0.0, 0.0));
}

fn dialog(snbt: &str) -> DialogScreen {
    let value = mcr_nbt::snbt::parse(snbt).expect("snbt");
    DialogScreen::new(Dialog::from_nbt(&value).expect("dialog"), Vec::new())
}

#[test]
fn dialog_shots_and_actions() {
    let Some(mut h) = Harness::new() else { return };
    let mut s = dialog(
        r#"{type:"minecraft:confirmation",title:{text:"Server Rules",color:"gold"},
        body:[{type:"minecraft:plain_message",contents:"Please read the rules and tell us your name."},
              {type:"minecraft:plain_message",contents:{text:"Click here",click_event:{action:"open_url",url:"https://minecraft.wiki"},color:"aqua",underlined:1b}}],
        inputs:[{type:"text",key:"name",label:"Your name",initial:"Steve",max_length:20},
                {type:"boolean",key:"agree",label:"I agree",initial:1b,on_true:"yes",on_false:"no"},
                {type:"single_option",key:"color",label:"Color",options:[{id:"red",display:"Red"},{id:"blue",display:"Blue",initial:1b}]},
                {type:"number_range",key:"level",label:"Level",start:0f,end:10f,initial:5f,step:1f}],
        yes:{label:{text:"Accept",color:"green"},action:{type:"minecraft:dynamic/run_command",template:"say $(name) $(agree) $(color) $(level)"}},
        no:{label:"Decline",action:{type:"minecraft:dynamic/custom",id:"srv:decline",additions:{x:1b}}}}"#,
    );
    h.shot("dialog_confirmation.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let mut s = dialog(
        r#"{type:"minecraft:multi_action",title:"Menu",columns:2,pause:0b,after_action:"wait_for_response",
        body:{type:"minecraft:item",item:{id:"minecraft:diamond",count:3},description:{contents:"A shiny item",width:100}},
        actions:[{label:"One"},{label:"Two",tooltip:"Tip"},{label:"Three"},{label:"Four"},{label:"Five"}],
        exit_action:{label:"Close"}}"#,
    );
    h.shot("dialog_multi.png", SIZE, &mut s, &NoView, (0.0, 0.0));
    let out = send(&mut s, &NoView, &[UiInput::key(Key::Escape)]);
    assert!(matches!(out.as_slice(), [ScreenAction::Pop]));
}
