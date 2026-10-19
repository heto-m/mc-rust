use super::env::{Env, RenderCx};
use super::input::{DoubleClickTracker, Key, UiInput};

/// Server-opened screens nest at most a few deep; anything past this is spam.
pub const MAX_SCREENS: usize = 16;

/// Serverbound container click modes, numbered like the protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickType {
    Pickup = 0,
    QuickMove = 1,
    Swap = 2,
    Clone = 3,
    Throw = 4,
    QuickCraft = 5,
    PickupAll = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerClick {
    pub slot: i32,
    pub button: i32,
    pub mode: ClickType,
}

impl ContainerClick {
    pub const OUTSIDE: i32 = -999;

    pub fn new(slot: i32, button: i32, mode: ClickType) -> Self {
        Self { slot, button, mode }
    }
}

/// Serverbound edits made through block and minecart editor screens.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorPacket {
    CommandBlock(mcr_protocol::play::serverbound::SetCommandBlock),
    Minecart(mcr_protocol::play::serverbound::SetCommandMinecart),
    Structure(Box<mcr_protocol::play::serverbound::SetStructureBlock>),
    Jigsaw(mcr_protocol::play::serverbound::SetJigsawBlock),
    JigsawGenerate(mcr_protocol::play::serverbound::JigsawGenerate),
    TestBlock(mcr_protocol::play::serverbound::SetTestBlock),
    TestInstance(Box<mcr_protocol::play::serverbound::TestInstanceBlockAction>),
}

