//! Vanilla's `MusicManager` state machine and situational music selection.

use crate::attributes::{BackgroundMusic, MAX_MUSIC_DELAY, Music};
use crate::events::JavaRandom;

const STARTING_DELAY: i64 = 100;
const MINUTE_TICKS: u32 = 1200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MusicFrequency {
    #[default]
    Default,
    Frequent,
    Constant,
}

impl MusicFrequency {
    fn max_ticks(self) -> u32 {
        match self {
            Self::Default => 20 * MINUTE_TICKS,
            Self::Frequent => 10 * MINUTE_TICKS,
            Self::Constant => 0,
        }
    }

    fn next_delay(self, music: Option<&Music>, rng: &mut JavaRandom) -> i64 {
        let Some(music) = music else {
            return i64::from(self.max_ticks());
        };
        if self == Self::Constant {
            return 100;
        }
        let min = music.min_delay.min(self.max_ticks());
        let max = music.max_delay.min(self.max_ticks());
        i64::from(rng.next_int_between(min as i32, max as i32))
    }
}

/// What is going on in the client, reduced to what music selection looks at.
#[derive(Debug, Clone, Copy)]
pub struct Situation<'a> {
    pub screen_music: Option<&'a Music>,
    pub in_world: bool,
    pub in_end: bool,
    pub end_boss_music: bool,
    pub creative_flying: bool,
    pub underwater: bool,
    pub background: Option<&'a BackgroundMusic>,
}

pub fn select_music(s: &Situation<'_>) -> Option<Music> {
    if let Some(screen) = s.screen_music {
        return Some(screen.clone());
    }
    if !s.in_world {
        return Some(Music::menu());
    }
    if s.in_end && s.end_boss_music {
        return Some(Music::end_boss());
    }
    s.background?
        .select(s.creative_flying, s.underwater)
        .cloned()
}

#[derive(Debug, Clone, Copy)]
pub struct MusicInput<'a> {
    pub situation: Option<&'a Music>,
    pub target_volume: f32,
    pub loading: bool,
    pub track_active: bool,
    pub record_playing: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicActions {
    pub stop: bool,
    pub start: Option<Music>,
    pub gain: Option<f32>,
}

#[derive(Debug)]
pub struct MusicManager {
    rng: JavaRandom,
    frequency: MusicFrequency,
    current: Option<String>,
    gain: f32,
    next_song_delay: i64,
}

