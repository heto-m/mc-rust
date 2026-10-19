//! Shared `options.txt` state and the client information built from it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use mcr_settings::{Options, OptionsFile};

#[derive(Debug)]
pub struct Settings {
    pub file: OptionsFile,
    path: PathBuf,
    /// Bumped on every edit so the running session knows to re-apply.
    pub version: u64,
    /// Java runs a dynamic light mod, so Video Settings offers mcr's switch for it; not saved.
    pub dynamic_lights_provider: bool,
    /// Where the player's resource packs come from; set once the shell knows the asset index.
    pub packs: crate::packs::PackHost,
}

pub type SharedSettings = Rc<RefCell<Settings>>;

impl Settings {
    pub fn load(game_dir: &std::path::Path) -> SharedSettings {
        let path = game_dir.join("options.txt");
        let file = OptionsFile::load(&path).unwrap_or_else(|e| {
            eprintln!("options.txt unreadable: {e}");
            OptionsFile::default()
        });
        Rc::new(RefCell::new(Self {
            file,
            path,
            version: 1,
            dynamic_lights_provider: false,
            packs: crate::packs::PackHost::default(),
        }))
    }

    pub fn edit(&mut self, f: impl FnOnce(&mut OptionsFile)) {
        f(&mut self.file);
        self.version += 1;
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = self.file.save(&self.path) {
            eprintln!("options.txt not saved: {e}");
        }
    }
}

/// `Options#buildPlayerInformation`: what the server learns about this client's options.
pub fn client_information(
    options: &Options,
    view_distance: i32,
) -> mcr_protocol::common::ClientInformation {
    use mcr_protocol::common::{ChatVisibility as Chat, MainHand, ParticleStatus as Particles};
    use mcr_settings::value::{ChatVisibility, HumanoidArm, ParticleStatus};
    let model_customisation = options
        .model_parts
        .iter()
        .enumerate()
        .filter(|(_, on)| **on)
        .fold(0u8, |mask, (bit, _)| mask | 1 << bit);
    mcr_protocol::common::ClientInformation {
        language: options.language.chars().take(16).collect(),
        view_distance: view_distance.clamp(2, 32) as i8,
        chat_visibility: match options.chat_visibility {
            ChatVisibility::Full => Chat::Full,
            ChatVisibility::System => Chat::System,
            ChatVisibility::Hidden => Chat::Hidden,
        },
        chat_colors: options.chat_colors,
        model_customisation,
        main_hand: match options.main_hand {
            HumanoidArm::Left => MainHand::Left,
            HumanoidArm::Right => MainHand::Right,
        },
        text_filtering_enabled: false,
        allows_listing: options.allow_server_listing,
        particle_status: match options.particles {
            ParticleStatus::All => Particles::All,
            ParticleStatus::Decreased => Particles::Decreased,
            ParticleStatus::Minimal => Particles::Minimal,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_bump_the_version_and_persist() {
        let dir = std::env::temp_dir().join(format!("mcr-settings-{}", std::process::id()));
        let settings = Settings::load(&dir);
        let before = settings.borrow().version;
        settings
            .borrow_mut()
            .edit(|f| f.options.render_distance = 5);
        assert!(settings.borrow().version > before);
        let again = Settings::load(&dir);
        assert_eq!(again.borrow().file.options.render_distance, 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn client_information_follows_the_options() {
        use mcr_protocol::common::{ChatVisibility, MainHand, ParticleStatus};
        let mut options = Options::default();
        let info = client_information(&options, 12);
        assert_eq!(info.model_customisation, 0x7f);
        assert_eq!(info.main_hand, MainHand::Right);
        assert!(info.chat_colors && info.allows_listing);
        assert_eq!(info.view_distance, 12);

        options.model_parts[0] = false;
        options.model_parts[6] = false;
        options.main_hand = mcr_settings::value::HumanoidArm::Left;
        options.chat_colors = false;
        options.allow_server_listing = false;
        options.chat_visibility = mcr_settings::value::ChatVisibility::System;
        options.particles = mcr_settings::value::ParticleStatus::Minimal;
        options.language = "de_de".into();
        let info = client_information(&options, 64);
        assert_eq!(info.model_customisation, 0b011_1110);
        assert_eq!(info.main_hand, MainHand::Left);
        assert!(!info.chat_colors && !info.allows_listing);
        assert_eq!(info.chat_visibility, ChatVisibility::System);
        assert_eq!(info.particle_status, ParticleStatus::Minimal);
        assert_eq!(info.language, "de_de");
        assert_eq!(info.view_distance, 32);

        options.language = "x".repeat(100);
        assert_eq!(client_information(&options, 2).language.len(), 16);
    }
}
