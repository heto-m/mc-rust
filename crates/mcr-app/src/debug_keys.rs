//! `KeyboardHandler`'s F3 chords: the overlay toggles on release only when F3 was not used as a modifier.

use mcr_settings::{InputKey, KeyBindings, MAPPINGS};

use crate::input::KEY_ESCAPE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    ReloadChunks,
    ShowHitboxes,
    ClearChat,
    ShowChunkBorders,
    AdvancedTooltips,
    CopyRecreateCommand,
    Spectate,
    SwitchGameMode,
    DebugOptions,
    FocusPause,
    DumpDynamicTextures,
    ReloadResourcePacks,
    Profiling,
    CopyLocation,
    DumpVersion,
    ProfilingChart,
    FpsCharts,
    NetworkCharts,
    LightmapTexture,
    ImprovedTransparency,
}

impl DebugAction {
    fn from_mapping(name: &str) -> Option<Self> {
        Some(match name.strip_prefix("key.debug.")? {
            "reloadChunk" => Self::ReloadChunks,
            "showHitboxes" => Self::ShowHitboxes,
            "clearChat" => Self::ClearChat,
            "showChunkBorders" => Self::ShowChunkBorders,
            "showAdvancedTooltips" => Self::AdvancedTooltips,
            "copyRecreateCommand" => Self::CopyRecreateCommand,
            "spectate" => Self::Spectate,
            "switchGameMode" => Self::SwitchGameMode,
            "debugOptions" => Self::DebugOptions,
            "focusPause" => Self::FocusPause,
            "dumpDynamicTextures" => Self::DumpDynamicTextures,
            "reloadResourcePacks" => Self::ReloadResourcePacks,
            "profiling" => Self::Profiling,
            "copyLocation" => Self::CopyLocation,
            "dumpVersion" => Self::DumpVersion,
            "profilingChart" => Self::ProfilingChart,
            "fpsCharts" => Self::FpsCharts,
            "networkCharts" => Self::NetworkCharts,
            "lightmapTexture" => Self::LightmapTexture,
            "improvedTransparency" => Self::ImprovedTransparency,
            _ => return None,
        })
    }
}

/// What a key press did besides (or instead of) its ordinary bindings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugPress {
    pub toggle_overlay: bool,
    /// F3+Esc: pause without the menu.
    pub pause_without_menu: bool,
    pub actions: Vec<DebugAction>,
}