/// What a screen asks the app to do; navigation variants are consumed by [`ScreenStack`].
pub enum ScreenAction {
    Push(Box<dyn Screen>),
    Replace(Box<dyn Screen>),
    Pop,
    SendChat(String),
    SendCommand(String),
    ContainerClick(ContainerClick),
    ContainerButton(i32),
    CloseContainer,
    CrafterSlotState {
        slot: usize,
        enabled: bool,
    },
    SelectTrade(usize),
    /// The bundle in menu slot `slot` selected content `index`, or -1 to clear it.
    SelectBundleItem {
        slot: i32,
        index: i32,
    },
    RenameItem(String),
    SetBeacon {
        primary: Option<String>,
        secondary: Option<String>,
    },
    Respawn,
    /// The in-bed screen's Leave Bed button or Esc.
    StopSleeping,
    Disconnect,
    Connect(String),
    CancelConnect,
    /// Play the world in this `saves` folder.
    PlayWorld(String),
    CreateWorld(Box<super::world_list::CreateWorldSettings>),
    /// Open the local world's Open to LAN screen.
    OpenLan,
    StartLan(super::lan_screen::LanSettings),
    OpenOptions,
    OpenMods,
    /// Leave the title screen for the launcher.
    Launcher,
    OpenAdvancements,
    OpenStats,
    RecipeBookSettings {
        book_type: mcr_protocol::play::serverbound::RecipeBookType,
        open: bool,
        filtering: bool,
    },
    PlaceRecipe {
        recipe: i32,
        max_items: bool,
    },
    RecipeSeen(i32),
    /// An advancements tab was opened, or `None` when the screen closed.
    SeenAdvancements(Option<String>),
    RequestStats,
    OpenUrl(String),
    CopyToClipboard(String),
    ClickSound,
    Quit,
    /// The answer to the server resource pack prompt.
    ResourcePackAnswer(bool),
    /// The player's answer to the server's code of conduct; `hash` identifies the text.
    ConductAnswer {
        accept: bool,
        remember: bool,
        hash: i32,
    },
    CreativeSetCarried(mcr_protocol::types::item::ItemStack),
    CreativeClick(ContainerClick),
    CreativeSetSlot {
        slot: usize,
        stack: mcr_protocol::types::item::ItemStack,
    },
    CreativeDrop(mcr_protocol::types::item::ItemStack),
    CreativeClearInventory,
    SubmitSign([String; 4]),
    SubmitBook {
        pages: Vec<String>,
        title: Option<String>,
    },
    Editor(EditorPacket),
    /// A dialog compound or registry id from a `show_dialog` click event or a dialog button.
    ShowDialog(mcr_nbt::Value),
    CustomClick {
        id: mcr_buf::Identifier,
        payload: Option<mcr_nbt::Value>,
    },
    /// A clicked `run_command`: the app checks it like `sendUnattendedCommand` and prompts if needed.
    RunCommand(String),
    /// A clicked `open_url` that passed the URL checks; the app applies `chatLinks` and the prompt.
    OpenLink(String),
    /// Open chat with this text (`Copy to Chat Screen`).
    OpenChat(String),
    /// Show the next delayed chat message.
    ChatQueueNext,
    DialogDisconnect,
    PlaySound(&'static str),
    /// The pause menu's Player Reporting button: `SocialInteractionsScreen`.
    OpenSocialInteractions,
}

impl std::fmt::Debug for ScreenAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Push(_) => f.write_str("Push(..)"),
            Self::Replace(_) => f.write_str("Replace(..)"),
            Self::Pop => f.write_str("Pop"),
            Self::SendChat(s) => f.debug_tuple("SendChat").field(s).finish(),
            Self::SendCommand(s) => f.debug_tuple("SendCommand").field(s).finish(),
            Self::ContainerClick(c) => f.debug_tuple("ContainerClick").field(c).finish(),
            Self::ContainerButton(b) => f.debug_tuple("ContainerButton").field(b).finish(),
            Self::CloseContainer => f.write_str("CloseContainer"),
            Self::CrafterSlotState { slot, enabled } => f
                .debug_struct("CrafterSlotState")
                .field("slot", slot)
                .field("enabled", enabled)
                .finish(),
            Self::SelectTrade(i) => f.debug_tuple("SelectTrade").field(i).finish(),
            Self::RenameItem(s) => f.debug_tuple("RenameItem").field(s).finish(),
            Self::SelectBundleItem { slot, index } => f
                .debug_struct("SelectBundleItem")
                .field("slot", slot)
                .field("index", index)
                .finish(),
            Self::SetBeacon { primary, secondary } => f
                .debug_struct("SetBeacon")
                .field("primary", primary)
                .field("secondary", secondary)
                .finish(),
            Self::ResourcePackAnswer(b) => f.debug_tuple("ResourcePackAnswer").field(b).finish(),
            Self::ConductAnswer {
                accept, remember, ..
            } => f
                .debug_struct("ConductAnswer")
                .field("accept", accept)
                .field("remember", remember)
                .finish(),
            Self::Respawn => f.write_str("Respawn"),
            Self::StopSleeping => f.write_str("StopSleeping"),
            Self::Disconnect => f.write_str("Disconnect"),
            Self::Connect(a) => f.debug_tuple("Connect").field(a).finish(),
            Self::CancelConnect => f.write_str("CancelConnect"),
            Self::PlayWorld(w) => f.debug_tuple("PlayWorld").field(w).finish(),
            Self::CreateWorld(s) => f.debug_tuple("CreateWorld").field(s).finish(),
            Self::OpenLan => f.write_str("OpenLan"),
            Self::StartLan(s) => f.debug_tuple("StartLan").field(s).finish(),
            Self::OpenOptions => f.write_str("OpenOptions"),
            Self::OpenMods => f.write_str("OpenMods"),
            Self::Launcher => f.write_str("Launcher"),
            Self::OpenAdvancements => f.write_str("OpenAdvancements"),
            Self::OpenStats => f.write_str("OpenStats"),
            Self::RecipeBookSettings {
                book_type,
                open,
                filtering,
            } => f
                .debug_struct("RecipeBookSettings")
                .field("book_type", book_type)
                .field("open", open)
                .field("filtering", filtering)
                .finish(),
            Self::PlaceRecipe { recipe, max_items } => f
                .debug_struct("PlaceRecipe")
                .field("recipe", recipe)
                .field("max_items", max_items)
                .finish(),
            Self::RecipeSeen(r) => f.debug_tuple("RecipeSeen").field(r).finish(),
            Self::SeenAdvancements(t) => f.debug_tuple("SeenAdvancements").field(t).finish(),
            Self::RequestStats => f.write_str("RequestStats"),
            Self::OpenUrl(u) => f.debug_tuple("OpenUrl").field(u).finish(),
            Self::CopyToClipboard(t) => f.debug_tuple("CopyToClipboard").field(t).finish(),
            Self::ClickSound => f.write_str("ClickSound"),
            Self::Quit => f.write_str("Quit"),
            Self::CreativeSetCarried(s) => f.debug_tuple("CreativeSetCarried").field(s).finish(),
            Self::CreativeClick(c) => f.debug_tuple("CreativeClick").field(c).finish(),
            Self::CreativeSetSlot { slot, stack } => f
                .debug_struct("CreativeSetSlot")
                .field("slot", slot)
                .field("stack", stack)
                .finish(),
            Self::CreativeDrop(s) => f.debug_tuple("CreativeDrop").field(s).finish(),
            Self::CreativeClearInventory => f.write_str("CreativeClearInventory"),
            Self::SubmitSign(lines) => f.debug_tuple("SubmitSign").field(lines).finish(),
            Self::SubmitBook { pages, title } => f
                .debug_struct("SubmitBook")
                .field("pages", &pages.len())
                .field("title", title)
                .finish(),
            Self::Editor(packet) => f.debug_tuple("Editor").field(packet).finish(),
            Self::ShowDialog(_) => f.write_str("ShowDialog(..)"),
            Self::CustomClick { id, .. } => f.debug_tuple("CustomClick").field(id).finish(),
            Self::RunCommand(c) => f.debug_tuple("RunCommand").field(c).finish(),
            Self::OpenLink(u) => f.debug_tuple("OpenLink").field(u).finish(),
            Self::OpenChat(t) => f.debug_tuple("OpenChat").field(t).finish(),
            Self::ChatQueueNext => f.write_str("ChatQueueNext"),
            Self::DialogDisconnect => f.write_str("DialogDisconnect"),
            Self::PlaySound(s) => f.debug_tuple("PlaySound").field(s).finish(),
            Self::OpenSocialInteractions => f.write_str("OpenSocialInteractions"),
        }
    }
}

