use std::sync::OnceLock;

use mcr_protocol::{Direction, PacketIds};

use super::phase::NetPhase;

/// Serverbound packets Java may send through Rust's connection; everything else is Rust's.
pub const FORWARD: &[(NetPhase, &str)] = &[
    (NetPhase::Login, "custom_query_answer"),
    (NetPhase::Configuration, "custom_payload"),
    (NetPhase::Configuration, "custom_click_action"),
    (NetPhase::Play, "custom_payload"),
    (NetPhase::Play, "container_click"),
    (NetPhase::Play, "container_button_click"),
    (NetPhase::Play, "container_close"),
    (NetPhase::Play, "container_slot_state_changed"),
    (NetPhase::Play, "rename_item"),
    (NetPhase::Play, "select_trade"),
    (NetPhase::Play, "set_beacon"),
    (NetPhase::Play, "edit_book"),
    (NetPhase::Play, "sign_update"),
    (NetPhase::Play, "place_recipe"),
    (NetPhase::Play, "recipe_book_change_settings"),
    (NetPhase::Play, "recipe_book_seen_recipe"),
    (NetPhase::Play, "set_creative_mode_slot"),
    (NetPhase::Play, "command_suggestion"),
    (NetPhase::Play, "client_command"),
    (NetPhase::Play, "custom_click_action"),
    (NetPhase::Play, "seen_advancements"),
    (NetPhase::Play, "bundle_item_selected"),
    (NetPhase::Play, "change_game_mode"),
    (NetPhase::Play, "change_difficulty"),
    (NetPhase::Play, "lock_difficulty"),
    (NetPhase::Play, "set_command_block"),
    (NetPhase::Play, "set_command_minecart"),
    (NetPhase::Play, "set_structure_block"),
    (NetPhase::Play, "set_jigsaw_block"),
    (NetPhase::Play, "jigsaw_generate"),
    (NetPhase::Play, "set_test_block"),
    (NetPhase::Play, "test_instance_block_action"),
];

/// Rust sends its own brand; a mod-mode brand from Java would contradict it.
const BRAND: &[u8] = b"minecraft:brand";
const MAX_ID_LEN: usize = 32767;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Forward,
    /// Java's tag is not Rust's current outbound phase.
    WrongPhase,
    /// Rust owns this packet (or it is not on the list).
    Owned,
    Brand,
    Malformed,
}

#[derive(Debug)]
pub struct ForwardTable {
    ids: [Vec<i32>; 3],
    custom_payload: [Option<i32>; 3],
}

fn slot(phase: NetPhase) -> usize {
    match phase {
        NetPhase::Login => 0,
        NetPhase::Configuration => 1,
        NetPhase::Play => 2,
    }
}

impl ForwardTable {
    pub fn resolve(ids: &PacketIds) -> Self {
        let mut table = Self {
            ids: Default::default(),
            custom_payload: [None; 3],
        };
        for &(phase, name) in FORWARD {
            let id = ids
                .id_by_name(phase.state(), Direction::Serverbound, name)
                .unwrap_or_else(|| panic!("forward list names unknown packet {name}"));
            table.ids[slot(phase)].push(id);
            if name == "custom_payload" {
                table.custom_payload[slot(phase)] = Some(id);
            }
        }
        table
    }