impl MusicManager {
    pub fn new(seed: i64, frequency: MusicFrequency) -> Self {
        Self {
            rng: JavaRandom::new(seed),
            frequency,
            current: None,
            gain: 1.0,
            next_song_delay: STARTING_DELAY,
        }
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn set_frequency(&mut self, frequency: MusicFrequency, situation: Option<&Music>) {
        self.frequency = frequency;
        self.next_song_delay = self.frequency.next_delay(situation, &mut self.rng);
    }

    pub fn is_playing(&self, music: &Music) -> bool {
        self.current.as_deref() == Some(music.sound.as_str())
    }

    /// Dimension change or credits end: drop the track and wait before the next one.
    pub fn stop_playing(&mut self, situation: Option<&Music>, actions: &mut MusicActions) {
        if self.current.take().is_some() {
            actions.stop = true;
        }
        self.next_song_delay = self.frequency.next_delay(situation, &mut self.rng) + STARTING_DELAY;
    }

    pub fn tick(&mut self, input: &MusicInput<'_>) -> MusicActions {
        let mut actions = MusicActions::default();
        if input.record_playing {
            if self.current.is_some() {
                self.stop_playing(input.situation, &mut actions);
            } else {
                self.next_song_delay = self.next_song_delay.max(STARTING_DELAY);
            }
            return actions;
        }
        let volume = input.target_volume.clamp(0.0, 1.0);
        if self.current.is_some() && self.gain != volume && !self.fade(volume, input, &mut actions)
        {
            return actions;
        }
        let Some(music) = input.situation else {
            self.next_song_delay = self.next_song_delay.max(STARTING_DELAY);
            return actions;
        };
        if let Some(current) = &self.current {
            if music.replace_current && music.sound != *current {
                actions.stop = true;
                self.current = None;
                self.next_song_delay = i64::from(
                    self.rng
                        .next_int_between(0, (music.min_delay / 2).min(MAX_MUSIC_DELAY) as i32),
                );
            } else if !input.track_active {
                self.current = None;
                self.next_song_delay = self
                    .next_song_delay
                    .min(self.frequency.next_delay(Some(music), &mut self.rng));
            }
        }
        self.next_song_delay = self.next_song_delay.min(i64::from(music.max_delay));
        if self.current.is_none() && !input.loading {
            self.next_song_delay -= 1;
            if self.next_song_delay <= 0 {
                self.current = Some(music.sound.clone());
                self.next_song_delay = i64::MAX;
                actions.start = Some(music.clone());
                actions.gain = Some(self.gain);
            }
        }
        actions
    }

    fn fade(&mut self, volume: f32, input: &MusicInput<'_>, actions: &mut MusicActions) -> bool {
        if self.gain < volume {
            self.gain += self.gain.clamp(5.0e-4, 0.005);
            if self.gain > volume {
                self.gain = volume;
            }
        } else {
            self.gain = 0.03 * volume + 0.97 * self.gain;
            if (self.gain - volume).abs() < 1.0e-4 || self.gain < volume {
                self.gain = volume;
            }
        }
        self.gain = self.gain.clamp(0.0, 1.0);
        if self.gain <= 1.0e-4 {
            self.stop_playing(input.situation, actions);
            return false;
        }
        actions.gain = Some(self.gain);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(situation: Option<&'a Music>, active: bool) -> MusicInput<'a> {
        MusicInput {
            situation,
            target_volume: 1.0,
            loading: false,
            track_active: active,
            record_playing: false,
        }
    }

    fn bg() -> BackgroundMusic {
        BackgroundMusic {
            default: Some(Music::game()),
            creative: Some(Music::creative()),
            underwater: None,
        }
    }

    fn world<'a>(bg: &'a BackgroundMusic) -> Situation<'a> {
        Situation {
            screen_music: None,
            in_world: true,
            in_end: false,
            end_boss_music: false,
            creative_flying: false,
            underwater: false,
            background: Some(bg),
        }
    }

    #[test]
    fn situation_table() {
        let bg = bg();
        let pick = |s: Situation<'_>| select_music(&s).map(|m| m.sound);
        assert_eq!(pick(world(&bg)).as_deref(), Some("music.game"));
        let creative = Situation {
            creative_flying: true,
            ..world(&bg)
        };
        assert_eq!(pick(creative).as_deref(), Some("music.creative"));
        let underwater = Situation {
            underwater: true,
            ..world(&bg)
        };
        assert_eq!(pick(underwater).as_deref(), Some("music.game"));
        let menu = Situation {
            in_world: false,
            ..world(&bg)
        };
        assert_eq!(pick(menu).as_deref(), Some("music.menu"));
        let boss = Situation {
            in_end: true,
            end_boss_music: true,
            ..world(&bg)
        };
        assert_eq!(pick(boss).as_deref(), Some("music.dragon"));
        let boss_elsewhere = Situation {
            end_boss_music: true,
            ..world(&bg)
        };
        assert_eq!(pick(boss_elsewhere).as_deref(), Some("music.game"));
        let credits = Music::credits();
        let screen = Situation {
            screen_music: Some(&credits),
            ..world(&bg)
        };
        assert_eq!(pick(screen).as_deref(), Some("music.credits"));
        let silent = Situation {
            background: None,
            ..world(&bg)
        };
        assert_eq!(pick(silent), None);
    }

    #[test]
    fn underwater_track_wins_when_present() {
        let mut b = bg();
        b.underwater = Some(Music {
            sound: "music.under_water".into(),
            ..Music::game()
        });
        let s = Situation {
            underwater: true,
            creative_flying: true,
            ..world(&b)
        };
        assert_eq!(select_music(&s).unwrap().sound, "music.under_water");
    }

    #[test]
    fn starts_after_the_initial_delay_and_waits_between_songs() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..99 {
            assert!(m.tick(&input(Some(&game), false)).start.is_none());
        }
        let started = m.tick(&input(Some(&game), false)).start;
        assert_eq!(started.map(|s| s.sound), Some("music.game".to_owned()));
        for _ in 0..1000 {
            assert!(m.tick(&input(Some(&game), true)).start.is_none());
        }
        // The track ends: the next delay is bounded by the situation's maximum.
        m.tick(&input(Some(&game), false));
        let mut waited = 0;
        while m.tick(&input(Some(&game), false)).start.is_none() {
            waited += 1;
            assert!(waited <= game.max_delay);
        }
    }

