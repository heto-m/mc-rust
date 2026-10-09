use mcr_buf::{Decode, Encode, Identifier, Result};

use super::common::{ItemStackTemplate, Pairs};
use crate::{packet, types::Component};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AdvancementFrame {
    Task,
    Challenge,
    Goal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    pub title: Component,
    pub description: Component,
    pub icon: ItemStackTemplate,
    pub frame: AdvancementFrame,
    pub background: Option<Identifier>,
    pub show_toast: bool,
    pub hidden: bool,
}

const FLAG_BACKGROUND: i32 = 1;
const FLAG_SHOW_TOAST: i32 = 2;
const FLAG_HIDDEN: i32 = 4;

impl Encode for DisplayInfo {
    fn encode(&self, out: &mut Vec<u8>) {
        self.title.encode(out);
        self.description.encode(out);
        self.icon.encode(out);
        self.frame.encode(out);
        let mut flags = 0;
        if self.background.is_some() {
            flags |= FLAG_BACKGROUND;
        }
        if self.show_toast {
            flags |= FLAG_SHOW_TOAST;
        }
        if self.hidden {
            flags |= FLAG_HIDDEN;
        }
        flags.encode(out);
        if let Some(background) = &self.background {
            background.encode(out);
        }
    }
}

impl Decode<'_> for DisplayInfo {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let title = Component::decode(input)?;
        let description = Component::decode(input)?;
        let icon = ItemStackTemplate::decode(input)?;
        let frame = AdvancementFrame::decode(input)?;
        let flags = i32::decode(input)?;
        let background = if flags & FLAG_BACKGROUND != 0 {
            Some(Identifier::decode(input)?)
        } else {
            None
        };
        Ok(Self {
            title,
            description,
            icon,
            frame,
            background,
            show_toast: flags & FLAG_SHOW_TOAST != 0,
            hidden: flags & FLAG_HIDDEN != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Advancement {
    pub parent: Option<Identifier>,
    pub display: Option<DisplayInfo>,
    pub requirements: Vec<Vec<String>>,
    pub sends_telemetry_event: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PositionedAdvancement {
    pub id: Identifier,
    pub advancement: Advancement,
    pub x: f32,
    pub y: f32,
}

/// Criterion name to the epoch-millisecond instant it was obtained, if it was.
pub type AdvancementProgress = Pairs<String, Option<i64>>;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UpdateAdvancements {
    pub reset: bool,
    pub added: Vec<PositionedAdvancement>,
    pub removed: Vec<Identifier>,
    pub progress: Pairs<Identifier, AdvancementProgress>,
    pub show_advancements: bool,
}

packet!(UpdateAdvancements, "update_advancements", Clientbound);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SelectAdvancementsTab {
    pub tab: Option<Identifier>,
}

packet!(
    SelectAdvancementsTab,
    "select_advancements_tab",
    Clientbound
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{play::clientbound::ui::common::ComponentPatch, test_util::roundtrip};

    fn plain(display: Option<DisplayInfo>) -> Advancement {
        Advancement {
            parent: None,
            display,
            requirements: vec![vec!["x".into()]],
            sends_telemetry_event: false,
        }
    }

    #[test]
    fn update_bytes() {
        let packet = UpdateAdvancements {
            reset: true,
            added: vec![PositionedAdvancement {
                id: Identifier::parse("a:b").unwrap(),
                advancement: plain(None),
                x: 1.0,
                y: 0.0,
            }],
            removed: vec![],
            progress: Pairs(vec![]),
            show_advancements: false,
        };
        #[rustfmt::skip]
        assert_eq!(roundtrip(&packet), [
            1,
            1, 3, b'a', b':', b'b', 0, 0, 1, 1, 1, b'x', 0, 0x3f, 0x80, 0, 0, 0, 0, 0,
            0, 0, 0, 0,
        ]);
    }

    #[test]
    fn progress_and_removed() {
        let packet = UpdateAdvancements {
            reset: false,
            added: vec![],
            removed: vec![Identifier::parse("a:b").unwrap()],
            progress: Pairs(vec![(
                Identifier::parse("a:c").unwrap(),
                Pairs(vec![("done".into(), Some(5)), ("todo".into(), None)]),
            )]),
            show_advancements: true,
        };
        #[rustfmt::skip]
        assert_eq!(roundtrip(&packet), [
            0, 0,
            1, 3, b'a', b':', b'b',
            1, 3, b'a', b':', b'c',
            2,
            4, b'd', b'o', b'n', b'e', 1, 0, 0, 0, 0, 0, 0, 0, 5,
            4, b't', b'o', b'd', b'o', 0,
            1,
        ]);
    }

    #[test]
    fn select_tab() {
        assert_eq!(roundtrip(&SelectAdvancementsTab { tab: None }), [0]);
        let tab = SelectAdvancementsTab {
            tab: Some(Identifier::parse("a:b").unwrap()),
        };
        assert_eq!(roundtrip(&tab), [1, 3, b'a', b':', b'b']);
    }

    #[test]
    #[ignore = "needs Component and data component patch"]
    fn display_info_roundtrip() {
        let display = DisplayInfo {
            title: Component::default(),
            description: Component::default(),
            icon: ItemStackTemplate {
                item: 1,
                count: 1,
                components: ComponentPatch::default(),
            },
            frame: AdvancementFrame::Goal,
            background: Some(Identifier::parse("textures/bg.png").unwrap()),
            show_toast: true,
            hidden: true,
        };
        roundtrip(&display);
        roundtrip(&plain(Some(display)));
    }
}