    pub fn v26_3() -> &'static Self {
        static TABLE: OnceLock<ForwardTable> = OnceLock::new();
        TABLE.get_or_init(|| Self::resolve(PacketIds::v26_3()))
    }

    /// `rust` is Rust's outbound phase, `tagged` the phase Java encoded the frame in.
    pub fn verdict(&self, rust: NetPhase, tagged: NetPhase, id: i32, payload: &[u8]) -> Verdict {
        if rust != tagged {
            return Verdict::WrongPhase;
        }
        let s = slot(rust);
        if !self.ids[s].contains(&id) {
            return Verdict::Owned;
        }
        if self.custom_payload[s] == Some(id) {
            let mut rest = payload;
            let Ok(len) = mcr_buf::read_varint(&mut rest) else {
                return Verdict::Malformed;
            };
            let Some(channel) = usize::try_from(len)
                .ok()
                .filter(|&l| l <= MAX_ID_LEN)
                .and_then(|l| rest.get(..l))
            else {
                return Verdict::Malformed;
            };
            if channel == BRAND {
                return Verdict::Brand;
            }
        }
        Verdict::Forward
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sb(phase: NetPhase, name: &str) -> i32 {
        PacketIds::v26_3()
            .id_by_name(phase.state(), Direction::Serverbound, name)
            .unwrap_or_else(|| panic!("{name}"))
    }

    fn payload(channel: &str, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        mcr_buf::write_varint(&mut out, channel.len() as i32);
        out.extend_from_slice(channel.as_bytes());
        out.extend_from_slice(data);
        out
    }

    fn check(phase: NetPhase, forwarded: &[&str], dropped: &[&str]) {
        let t = ForwardTable::v26_3();
        for name in forwarded {
            let body = if *name == "custom_payload" {
                payload("mcr_testmod:hello", b"hi")
            } else {
                Vec::new()
            };
            assert_eq!(
                t.verdict(phase, phase, sb(phase, name), &body),
                Verdict::Forward,
                "{phase:?} {name}"
            );
        }
        for name in dropped {
            assert_eq!(
                t.verdict(phase, phase, sb(phase, name), &[]),
                Verdict::Owned,
                "{phase:?} {name}"
            );
        }
    }

    #[test]
    fn login_row() {
        check(
            NetPhase::Login,
            &["custom_query_answer"],
            &["hello", "key", "login_acknowledged", "cookie_response"],
        );
    }

    #[test]
    fn configuration_row() {
        check(
            NetPhase::Configuration,
            &["custom_payload", "custom_click_action"],
            &[
                "client_information",
                "select_known_packs",
                "finish_configuration",
                "keep_alive",
                "pong",
                "resource_pack",
                "accept_code_of_conduct",
                "cookie_response",
            ],
        );
    }

    #[test]
    fn play_row() {
        check(
            NetPhase::Play,
            &[
                "custom_payload",
                "container_click",
                "container_button_click",
                "container_close",
                "container_slot_state_changed",
                "rename_item",
                "select_trade",
                "set_beacon",
                "edit_book",
                "sign_update",
                "place_recipe",
                "recipe_book_change_settings",
                "recipe_book_seen_recipe",
                "set_creative_mode_slot",
                "command_suggestion",
                "client_command",
                "custom_click_action",
                "seen_advancements",
                "bundle_item_selected",
                "change_game_mode",
                "change_difficulty",
                "lock_difficulty",
                "set_command_block",
                "set_command_minecart",
                "set_structure_block",
                "set_jigsaw_block",
                "jigsaw_generate",
                "set_test_block",
                "test_instance_block_action",
            ],
            &[
                "keep_alive",
                "pong",
                "move_player_pos",
                "move_player_pos_rot",
                "move_player_rot",
                "move_player_status_only",
                "player_input",
                "accept_teleportation",
                "chunk_batch_received",
                "client_tick_end",
                "chat",
                "chat_command",
                "chat_command_signed",
                "chat_ack",
                "chat_session_update",
                "player_loaded",
                "set_carried_item",
                "use_item",
                "use_item_on",
                "player_action",
                "interact",
                "attack",
                "punch",
                "configuration_acknowledged",
                "client_information",
            ],
        );
    }

    #[test]
    fn brand_wrong_phase_and_malformed_payloads_drop() {
        let t = ForwardTable::v26_3();
        let id = sb(NetPhase::Play, "custom_payload");
        let brand = payload("minecraft:brand", b"\x06fabric");
        assert_eq!(
            t.verdict(NetPhase::Play, NetPhase::Play, id, &brand),
            Verdict::Brand
        );
        assert_eq!(
            t.verdict(NetPhase::Play, NetPhase::Play, id, &[0xff, 0xff]),
            Verdict::Malformed
        );
        assert_eq!(
            t.verdict(NetPhase::Play, NetPhase::Play, id, &[40, b'a']),
            Verdict::Malformed
        );
        let cfg_id = sb(NetPhase::Configuration, "custom_payload");
        let mod_payload = payload("fabric:test", &[]);
        assert_eq!(
            t.verdict(
                NetPhase::Play,
                NetPhase::Configuration,
                cfg_id,
                &mod_payload
            ),
            Verdict::WrongPhase
        );
    }
}