    #[test]
    fn constant_frequency_replays_after_five_seconds() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Constant);
        for _ in 0..100 {
            m.tick(&input(Some(&game), false));
        }
        assert!(m.current().is_some());
        m.tick(&input(Some(&game), false));
        let mut waited = 0;
        while m.tick(&input(Some(&game), false)).start.is_none() {
            waited += 1;
            assert!(waited < 200);
        }
    }

    #[test]
    fn no_situation_holds_the_countdown() {
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..500 {
            assert!(m.tick(&input(None, false)).start.is_none());
        }
        let game = Music::game();
        for _ in 0..99 {
            assert!(m.tick(&input(Some(&game), false)).start.is_none());
        }
        assert!(m.tick(&input(Some(&game), false)).start.is_some());
    }

    #[test]
    fn loading_screen_blocks_starting() {
        let menu = Music::menu();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..300 {
            let mut i = input(Some(&menu), false);
            i.loading = true;
            assert!(m.tick(&i).start.is_none());
        }
        let started = (0..100).any(|_| m.tick(&input(Some(&menu), false)).start.is_some());
        assert!(started);
    }

    #[test]
    fn replacing_music_stops_the_current_track() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..100 {
            m.tick(&input(Some(&game), false));
        }
        assert!(m.is_playing(&game));
        let menu = Music::menu();
        let actions = m.tick(&input(Some(&menu), true));
        assert!(actions.stop);
        assert!(!m.is_playing(&game));
        let mut started = None;
        for _ in 0..30 {
            started = m.tick(&input(Some(&menu), false)).start.or(started);
        }
        assert_eq!(started.map(|s| s.sound), Some("music.menu".into()));
    }

    #[test]
    fn non_replacing_music_keeps_playing_through_a_situation_change() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..100 {
            m.tick(&input(Some(&game), false));
        }
        let creative = Music::creative();
        assert!(!m.tick(&input(Some(&creative), true)).stop);
        assert!(m.is_playing(&game));
    }

    #[test]
    fn jukebox_stops_music_and_holds_the_countdown() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..100 {
            m.tick(&input(Some(&game), false));
        }
        assert!(m.current().is_some());
        let mut i = input(Some(&game), true);
        i.record_playing = true;
        assert!(m.tick(&i).stop);
        assert!(m.current().is_none());
        for _ in 0..5000 {
            assert!(m.tick(&i).start.is_none());
        }
    }

    #[test]
    fn volume_fades_down_and_stops_at_zero() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..100 {
            m.tick(&input(Some(&game), false));
        }
        let mut i = input(Some(&game), true);
        i.target_volume = 0.5;
        let a = m.tick(&i);
        assert!(a.gain.unwrap() < 1.0 && a.gain.unwrap() > 0.5);
        for _ in 0..500 {
            m.tick(&i);
        }
        assert_eq!(m.gain(), 0.5);
        i.target_volume = 0.0;
        let mut stopped = false;
        for _ in 0..1000 {
            stopped |= m.tick(&i).stop;
        }
        assert!(stopped);
        assert!(m.current().is_none());
    }

    #[test]
    fn gain_recovers_upwards() {
        let game = Music::game();
        let mut m = MusicManager::new(1, MusicFrequency::Default);
        for _ in 0..100 {
            m.tick(&input(Some(&game), false));
        }
        let mut i = input(Some(&game), true);
        i.target_volume = 0.5;
        for _ in 0..500 {
            m.tick(&i);
        }
        i.target_volume = 1.0;
        let a = m.tick(&i);
        assert!(a.gain.unwrap() > 0.5);
        for _ in 0..2000 {
            m.tick(&i);
        }
        assert_eq!(m.gain(), 1.0);
    }
}
