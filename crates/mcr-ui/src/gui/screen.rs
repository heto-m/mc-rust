use super::env::{Env, RenderCx};
use super::input::{DoubleClickTracker, UiInput};

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
    RenameItem(String),
    SetBeacon {
        primary: Option<String>,
        secondary: Option<String>,
    },
    Respawn,
    Disconnect,
    Connect(String),
    CancelConnect,
    OpenOptions,
    OpenAdvancements,
    OpenStats,
    OpenUrl(String),
    CopyToClipboard(String),
    ClickSound,
    Quit,
    CreativeSetCarried(mcr_protocol::types::item::ItemStack),
    CreativeClick(ContainerClick),
    CreativeSetSlot {
        slot: usize,
        stack: mcr_protocol::types::item::ItemStack,
    },
    CreativeDrop(mcr_protocol::types::item::ItemStack),
    CreativeClearInventory,
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
            Self::SetBeacon { primary, secondary } => f
                .debug_struct("SetBeacon")
                .field("primary", primary)
                .field("secondary", secondary)
                .finish(),
            Self::Respawn => f.write_str("Respawn"),
            Self::Disconnect => f.write_str("Disconnect"),
            Self::Connect(a) => f.debug_tuple("Connect").field(a).finish(),
            Self::CancelConnect => f.write_str("CancelConnect"),
            Self::OpenOptions => f.write_str("OpenOptions"),
            Self::OpenAdvancements => f.write_str("OpenAdvancements"),
            Self::OpenStats => f.write_str("OpenStats"),
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

pub trait Screen {
    fn init(&mut self, width: i32, height: i32);

    fn render(&mut self, cx: &mut RenderCx<'_, '_>);

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool;

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {}

    fn pauses_game(&self) -> bool {
        false
    }

    /// Rectangle (x, y, w, h) in GUI pixels where the app may draw a 3D preview such as the player model.
    fn entity_rect(&self) -> Option<[i32; 4]> {
        None
    }

    /// Center in GUI pixels of container slot `index`, for scripted clicks.
    fn slot_center(&self, _index: usize) -> Option<(i32, i32)> {
        None
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