impl ScreenAction {
    pub fn click(&self) -> Option<ContainerClick> {
        match self {
            Self::ContainerClick(c) | Self::CreativeClick(c) => Some(*c),
            _ => None,
        }
    }
}

/// A 3D picture the app draws for the screen, besides the local player in [`Screen::entity_rect`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScreenPreview {
    /// The smithing result worn by an armour stand.
    ArmorStand { rect: [i32; 4] },
    /// The enchanting table book; `open` and `flip` are the previous and current tick's values.
    Book {
        rect: [i32; 4],
        open: [f32; 2],
        flip: [f32; 2],
    },
}

pub trait Screen {
    fn init(&mut self, width: i32, height: i32);

    fn render(&mut self, cx: &mut RenderCx<'_, '_>);

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool;

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {}

    fn pauses_game(&self) -> bool {
        false
    }

    /// The credits screen asks for `music.credits` while it is open.
    fn credits_music(&self) -> bool {
        false
    }

    /// Whether the now-playing music toast is drawn in its static form on this screen.
    fn shows_music_toast(&self) -> bool {
        false
    }

    /// `Screen#showsActiveEffects`: the HUD leaves its effect icons out under this screen.
    fn shows_active_effects(&self, _gui_width: i32) -> bool {
        false
    }

    /// Server dialogs and the screen shown while waiting for the server to answer one.
    fn is_dialog(&self) -> bool {
        false
    }

    /// Rectangle (x, y, w, h) in GUI pixels where the app may draw a 3D preview such as the player model.
    fn entity_rect(&self) -> Option<[i32; 4]> {
        None
    }

    fn screen_preview(&self) -> Option<ScreenPreview> {
        None
    }

    /// Center in GUI pixels of container slot `index`, for scripted clicks.
    fn slot_center(&self, _index: usize) -> Option<(i32, i32)> {
        None
    }

    /// `Screen#keyReleased`; true when the screen used the release.
    fn key_released(&mut self, _key: Key, _out: &mut Vec<ScreenAction>) -> bool {
        false
    }

    /// The screen is waiting for a key to bind, so global keys such as F2 go to it.
    fn captures_keys(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct ScreenStack {
    screens: Vec<Box<dyn Screen>>,
    size: (i32, i32),
    clicks: DoubleClickTracker,
}

impl std::fmt::Debug for ScreenStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScreenStack")
            .field("depth", &self.screens.len())
            .finish()
    }
}

