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

const CRASH_HOLD_MS: u64 = 10_000;
const CRASH_REPORT_MS: u64 = 1_000;
const CRASH_EAT_AFTER_MS: u64 = 100;

/// What holding F3+C asks for on a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashEvent {
    Notice,
    Warning(u32),
    Crash,
}

/// What a key press did besides (or instead of) its ordinary bindings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DebugPress {
    pub toggle_overlay: bool,
    /// F3+Esc: pause without the menu.
    pub pause_without_menu: bool,
    pub actions: Vec<DebugAction>,
    /// The crash hold is running, so chords are swallowed without acting.
    pub eaten: bool,
}

impl DebugPress {
    /// A chord fired, so the key does not also act as a game key.
    pub fn consumed(&self) -> bool {
        self.pause_without_menu || self.eaten || !self.actions.is_empty()
    }
}

/// Where a press goes once the chords have looked at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRoute {
    PauseWithoutMenu,
    Chord,
    Pass,
}

impl DebugPress {
    /// `KeyboardHandler#keyPress` falls through to the chords when the screen did not take the key;
    /// F3+Esc only pauses where no menu is open.
    pub fn route(&self, screen_open: bool) -> KeyRoute {
        if self.pause_without_menu {
            return if screen_open {
                KeyRoute::Pass
            } else {
                KeyRoute::PauseWithoutMenu
            };
        }
        if self.consumed() {
            KeyRoute::Chord
        } else {
            KeyRoute::Pass
        }
    }
}

#[derive(Debug, Default)]
pub struct DebugKeys {
    modifier_down: bool,
    used_as_modifier: bool,
    crash_down: bool,
    crash_started: Option<u64>,
    crash_reported: u64,
    crash_count: u32,
    now: u64,
}

fn index(name: &str) -> usize {
    mcr_settings::bindings::mapping_index(name).expect("vanilla debug mapping")
}

impl DebugKeys {
    pub fn modifier_down(&self) -> bool {
        self.modifier_down
    }

    /// Every physical key event, before `press` or `release`: F3+C held starts the crash countdown.
    pub fn track_crash(&mut self, bindings: &KeyBindings, key: InputKey, pressed: bool, now: u64) {
        self.now = now;
        let crash = bindings.input(index("key.debug.crash"));
        let modifier = bindings.input(index("key.debug.modifier"));
        if key != InputKey::Unbound && key == crash {
            self.crash_down = pressed;
        }
        let modifier_down = if key != InputKey::Unbound && key == modifier {
            pressed
        } else {
            self.modifier_down
        };
        let both = self.crash_down && modifier_down && crash != InputKey::Unbound;
        match self.crash_started {
            Some(_) if !both => self.crash_started = None,
            None if both => {
                self.crash_started = Some(now);
                self.crash_reported = now;
                self.crash_count = 0;
            }
            _ => {}
        }
    }

    /// `KeyboardHandler#tick`'s countdown: a notice after a second, then a warning every second.
    pub fn crash_tick(&mut self, now: u64) -> Option<CrashEvent> {
        self.now = now;
        let start = self.crash_started?;
        let remaining = CRASH_HOLD_MS as i64 - now.saturating_sub(start) as i64;
        if remaining < 0 {
            self.crash_started = None;
            return Some(CrashEvent::Crash);
        }
        if now.saturating_sub(self.crash_reported) < CRASH_REPORT_MS {
            return None;
        }
        self.crash_reported = now;
        self.crash_count += 1;
        Some(if self.crash_count == 1 {
            CrashEvent::Notice
        } else {
            CrashEvent::Warning(((remaining + 999) / 1000) as u32)
        })
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
        let counting_down = self
            .crash_started
            .is_some_and(|s| s < self.now.saturating_sub(CRASH_EAT_AFTER_MS));
        if key == InputKey::Keyboard(KEY_ESCAPE) {
            out.pause_without_menu = held;
        } else if held && counting_down {
            out.eaten = true;
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
        self.crash_down = false;
        self.crash_started = None;
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

    fn hold(d: &mut DebugKeys, b: &KeyBindings, key: InputKey, pressed: bool, now: u64) {
        d.track_crash(b, key, pressed, now);
        if pressed {
            d.press(b, key, true);
        } else {
            d.release(b, key);
        }
    }

    #[test]
    fn holding_f3_c_counts_down_to_the_crash_after_ten_seconds() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        hold(&mut d, &b, F3, true, 0);
        hold(&mut d, &b, C, true, 10);
        assert_eq!(d.crash_tick(500), None);
        assert_eq!(d.crash_tick(1010), Some(CrashEvent::Notice));
        assert_eq!(d.crash_tick(1500), None);
        assert_eq!(d.crash_tick(2010), Some(CrashEvent::Warning(8)));
        assert_eq!(d.crash_tick(9010), Some(CrashEvent::Warning(1)));
        assert_eq!(d.crash_tick(10_010), Some(CrashEvent::Warning(0)));
        assert_eq!(d.crash_tick(10_011), Some(CrashEvent::Crash));
        assert_eq!(d.crash_tick(10_012), None, "a crash fires once");
    }

    #[test]
    fn releasing_either_key_cancels_the_crash_and_chords_wait_for_it() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        hold(&mut d, &b, F3, true, 0);
        hold(&mut d, &b, C, true, 10);
        d.crash_tick(1010);
        let eaten = d.press(&b, H, true);
        assert!(
            eaten.eaten && eaten.actions.is_empty(),
            "chords are eaten while counting down"
        );
        hold(&mut d, &b, C, false, 3000);
        assert_eq!(d.crash_tick(20_000), None);
        assert!(!d.press(&b, H, true).eaten);
        hold(&mut d, &b, C, true, 21_000);
        hold(&mut d, &b, F3, false, 22_000);
        assert_eq!(d.crash_tick(40_000), None);
    }

    #[test]
    fn the_crash_key_alone_never_counts_down() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        hold(&mut d, &b, C, true, 0);
        assert_eq!(d.crash_tick(30_000), None);
    }

    #[test]
    fn chords_reach_the_debug_keys_while_a_screen_is_open() {
        let b = KeyBindings::default();
        let mut d = DebugKeys::default();
        d.press(&b, F3, true);
        assert_eq!(d.press(&b, H, true).route(true), KeyRoute::Chord);
        assert_eq!(d.press(&b, W, true).route(true), KeyRoute::Pass);
        let esc = d.press(&b, ESC, true);
        assert_eq!(esc.route(true), KeyRoute::Pass, "Esc closes the screen");
        assert_eq!(esc.route(false), KeyRoute::PauseWithoutMenu);
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
