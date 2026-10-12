use std::path::PathBuf;

use crate::abi::{
    REPORT_CONNECT, REPORT_CRASH, REPORT_INFO, REPORT_INIT, REPORT_KEYS, REPORT_MOD_LOADED,
    REPORT_SCREEN, REPORT_STOPPING, SYNC_MAX_LINES,
};

/// Longest mapping or key name accepted from Java.
const MAX_KEY_NAME: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    ModLoaded {
        id: String,
        version: String,
    },
    /// A client entrypoint of the bridge mod finished (the payload names it).
    Initialized(String),
    /// The screen Java now shows (class name), empty for none.
    Screen(String),
    Crash {
        report: Option<PathBuf>,
    },
    Info {
        key: String,
        value: String,
    },
    ConnectRequested {
        address: String,
        server: String,
    },
    Stopping,
    /// Vanilla key mappings Java rebound: `(mapping name, key name)` in vanilla's `options.txt` spelling.
    KeysChanged(Vec<(String, String)>),
    /// Java missed a frame deadline or died; mcr stopped composing its GUI.
    RenderDetached {
        reason: String,
    },
    /// Java recovered from a stall; its GUI and world layers are composed again.
    RenderResumed,
    Unknown {
        kind: i32,
        text: String,
    },
    /// Java called `System.exit`; the VM is frozen for good.
    VmExited {
        code: i32,
    },
    /// The boot call returned and the VM was torn down (or failed to be).
    VmFinished {
        boot_ok: bool,
        destroyed: bool,
    },
}

impl BridgeEvent {
    pub(crate) fn from_report(kind: i32, text: &str) -> Self {
        let (head, tail) = text.split_once('\t').unwrap_or((text, ""));
        match kind {
            REPORT_MOD_LOADED => Self::ModLoaded {
                id: head.into(),
                version: tail.into(),
            },
            REPORT_INIT => Self::Initialized(text.into()),
            REPORT_SCREEN => Self::Screen(text.into()),
            REPORT_CRASH => Self::Crash {
                report: (!text.is_empty()).then(|| PathBuf::from(text)),
            },
            REPORT_INFO => {
                let (key, value) = text.split_once('=').unwrap_or((text, ""));
                Self::Info {
                    key: key.into(),
                    value: value.into(),
                }
            }
            REPORT_CONNECT => Self::ConnectRequested {
                address: head.into(),
                server: tail.into(),
            },
            REPORT_STOPPING => Self::Stopping,
            REPORT_KEYS => Self::KeysChanged(parse_keys(text)),
            _ => Self::Unknown {
                kind,
                text: text.into(),
            },
        }
    }

    pub(crate) fn log(&self) {
        match self {
            Self::Crash { report } => tracing::error!(?report, "java crashed; bridge detached"),
            Self::RenderDetached { reason } => tracing::error!(reason, "java gui detached"),
            Self::VmExited { code } => {
                tracing::error!(code, "java called System.exit; bridge detached")
            }
            Self::Unknown { kind, text } => tracing::warn!(kind, text, "unknown bridge report"),
            other => tracing::info!(event = ?other, "bridge"),
        }
    }

    pub fn is_title_screen(&self) -> bool {
        matches!(self, Self::Screen(class) if class == "net.minecraft.client.gui.screens.TitleScreen")
    }
}

/// `name\tkey` lines, bounded in count and length; malformed lines are skipped.
pub(crate) fn parse_keys(text: &str) -> Vec<(String, String)> {
    text.lines()
        .take(SYNC_MAX_LINES)
        .filter_map(|line| {
            let (name, key) = line.split_once('\t')?;
            let ok = |s: &str| {
                !s.is_empty() && s.len() <= MAX_KEY_NAME && s.bytes().all(|b| b.is_ascii_graphic())
            };
            (ok(name) && ok(key)).then(|| (name.to_owned(), key.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reports() {
        assert_eq!(
            BridgeEvent::from_report(REPORT_INFO, "sdl_driver=offscreen"),
            BridgeEvent::Info {
                key: "sdl_driver".into(),
                value: "offscreen".into()
            }
        );
        assert_eq!(
            BridgeEvent::from_report(REPORT_CRASH, ""),
            BridgeEvent::Crash { report: None }
        );
        assert_eq!(
            BridgeEvent::from_report(REPORT_CONNECT, "localhost:25565\tMy server"),
            BridgeEvent::ConnectRequested {
                address: "localhost:25565".into(),
                server: "My server".into()
            }
        );
        assert!(
            BridgeEvent::from_report(
                REPORT_SCREEN,
                "net.minecraft.client.gui.screens.TitleScreen"
            )
            .is_title_screen()
        );
        assert_eq!(
            BridgeEvent::from_report(
                REPORT_KEYS,
                "key.jump\tkey.keyboard.j\nbad\n\tx\nkey.a b\tk\n"
            ),
            BridgeEvent::KeysChanged(vec![("key.jump".into(), "key.keyboard.j".into())])
        );
        let many = "key.x\tkey.keyboard.x\n".repeat(SYNC_MAX_LINES + 10);
        assert!(matches!(
            BridgeEvent::from_report(REPORT_KEYS, &many),
            BridgeEvent::KeysChanged(v) if v.len() == SYNC_MAX_LINES
        ));
        assert_eq!(
            BridgeEvent::from_report(99, "x"),
            BridgeEvent::Unknown {
                kind: 99,
                text: "x".into()
            }
        );
    }
}