impl ScreenStack {
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            screens: Vec::new(),
            size: (width, height),
            clicks: DoubleClickTracker::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.screens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }

    pub fn push(&mut self, mut screen: Box<dyn Screen>) {
        if self.screens.len() >= MAX_SCREENS {
            self.screens.remove(0);
        }
        screen.init(self.size.0, self.size.1);
        self.screens.push(screen);
    }

    pub fn pop(&mut self) -> Option<Box<dyn Screen>> {
        self.screens.pop()
    }

    pub fn clear(&mut self) {
        self.screens.clear();
    }

    pub fn top(&self) -> Option<&dyn Screen> {
        self.screens.last().map(|s| &**s)
    }

    pub fn pauses_game(&self) -> bool {
        self.top().is_some_and(Screen::pauses_game)
    }

    /// A pausing screen anywhere on the stack, e.g. options opened from the pause menu.
    pub fn any_pauses_game(&self) -> bool {
        self.screens.iter().any(|s| s.pauses_game())
    }

    pub fn credits_music(&self) -> bool {
        self.top().is_some_and(Screen::credits_music)
    }

    pub fn top_shows_music_toast(&self) -> bool {
        self.top().is_some_and(Screen::shows_music_toast)
    }

    pub fn top_shows_active_effects(&self) -> bool {
        self.top()
            .is_some_and(|s| s.shows_active_effects(self.size.0))
    }

    pub fn top_is_dialog(&self) -> bool {
        self.top().is_some_and(Screen::is_dialog)
    }

    pub fn top_captures_keys(&self) -> bool {
        self.top().is_some_and(Screen::captures_keys)
    }

    pub fn key_released(&mut self, key: Key, out: &mut Vec<ScreenAction>) -> bool {
        let mut local = Vec::new();
        let used = match self.screens.last_mut() {
            Some(screen) => screen.key_released(key, &mut local),
            None => false,
        };
        self.apply(local, out);
        used
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.size = (width, height);
        for screen in &mut self.screens {
            screen.init(width, height);
        }
    }

    pub fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        if let Some(screen) = self.screens.last_mut() {
            screen.render(cx);
            cx.finish();
        }
    }

    pub fn tick(&mut self, env: &mut Env<'_>, out: &mut Vec<ScreenAction>) {
        let mut local = Vec::new();
        if let Some(screen) = self.screens.last_mut() {
            screen.tick(env, &mut local);
        }
        self.apply(local, out);
    }

    pub fn input(
        &mut self,
        env: &mut Env<'_>,
        input: &UiInput,
        out: &mut Vec<ScreenAction>,
    ) -> bool {
        let mut input = *input;
        if let UiInput::MousePress { button, double, .. } = &mut input {
            *double = self.clicks.press(*button, env.now_ms);
        }
        let mut local = Vec::new();
        let used = match self.screens.last_mut() {
            Some(screen) => screen.input(env, &input, &mut local),
            None => false,
        };
        self.apply(local, out);
        used
    }

    fn apply(&mut self, actions: Vec<ScreenAction>, out: &mut Vec<ScreenAction>) {
        for action in actions {
            match action {
                ScreenAction::Push(screen) => self.push(screen),
                ScreenAction::Replace(screen) => {
                    self.screens.pop();
                    self.push(screen);
                }
                ScreenAction::Pop => {
                    self.screens.pop();
                }
                other => out.push(other),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::env::{MemoryClipboard, NoView};
    use crate::gui::input::Key;

    struct Dummy {
        inits: Vec<(i32, i32)>,
        pop_on_escape: bool,
    }

    impl Screen for Dummy {
        fn init(&mut self, w: i32, h: i32) {
            self.inits.push((w, h));
        }

        fn render(&mut self, _cx: &mut RenderCx<'_, '_>) {}

        fn input(
            &mut self,
            _env: &mut Env<'_>,
            input: &UiInput,
            out: &mut Vec<ScreenAction>,
        ) -> bool {
            if self.pop_on_escape && *input == UiInput::key(Key::Escape) {
                out.push(ScreenAction::Pop);
                out.push(ScreenAction::Respawn);
                return true;
            }
            false
        }
    }

    fn dummy(pop: bool) -> Box<dyn Screen> {
        Box::new(Dummy {
            inits: Vec::new(),
            pop_on_escape: pop,
        })
    }

    #[test]
    fn stack_depth_is_capped() {
        let mut stack = ScreenStack::new(320, 240);
        for _ in 0..MAX_SCREENS * 4 {
            stack.push(dummy(false));
        }
        assert_eq!(stack.len(), MAX_SCREENS);
    }

    #[test]
    fn pop_returns_to_previous_and_forwards_the_rest() {
        let mut stack = ScreenStack::new(320, 240);
        stack.push(dummy(false));
        stack.push(dummy(true));
        let mut clip = MemoryClipboard::default();
        let mut env = Env {
            view: &NoView,
            clipboard: &mut clip,
            now_ms: 0,
        };
        let mut out = Vec::new();
        assert!(stack.input(&mut env, &UiInput::key(Key::Escape), &mut out));
        assert_eq!(stack.len(), 1);
        assert!(matches!(out.as_slice(), [ScreenAction::Respawn]));
    }
}
