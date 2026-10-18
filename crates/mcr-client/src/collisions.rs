//! What the local player's physics needs from other entities each tick: the boxes it collides with
//! (`getEntityCollisions`), the world border, and the entities that push it (`pushEntities`).

use mcr_physics::{Aabb, BorderBounds};
use mcr_protocol::play::clientbound::ui::CollisionRule;
use mcr_protocol::types::game::GameType;

use crate::entity::Entity;
use crate::state::ClientState;

const MAX_COLLIDERS: usize = 64;
const MAX_PUSHERS: usize = 64;
const SCAN_MARGIN: f64 = 4.0;
/// A boat pushes what touches its box grown by this sideways and shrunk by 0.01 vertically.
const BOAT_PUSH_GROW: f64 = 0.20000000298023224;

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn is_boat(name: &str) -> bool {
    name.ends_with("_boat") || name.ends_with("_raft")
}

/// `EntitySelector#pushableBy` between the pusher's and the local player's team rules.
fn teams_allow_push(
    own: Option<(&str, CollisionRule)>,
    theirs: Option<(&str, CollisionRule)>,
) -> bool {
    let own_rule = own.map_or(CollisionRule::Always, |t| t.1);
    let their_rule = theirs.map_or(CollisionRule::Always, |t| t.1);
    if own_rule == CollisionRule::Never || their_rule == CollisionRule::Never {
        return false;
    }
    let same = matches!((own, theirs), (Some(a), Some(b)) if a.0 == b.0);
    if (own_rule == CollisionRule::PushOwnTeam || their_rule == CollisionRule::PushOwnTeam) && same
    {
        return false;
    }
    own_rule != CollisionRule::PushOtherTeams && their_rule != CollisionRule::PushOtherTeams || same
}

impl ClientState {
    fn team_rule(&self, member: &str) -> Option<(&str, CollisionRule)> {
        let team = self.scoreboard.team_of(member)?;
        let t = self.scoreboard.teams.get(team)?;
        Some((team, t.parameters.collision_rule))
    }

    fn entity_team_rule(&self, e: &Entity) -> Option<(&str, CollisionRule)> {
        if self.scoreboard.teams.is_empty() {
            return None;
        }
        match self.players.get(e.uuid) {
            Some(p) => self.team_rule(&p.name),
            None => self.team_rule(&e.uuid.to_string()),
        }
    }

    /// Whether `e`'s own `pushEntities` shoves the local player this tick.
    fn pushes_local_player(&self, e: &Entity, name: &str) -> bool {
        if matches!(name, "bat" | "armor_stand" | "parrot") || !self.entity_alive(e) {
            return false;
        }
        if self
            .players
            .get(e.uuid)
            .is_some_and(|p| p.game_mode == GameType::Spectator)
        {
            return false;
        }
        self.teams_allow(e)
    }

    fn teams_allow(&self, e: &Entity) -> bool {
        self.scoreboard.teams.is_empty()
            || teams_allow_push(self.entity_team_rule(e), self.team_rule(&self.name))
    }

    /// Fills the player's entity colliders and border, then applies this tick's pushes.
    pub(crate) fn refresh_collision_env(&mut self, pushers: &mut Vec<[f64; 3]>) {
        let bb = self.player.physics.bounding_box();
        let speed = self
            .player
            .physics
            .vel
            .iter()
            .map(|v| v.abs())
            .fold(0.0, f64::max);
        let reach = if speed.is_finite() {
            speed.min(64.0)
        } else {
            0.0
        } + SCAN_MARGIN;
        let scan = bb.inflate(reach);
        let mut colliders = std::mem::take(&mut self.player.physics.entity_colliders);
        colliders.clear();
        pushers.clear();
        let own = self.player.entity_id;
        let pushable = !self.player.dead
            && self.player.game_mode != GameType::Spectator
            && self.player.vehicle.is_none();
        for e in self.entities.iter() {
            if e.id == own || e.vehicle == Some(own) {
                continue;
            }
            let p = e.pos;
            if p[0] < scan.min[0] - 16.0
                || p[0] > scan.max[0] + 16.0
                || p[2] < scan.min[2] - 16.0
                || p[2] > scan.max[2] + 16.0
            {
                continue;
            }
            let ebox = self.entity_box(e);
            if !ebox.intersects(&scan) {
                continue;
            }
            if colliders.len() < MAX_COLLIDERS && self.entity_collidable(e, &ebox) {
                colliders.push(ebox);
            }
            if !pushable || pushers.len() >= MAX_PUSHERS {
                continue;
            }
            let name = u32::try_from(e.kind)
                .ok()
                .and_then(|k| self.data.entity_type_name(k))
                .map_or("", short);
            let touches = if is_boat(name) {
                let grown = Aabb::new(
                    [
                        ebox.min[0] - BOAT_PUSH_GROW,
                        ebox.min[1] + 0.01,
                        ebox.min[2] - BOAT_PUSH_GROW,
                    ],
                    [
                        ebox.max[0] + BOAT_PUSH_GROW,
                        ebox.max[1] - 0.01,
                        ebox.max[2] + BOAT_PUSH_GROW,
                    ],
                );
                grown.intersects(&bb) && self.teams_allow(e)
            } else {
                ebox.intersects(&bb) && self.is_living(e) && self.pushes_local_player(e, name)
            };
            if touches {
                pushers.push(p);
            }
        }
        self.player.physics.entity_colliders = colliders;
        let b = &self.world.border;
        self.player.physics.border = BorderBounds::new(
            b.center_x,
            b.center_z,
            b.previous_size,
            f64::from(b.absolute_max_size),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_rules_follow_vanilla() {
        let a = |r| Some(("a", r));
        let b = |r| Some(("b", r));
        assert!(teams_allow_push(None, None));
        assert!(!teams_allow_push(a(CollisionRule::Never), None));
        assert!(!teams_allow_push(None, b(CollisionRule::Never)));
        assert!(!teams_allow_push(
            a(CollisionRule::PushOwnTeam),
            a(CollisionRule::Always)
        ));
        assert!(teams_allow_push(
            a(CollisionRule::PushOwnTeam),
            b(CollisionRule::Always)
        ));
        assert!(!teams_allow_push(
            a(CollisionRule::PushOtherTeams),
            b(CollisionRule::Always)
        ));
        assert!(teams_allow_push(
            a(CollisionRule::PushOtherTeams),
            a(CollisionRule::Always)
        ));
    }
}
