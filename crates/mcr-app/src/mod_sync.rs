//! Vanilla key bindings shared with Java in mod mode; mcr's `options.txt` is the one persisted copy.

use mcr_settings::bindings::mapping_index;
use mcr_settings::{InputKey, KeyBindings, MAPPINGS};

/// Every vanilla mapping and its key, in `options.txt` spelling, for Java to follow.
pub fn key_table(
    bindings: &KeyBindings,
) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
    MAPPINGS
        .iter()
        .enumerate()
        .map(|(i, m)| (m.name, bindings.input(i).name()))
}

/// Java's rebinds that change mcr's bindings; unknown mappings and key names are ignored.
pub fn java_key_edits(
    bindings: &KeyBindings,
    changes: &[(String, String)],
) -> Vec<(usize, InputKey)> {
    let mut edits: Vec<(usize, InputKey)> = Vec::new();
    for (name, key) in changes {
        let (Some(index), Some(input)) = (mapping_index(name), InputKey::from_name(key)) else {
            continue;
        };
        edits.retain(|(i, _)| *i != index);
        if bindings.input(index) != input {
            edits.push((index, input));
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(name: &str, key: &str) -> (String, String) {
        (name.into(), key.into())
    }

    #[test]
    fn table_names_every_vanilla_mapping() {
        let bindings = KeyBindings::default();
        let table: Vec<_> = key_table(&bindings).collect();
        assert_eq!(table.len(), MAPPINGS.len());
        assert!(table.contains(&("key.forward", "key.keyboard.w")));
        assert!(table.contains(&("key.attack", "key.mouse.left")));
        assert!(table.contains(&("key.smoothCamera", "key.keyboard.unknown")));
    }

    #[test]
    fn java_rebinds_apply_only_to_known_vanilla_mappings() {
        let bindings = KeyBindings::default();
        let jump = mapping_index("key.jump").unwrap();
        let edits = java_key_edits(
            &bindings,
            &[
                change("key.jump", "key.keyboard.j"),
                change("key.mcr_testmod.ping", "key.keyboard.l"),
                change("key.drop", "key.keyboard.nonsense"),
                change("key.forward", "key.keyboard.w"),
            ],
        );
        assert_eq!(edits, [(jump, InputKey::Keyboard(13))]);
    }

    #[test]
    fn the_last_rebind_of_a_mapping_wins() {
        let bindings = KeyBindings::default();
        let jump = mapping_index("key.jump").unwrap();
        let edits = java_key_edits(
            &bindings,
            &[
                change("key.jump", "key.keyboard.j"),
                change("key.jump", "key.keyboard.unknown"),
            ],
        );
        assert_eq!(edits, [(jump, InputKey::Unbound)]);
        let back = java_key_edits(
            &bindings,
            &[
                change("key.jump", "key.keyboard.j"),
                change("key.jump", "key.keyboard.space"),
            ],
        );
        assert!(back.is_empty());
    }
}
