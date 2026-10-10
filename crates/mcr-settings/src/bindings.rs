use rustc_hash::FxHashMap;

use crate::keys::InputKey;
use crate::options::Options;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Movement,
    Inventory,
    Gameplay,
    Multiplayer,
    Misc,
    Creative,
    Spectator,
    Debug,
}

/// Which `options.txt` flag switches a key from hold to toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    None,
    Crouch,
    Sprint,
    Attack,
    Use,
}

impl Toggle {
    pub fn enabled(self, options: &Options) -> bool {
        match self {
            Self::None => false,
            Self::Crouch => options.toggle_crouch,
            Self::Sprint => options.toggle_sprint,
            Self::Attack => options.toggle_attack,
            Self::Use => options.toggle_use,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyMapping {
    pub name: &'static str,
    pub default: InputKey,
    pub category: Category,
    pub toggle: Toggle,
}

const fn km(name: &'static str, default: InputKey, category: Category) -> KeyMapping {
    KeyMapping {
        name,
        default,
        category,
        toggle: Toggle::None,
    }
}

const fn kt(
    name: &'static str,
    default: InputKey,
    category: Category,
    toggle: Toggle,
) -> KeyMapping {
    KeyMapping {
        name,
        default,
        category,
        toggle,
    }
}

const fn kb(code: i32) -> InputKey {
    InputKey::Keyboard(code)
}

const fn mouse(code: i32) -> InputKey {
    InputKey::Mouse(code)
}

const UNBOUND: InputKey = InputKey::Unbound;

/// Every vanilla 26.3 `KeyMapping`, in `Options.keyMappings` order.
pub const MAPPINGS: &[KeyMapping] = &[
    kt("key.attack", mouse(1), Category::Gameplay, Toggle::Attack),
    kt("key.use", mouse(3), Category::Gameplay, Toggle::Use),
    km("key.forward", kb(26), Category::Movement),
    km("key.left", kb(4), Category::Movement),
    km("key.back", kb(22), Category::Movement),
    km("key.right", kb(7), Category::Movement),
    km("key.jump", kb(44), Category::Movement),
    kt("key.sneak", kb(225), Category::Movement, Toggle::Crouch),
    kt("key.sprint", kb(224), Category::Movement, Toggle::Sprint),
    km("key.drop", kb(20), Category::Inventory),
    km("key.inventory", kb(8), Category::Inventory),
    km("key.chat", kb(23), Category::Multiplayer),
    km("key.playerlist", kb(43), Category::Multiplayer),
    km("key.pickItem", mouse(2), Category::Gameplay),
    km("key.command", kb(56), Category::Multiplayer),
    km("key.friends", kb(18), Category::Multiplayer),
    km("key.socialInteractions", kb(19), Category::Multiplayer),
    km("key.toggleGui", kb(58), Category::Misc),
    km("key.toggleSpectatorShaderEffects", kb(61), Category::Misc),
    km("key.screenshot", kb(59), Category::Misc),
    km("key.togglePerspective", kb(62), Category::Misc),
    km("key.smoothCamera", UNBOUND, Category::Misc),
    km("key.fullscreen", kb(68), Category::Misc),
    km("key.spectatorOutlines", UNBOUND, Category::Spectator),
    km("key.spectatorHotbar", mouse(2), Category::Spectator),
    km("key.swapOffhand", kb(9), Category::Inventory),
    km("key.saveToolbarActivator", kb(6), Category::Creative),
    km("key.loadToolbarActivator", kb(27), Category::Creative),
    km("key.advancements", kb(15), Category::Misc),
    km("key.quickActions", kb(10), Category::Misc),
    km("key.hotbar.1", kb(30), Category::Inventory),
    km("key.hotbar.2", kb(31), Category::Inventory),
    km("key.hotbar.3", kb(32), Category::Inventory),
    km("key.hotbar.4", kb(33), Category::Inventory),
    km("key.hotbar.5", kb(34), Category::Inventory),
    km("key.hotbar.6", kb(35), Category::Inventory),
    km("key.hotbar.7", kb(36), Category::Inventory),
    km("key.hotbar.8", kb(37), Category::Inventory),
    km("key.hotbar.9", kb(38), Category::Inventory),
    km("key.debug.overlay", kb(60), Category::Debug),
    km("key.debug.modifier", kb(60), Category::Debug),
    km("key.debug.crash", kb(6), Category::Debug),
    km("key.debug.reloadChunk", kb(4), Category::Debug),
    km("key.debug.showHitboxes", kb(5), Category::Debug),
    km("key.debug.clearChat", kb(7), Category::Debug),
    km("key.debug.showChunkBorders", kb(10), Category::Debug),
    km("key.debug.showAdvancedTooltips", kb(11), Category::Debug),
    km("key.debug.copyRecreateCommand", kb(12), Category::Debug),
    km("key.debug.spectate", kb(17), Category::Debug),
    km("key.debug.switchGameMode", kb(61), Category::Debug),
    km("key.debug.debugOptions", kb(63), Category::Debug),
    km("key.debug.focusPause", kb(19), Category::Debug),
    km("key.debug.dumpDynamicTextures", kb(22), Category::Debug),
    km("key.debug.reloadResourcePacks", kb(23), Category::Debug),
    km("key.debug.profiling", kb(15), Category::Debug),
    km("key.debug.copyLocation", kb(6), Category::Debug),
    km("key.debug.dumpVersion", kb(25), Category::Debug),
    km("key.debug.profilingChart", kb(30), Category::Debug),
    km("key.debug.fpsCharts", kb(31), Category::Debug),
    km("key.debug.networkCharts", kb(32), Category::Debug),
    km("key.debug.lightmapTexture", kb(33), Category::Debug),
    km("key.debug.improvedTransparency", kb(27), Category::Debug),
];

pub fn mapping_index(name: &str) -> Option<usize> {
    MAPPINGS.iter().position(|m| m.name == name)
}

fn context_group(category: Category) -> u8 {
    match category {
        Category::Debug => 1,
        Category::Spectator => 2,
        _ => 0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub input: InputKey,
    pub mappings: Vec<&'static str>,
}

/// The input bound to each mapping, with an index from input to mappings for lookup.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyBindings {
    inputs: Vec<InputKey>,
    lookup: FxHashMap<InputKey, Vec<usize>>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let inputs = MAPPINGS.iter().map(|m| m.default).collect();
        Self::from_inputs(inputs)
    }
}

impl KeyBindings {
    fn from_inputs(inputs: Vec<InputKey>) -> Self {
        let mut lookup: FxHashMap<InputKey, Vec<usize>> = FxHashMap::default();
        for (i, key) in inputs.iter().enumerate() {
            if *key != InputKey::Unbound {
                lookup.entry(*key).or_default().push(i);
            }
        }
        Self { inputs, lookup }
    }

    pub fn input(&self, index: usize) -> InputKey {
        self.inputs[index]
    }

    pub fn set(&mut self, index: usize, key: InputKey) {
        self.inputs[index] = key;
        self.lookup.clear();
        for (i, k) in self.inputs.iter().enumerate() {
            if *k != InputKey::Unbound {
                self.lookup.entry(*k).or_default().push(i);
            }
        }
    }

    /// Indices into [`MAPPINGS`] of every mapping bound to `key`.
    pub fn lookup(&self, key: InputKey) -> &[usize] {
        self.lookup.get(&key).map_or(&[], Vec::as_slice)
    }

    /// Inputs bound to two or more mappings in the same context. Debug mappings are F3 chords
    /// that share keys by design and are not checked.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut out = Vec::new();
        for (key, indices) in &self.lookup {
            if indices.len() < 2 {
                continue;
            }
            for group in [0u8, 2] {
                let names: Vec<&'static str> = indices
                    .iter()
                    .filter(|&&i| context_group(MAPPINGS[i].category) == group)
                    .map(|&i| MAPPINGS[i].name)
                    .collect();
                if names.len() >= 2 {
                    out.push(Conflict {
                        input: *key,
                        mappings: names,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.mappings.cmp(&b.mappings));
        out
    }

    pub(crate) fn from_parts(inputs: Vec<InputKey>) -> Self {
        Self::from_inputs(inputs)
    }

    pub(crate) fn inputs(&self) -> &[InputKey] {
        &self.inputs
    }
}

/// Logical pressed state of each mapping, honoring toggle flags for sneak, sprint, attack and use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldState {
    physical: Vec<bool>,
    logical: Vec<bool>,
}

impl Default for HeldState {
    fn default() -> Self {
        Self {
            physical: vec![false; MAPPINGS.len()],
            logical: vec![false; MAPPINGS.len()],
        }
    }
}

impl HeldState {
    /// Records a physical press or release of mapping `index` and returns its logical state.
    /// With toggle enabled, each new press flips the logical state and releases are ignored.
    pub fn update(&mut self, index: usize, down: bool, options: &Options) -> bool {
        let was_down = std::mem::replace(&mut self.physical[index], down);
        let toggle = MAPPINGS[index].toggle;
        let logical = &mut self.logical[index];
        if toggle.enabled(options) {
            if down && !was_down {
                *logical = !*logical;
            }
        } else {
            *logical = down;
        }
        *logical
    }

    pub fn is_active(&self, index: usize) -> bool {
        self.logical[index]
    }
}
