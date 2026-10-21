//! Command access and a forced game mode for LAN guests, applied through the server console.

use std::collections::HashSet;

use crate::singleplayer::{GAME_MODES, plain_player};

#[derive(Debug)]
pub struct GuestPolicy {
    pub default_mode: u8,
    pub commands: bool,
    pub force_mode: bool,
    present: HashSet<String>,
    opped: HashSet<String>,
}

impl Default for GuestPolicy {
    fn default() -> Self {
        Self {
            default_mode: 0,
            commands: false,
            force_mode: true,
            present: HashSet::new(),
            opped: HashSet::new(),
        }
    }
}

impl GuestPolicy {
    fn mode_line(&self, guest: &str) -> String {
        format!(
            "gamemode {} {guest}",
            GAME_MODES[usize::from(self.default_mode.min(3))]
        )
    }

    fn join_lines(&mut self, guest: &str) -> Vec<String> {
        if self.commands {
            self.opped.insert(guest.to_owned());
            vec![format!("op {guest}")]
        } else if self.force_mode {
            vec![self.mode_line(guest)]
        } else {
            Vec::new()
        }
    }

    /// The console lines for guests who joined or left since the last call.
    pub fn sync(&mut self, guests: &[String]) -> Vec<String> {
        let now: HashSet<&String> = guests.iter().filter(|g| plain_player(g)).collect();
        let mut lines = Vec::new();
        let left: Vec<String> = self
            .present
            .iter()
            .filter(|g| !now.contains(g))
            .cloned()
            .collect();
        for guest in left {
            self.present.remove(&guest);
            if self.opped.remove(&guest) {
                lines.push(format!("deop {guest}"));
            }
        }
        for guest in now {
            if self.present.insert(guest.clone()) {
                lines.extend(self.join_lines(guest));
            }
        }
        lines
    }

    /// Applies new settings to the guests already here.
    pub fn set(
        &mut self,
        commands: Option<bool>,
        force_mode: Option<bool>,
        default_mode: Option<u8>,
    ) -> Vec<String> {
        let before = (self.commands, self.force_mode, self.default_mode);
        self.commands = commands.unwrap_or(self.commands);
        self.force_mode = force_mode.unwrap_or(self.force_mode);
        self.default_mode = default_mode.map_or(self.default_mode, |m| m.min(3));
        let mut guests: Vec<String> = self.present.iter().cloned().collect();
        guests.sort();
        let mut lines = Vec::new();
        if self.commands != before.0 {
            for guest in &guests {
                if self.commands {
                    self.opped.insert(guest.clone());
                    lines.push(format!("op {guest}"));
                } else if self.opped.remove(guest) {
                    lines.push(format!("deop {guest}"));
                }
            }
        }
        let forcing = self.force_mode && !self.commands;
        let force_changed = (self.force_mode, self.default_mode) != (before.1, before.2);
        if forcing && (force_changed || self.commands != before.0) {
            lines.extend(guests.iter().map(|g| self.mode_line(g)));
        }
        lines
    }

    /// The world closed to LAN: guests lose the access given to them.
    pub fn release(&mut self) -> Vec<String> {
        let mut opped: Vec<String> = self.opped.drain().collect();
        opped.sort();
        self.present.clear();
        opped.into_iter().map(|g| format!("deop {g}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn a_joining_guest_is_forced_into_the_default_mode() {
        let mut p = GuestPolicy {
            default_mode: 2,
            ..GuestPolicy::default()
        };
        assert_eq!(p.sync(&names(&["alex"])), ["gamemode adventure alex"]);
        assert!(p.sync(&names(&["alex"])).is_empty());
    }

    #[test]
    fn command_access_ops_on_join_and_deops_on_leave() {
        let mut p = GuestPolicy {
            commands: true,
            ..GuestPolicy::default()
        };
        assert_eq!(p.sync(&names(&["alex"])), ["op alex"]);
        assert_eq!(p.sync(&[]), ["deop alex"]);
    }

    #[test]
    fn switching_access_changes_the_guests_already_here() {
        let mut p = GuestPolicy::default();
        p.sync(&names(&["alex", "sam"]));
        assert_eq!(
            p.set(Some(true), None, None),
            ["op alex", "op sam"],
            "no mode is forced while guests may use commands"
        );
        assert_eq!(
            p.set(Some(false), None, None),
            [
                "deop alex",
                "deop sam",
                "gamemode survival alex",
                "gamemode survival sam"
            ]
        );
    }

    #[test]
    fn a_new_default_mode_reaches_forced_guests_only() {
        let mut p = GuestPolicy::default();
        p.sync(&names(&["alex"]));
        assert_eq!(p.set(None, None, Some(1)), ["gamemode creative alex"]);
        p.set(None, Some(false), None);
        assert!(p.set(None, None, Some(2)).is_empty());
    }

    #[test]
    fn release_deops_everyone_given_access() {
        let mut p = GuestPolicy {
            commands: true,
            ..GuestPolicy::default()
        };
        p.sync(&names(&["alex"]));
        assert_eq!(p.release(), ["deop alex"]);
        assert!(p.release().is_empty());
    }

    #[test]
    fn names_the_console_cannot_take_are_ignored() {
        let mut p = GuestPolicy::default();
        assert!(p.sync(&names(&["a b", "@a"])).is_empty());
    }
}