impl DebugPress {
    /// A chord fired, so the key does not also act as a game key.
    pub fn consumed(&self) -> bool {
        self.pause_without_menu || !self.actions.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct DebugKeys {
    modifier_down: bool,
    used_as_modifier: bool,
}

fn index(name: &str) -> usize {
    mcr_settings::bindings::mapping_index(name).expect("vanilla debug mapping")
}

impl DebugKeys {
    pub fn modifier_down(&self) -> bool {
        self.modifier_down
    }

    /// A physical press. `chords` is false while a screen has the keyboard: only F3's own state is tracked.
    pub fn press(&mut self, bindings: &KeyBindings, key: InputKey, chords: bool) -> DebugPress {
        let modifier = bindings.input(index("key.debug.modifier"));
        let overlay = bindings.input(index("key.debug.overlay"));
        let same = modifier == overlay;
        let held = self.modifier_down;
        if key == modifier && key != InputKey::Unbound {
            self.modifier_down = true;
        }
        let mut out = DebugPress {
            toggle_overlay: !same && key == overlay && key != InputKey::Unbound,
            ..DebugPress::default()
        };
        if !chords {
            return out;
        }
        if key == InputKey::Keyboard(KEY_ESCAPE) {
            out.pause_without_menu = held;
        } else if held {
            out.actions = bindings
                .lookup(key)
                .iter()
                .filter_map(|&i| DebugAction::from_mapping(MAPPINGS[i].name))
                .collect();
        }
        if same && out.consumed() {
            self.used_as_modifier = true;
        }
        out
    }

    /// A physical release; true when the overlay toggles.
    pub fn release(&mut self, bindings: &KeyBindings, key: InputKey) -> bool {
        let modifier = bindings.input(index("key.debug.modifier"));
        let overlay = bindings.input(index("key.debug.overlay"));
        if key != modifier || key == InputKey::Unbound {
            return false;
        }
        self.modifier_down = false;
        if modifier != overlay {
            return false;
        }
        !std::mem::take(&mut self.used_as_modifier)
    }

    /// A screen took F3's release (the game mode switcher), so the chord is over.
    pub fn release_consumed(&mut self, bindings: &KeyBindings, key: InputKey) {
        if key == bindings.input(index("key.debug.modifier")) {
            self.modifier_down = false;
            self.used_as_modifier = false;
        }
    }

    /// Focus moved away with F3 held: forget it like a release that toggles nothing.
    pub fn reset(&mut self) {
        self.modifier_down = false;
        self.used_as_modifier = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F3: InputKey = InputKey::Keyboard(60);
    const H: InputKey = InputKey::Keyboard(11);
    const W: InputKey = InputKey::Keyboard(26);
    const C: InputKey = InputKey::Keyboard(6);
    const ESC: InputKey = InputKey::Keyboard(KEY_ESCAPE);

    #[test]
    fn overlay_toggles_on_release_not_press() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        assert_eq!(d.press(&b, F3, true), DebugPress::default());
        assert!(d.modifier_down());
        assert!(d.release(&b, F3));
        assert!(!d.modifier_down());
    }

    #[test]
    fn a_chord_keeps_the_overlay_and_eats_the_key() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        d.press(&b, F3, true);
        let p = d.press(&b, H, true);
        assert_eq!(p.actions, vec![DebugAction::AdvancedTooltips]);
        assert!(p.consumed());
        assert!(!d.release(&b, F3));
        d.press(&b, F3, true);
        assert!(d.release(&b, F3), "the next plain F3 toggles again");
    }

    #[test]
    fn unbound_keys_with_f3_held_are_ordinary() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        d.press(&b, F3, true);
        let p = d.press(&b, W, true);
        assert!(!p.consumed());
        assert!(d.release(&b, F3));
    }

    #[test]
    fn keys_without_f3_are_not_chords() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        assert!(!d.press(&b, H, true).consumed());
        assert!(!d.press(&b, ESC, true).pause_without_menu);
    }

    #[test]
    fn shared_keys_fire_every_debug_mapping_and_esc_pauses_without_menu() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        d.press(&b, F3, true);
        assert_eq!(
            d.press(&b, C, true).actions,
            vec![DebugAction::CopyLocation]
        );
        assert!(d.press(&b, ESC, true).pause_without_menu);
        assert!(!d.release(&b, F3));
    }

    #[test]
    fn screens_track_f3_without_chords() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        d.press(&b, F3, false);
        assert!(!d.press(&b, H, false).consumed());
        assert!(d.release(&b, F3));
    }

    #[test]
    fn a_separate_overlay_key_toggles_on_press() {
        let mut b = KeyBindings::default();
        b.set(index("key.debug.overlay"), InputKey::Keyboard(64));
        let mut d = DebugKeys::default();
        assert!(d.press(&b, InputKey::Keyboard(64), true).toggle_overlay);
        d.press(&b, F3, true);
        assert!(!d.release(&b, F3));
        d.press(&b, F3, true);
        assert!(d.press(&b, H, true).consumed());
    }

    #[test]
    fn a_screen_taking_the_release_ends_the_chord() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        d.press(&b, F3, true);
        d.press(&b, InputKey::Keyboard(61), true);
        d.release_consumed(&b, F3);
        assert!(!d.modifier_down());
        d.press(&b, F3, true);
        assert!(d.release(&b, F3));
    }
}
