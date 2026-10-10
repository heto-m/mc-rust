use std::sync::OnceLock;

use mcr_protocol::types::item::ItemStack;

const VANILLA_FEATURE: &str = "minecraft:vanilla";
const MAX_STACK_BYTES: usize = 1 << 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    Category,
    Hotbar,
    Search,
    Inventory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabRow {
    Top,
    Bottom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabEntry {
    pub stack: ItemStack,
    features: Vec<String>,
}

impl TabEntry {
    pub fn enabled(&self, enabled: &[String]) -> bool {
        self.features
            .iter()
            .all(|f| f == VANILLA_FEATURE || enabled.contains(f))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreativeTab {
    pub key: String,
    pub title_key: String,
    pub row: TabRow,
    pub column: u8,
    pub aligned_right: bool,
    pub kind: TabKind,
    pub show_title: bool,
    pub can_scroll: bool,
    pub icon: ItemStack,
    pub parent: Vec<TabEntry>,
    pub search: Vec<TabEntry>,
}

impl CreativeTab {
    pub fn is_operator(&self) -> bool {
        self.key == "op_blocks"
    }

    pub fn contains(&self, stack: &ItemStack) -> bool {
        self.parent
            .iter()
            .any(|e| e.stack.item == stack.item && e.stack.components == stack.components)
    }
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > MAX_STACK_BYTES * 2 {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let digit = |b: u8| char::from(b).to_digit(16);
            Some((digit(pair[0])? * 16 + digit(pair[1])?) as u8)
        })
        .collect()
}

fn stack_of(hex: &str) -> Option<ItemStack> {
    mcr_buf::from_bytes_exact::<ItemStack>(&unhex(hex)?).ok()
}

fn entry_of(rest: &str) -> Option<TabEntry> {
    let (features, hex) = rest.split_once(' ')?;
    let features = match features {
        "-" => Vec::new(),
        list => list.split(',').map(str::to_owned).collect(),
    };
    Some(TabEntry {
        stack: stack_of(hex)?,
        features,
    })
}

fn header_of(rest: &str) -> Option<CreativeTab> {
    let mut it = rest.split(' ');
    let key = it.next()?.to_owned();
    let row = match it.next()? {
        "TOP" => TabRow::Top,
        "BOTTOM" => TabRow::Bottom,
        _ => return None,
    };
    let column = it.next()?.parse().ok()?;
    let aligned_right = it.next()? == "1";
    let kind = match it.next()? {
        "CATEGORY" => TabKind::Category,
        "HOTBAR" => TabKind::Hotbar,
        "SEARCH" => TabKind::Search,
        "INVENTORY" => TabKind::Inventory,
        _ => return None,
    };
    let show_title = it.next()? == "1";
    let can_scroll = it.next()? == "1";
    let title_key = it.next()?.to_owned();
    let icon = stack_of(it.next()?)?;
    Some(CreativeTab {
        key,
        title_key,
        row,
        column,
        aligned_right,
        kind,
        show_title,
        can_scroll,
        icon,
        parent: Vec::new(),
        search: Vec::new(),
    })
}

pub fn parse_tabs(text: &str) -> Vec<CreativeTab> {
    let mut tabs: Vec<CreativeTab> = Vec::new();
    for line in text.lines() {
        let Some((tag, rest)) = line.split_once(' ') else {
            continue;
        };
        match tag {
            "tab" => match header_of(rest) {
                Some(tab) => tabs.push(tab),
                None => tracing::warn!("bad creative tab header"),
            },
            "p" | "s" => {
                let (Some(tab), Some(entry)) = (tabs.last_mut(), entry_of(rest)) else {
                    continue;
                };
                if tag == "p" {
                    tab.parent.push(entry);
                } else {
                    tab.search.push(entry);
                }
            }
            _ => {}
        }
    }
    tabs
}

/// Vanilla's creative tabs in registry order.
pub fn creative_tabs() -> &'static [CreativeTab] {
    static TABS: OnceLock<Vec<CreativeTab>> = OnceLock::new();
    TABS.get_or_init(|| parse_tabs(mcr_data::creative_tabs_26_3()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_tabs_parse_with_vanilla_layout() {
        let tabs = creative_tabs();
        let keys: Vec<&str> = tabs.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "building_blocks",
                "colored_blocks",
                "natural_blocks",
                "functional_blocks",
                "redstone_blocks",
                "hotbar",
                "search",
                "tools_and_utilities",
                "combat",
                "food_and_drinks",
                "ingredients",
                "spawn_eggs",
                "op_blocks",
                "inventory",
            ]
        );
        let building = &tabs[0];
        assert_eq!(building.row, TabRow::Top);
        assert!(building.parent.len() > 400);
        assert!(!tabs[1].parent.is_empty() && !tabs[12].parent.is_empty());
        assert!(tabs[10].search.len() > tabs[10].parent.len());
        assert!(tabs.iter().all(|t| !t.icon.is_empty()));
    }

    #[test]
    fn first_building_block_is_oak_log() {
        let first = &creative_tabs()[0].parent[0].stack;
        let version = mcr_data::v26_3();
        assert_eq!(
            version.item_name(mcr_data::ItemId::try_from(first.item).unwrap()),
            Some("minecraft:oak_log")
        );
    }

    #[test]
    fn feature_gating_requires_every_flag() {
        let entry = TabEntry {
            stack: ItemStack::new(1, 1),
            features: vec![
                "minecraft:vanilla".into(),
                "minecraft:trade_rebalance".into(),
            ],
        };
        assert!(!entry.enabled(&["minecraft:vanilla".into()]));
        assert!(entry.enabled(&["minecraft:trade_rebalance".into()]));
    }

    #[test]
    fn hex_decoding_rejects_garbage() {
        assert_eq!(unhex("0aff"), Some(vec![10, 255]));
        assert_eq!(unhex("0"), None);
        assert_eq!(unhex("zz"), None);
    }
}
