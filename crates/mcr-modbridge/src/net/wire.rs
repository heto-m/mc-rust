use mcr_client::LocalPlayer;
use mcr_physics::Pose;

use crate::abi::*;

/// Hostile-input bounds for Java's probe answers.
pub const MAX_PROBE_POSITIONS: usize = 1024;
const MAX_ANSWER_ITEMS: usize = 1 << 20;

pub(crate) fn put_header(buf: &mut [u8], tag: u8, session: u32) {
    buf[..NET_HEADER].fill(0);
    buf[NET_TAG] = tag;
    buf[NET_SESSION..NET_SESSION + 4].copy_from_slice(&session.to_ne_bytes());
}

pub(crate) fn header(payload: &[u8]) -> Option<(u8, u32)> {
    let session = payload.get(NET_SESSION..NET_SESSION + 4)?;
    Some((
        payload[NET_TAG],
        u32::from_ne_bytes(session.try_into().ok()?),
    ))
}

/// Writes `value` as a VarInt into the front of `buf` and returns its length.
pub(crate) fn put_varint(buf: &mut [u8], value: i32) -> usize {
    let mut v = value as u32;
    let mut i = 0;
    loop {
        if v < 0x80 {
            buf[i] = v as u8;
            return i + 1;
        }
        buf[i] = (v as u8 & 0x7f) | 0x80;
        v >>= 7;
        i += 1;
    }
}

fn put<const N: usize>(buf: &mut [u8], at: usize, bytes: [u8; N]) {
    buf[at..at + N].copy_from_slice(&bytes);
}

fn get<const N: usize>(buf: &[u8], at: usize) -> Option<[u8; N]> {
    buf.get(at..at + N)?.try_into().ok()
}

/// What Java's `LocalPlayer` is overwritten with at the end of each of its ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerState {
    pub tick: u64,
    pub pos: [f64; 3],
    pub old_pos: [f64; 3],
    pub velocity: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u32,
    /// Vanilla `Pose` id.
    pub pose: i32,
    pub slot: i32,
    pub vehicle: i32,
}

impl PlayerState {
    pub fn from_player(tick: u64, player: &LocalPlayer, old_pos: [f64; 3]) -> Self {
        let p = &player.physics;
        let mut flags = 0;
        for (on, bit) in [
            (p.on_ground, PLAYER_ON_GROUND),
            (p.pose == Pose::Crouching, PLAYER_SHIFT),
            (p.sprinting, PLAYER_SPRINT),
            (p.swimming, PLAYER_SWIM),
            (p.fall_flying, PLAYER_FALL_FLYING),
        ] {
            if on {
                flags |= bit;
            }
        }
        let pose = match p.pose {
            Pose::Standing => 0,
            Pose::FallFlying => 1,
            Pose::Swimming => 3,
            Pose::SpinAttack => 4,
            Pose::Crouching => 5,
        };
        Self {
            tick,
            pos: p.pos,
            old_pos,
            velocity: p.vel,
            yaw: p.yaw,
            pitch: p.pitch,
            flags,
            pose,
            slot: i32::from(player.held_slot),
            vehicle: player.vehicle.unwrap_or(-1),
        }
    }

    pub(crate) fn encode(&self, buf: &mut [u8]) {
        put(buf, PLAYER_TICK, self.tick.to_ne_bytes());
        for i in 0..3 {
            put(buf, PLAYER_POS + 8 * i, self.pos[i].to_ne_bytes());
            put(buf, PLAYER_OLD_POS + 8 * i, self.old_pos[i].to_ne_bytes());
            put(buf, PLAYER_VELOCITY + 8 * i, self.velocity[i].to_ne_bytes());
        }
        put(buf, PLAYER_YAW, self.yaw.to_ne_bytes());
        put(buf, PLAYER_PITCH, self.pitch.to_ne_bytes());
        put(buf, PLAYER_FLAGS, self.flags.to_ne_bytes());
        put(buf, PLAYER_POSE, self.pose.to_ne_bytes());
        put(buf, PLAYER_SLOT, self.slot.to_ne_bytes());
        put(buf, PLAYER_VEHICLE, self.vehicle.to_ne_bytes());
    }
}

/// Asks Java for a snapshot of its client world.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProbeRequest {
    /// Chunk square to scan for loaded columns: `center ± radius`.
    pub center: (i32, i32),
    pub radius: i32,
    pub positions: Vec<[i32; 3]>,
    /// Run a full GC first so the heap figure is the live set.
    pub gc: bool,
    /// Test control: Java then leaves the world the way its pause menu does.
    pub disconnect: bool,
}

impl ProbeRequest {
    pub(crate) fn len(&self) -> usize {
        PROBE_POSITIONS + 12 * self.positions.len().min(MAX_PROBE_POSITIONS)
    }

    pub(crate) fn encode(&self, request: u32, buf: &mut [u8]) {
        let positions = &self.positions[..self.positions.len().min(MAX_PROBE_POSITIONS)];
        put(buf, PROBE_REQUEST, request.to_ne_bytes());
        put(buf, PROBE_CENTER_X, self.center.0.to_ne_bytes());
        put(buf, PROBE_CENTER_Z, self.center.1.to_ne_bytes());
        put(buf, PROBE_RADIUS, self.radius.clamp(0, 64).to_ne_bytes());
        let mut flags = 0;
        if self.gc {
            flags |= PROBE_GC;
        }
        if self.disconnect {
            flags |= PROBE_DISCONNECT;
        }
        put(buf, PROBE_FLAGS, flags.to_ne_bytes());
        put(buf, PROBE_COUNT, (positions.len() as u32).to_ne_bytes());
        for (i, p) in positions.iter().enumerate() {
            for (j, v) in p.iter().enumerate() {
                put(buf, PROBE_POSITIONS + 12 * i + 4 * j, v.to_ne_bytes());
            }
        }
    }
}

/// Java's view of its world at the moment it handled a [`ProbeRequest`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorldProbe {
    pub request: u32,
    pub session: u32,
    pub level: bool,
    /// Tick of the last [`PlayerState`] Java applied, with Java's player position after it.
    pub player: Option<(u64, [f64; 3])>,
    pub heap_used: u64,
    pub chunk_packets: u64,
    pub chunk_nanos: u64,
    pub frames_fed: u64,
    pub feed_nanos: u64,
    pub chunks: Vec<(i32, i32)>,
    /// Global block state ids at the requested positions, in order.
    pub blocks: Vec<i32>,
    pub entities: Vec<i32>,
}

impl WorldProbe {
    pub(crate) fn decode(payload: &[u8]) -> Option<Self> {
        let (_, session) = header(payload)?;
        let u32_at = |at| get::<4>(payload, at).map(u32::from_ne_bytes);
        let u64_at = |at| get::<8>(payload, at).map(u64::from_ne_bytes);
        let f64_at = |at| get::<8>(payload, at).map(f64::from_ne_bytes);
        let flags = u32_at(ANSWER_FLAGS)?;
        let counts = [
            u32_at(ANSWER_CHUNKS)? as usize,
            u32_at(ANSWER_BLOCKS)? as usize,
            u32_at(ANSWER_ENTITIES)? as usize,
        ];
        if counts.iter().any(|&c| c > MAX_ANSWER_ITEMS) {
            return None;
        }
        let ints = 2 * counts[0] + counts[1] + counts[2];
        let data = payload.get(ANSWER_DATA..ANSWER_DATA + 4 * ints)?;
        let mut words = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| i32::from_ne_bytes(*w));
        let chunks = (0..counts[0])
            .map(|_| (words.next().unwrap_or(0), words.next().unwrap_or(0)))
            .collect();
        let blocks = words.by_ref().take(counts[1]).collect();
        let entities = words.take(counts[2]).collect();
        let player = (flags & ANSWER_PLAYER != 0).then(|| {
            let pos = [0, 1, 2].map(|i| f64_at(ANSWER_POS + 8 * i).unwrap_or(f64::NAN));
            (u64_at(ANSWER_PLAYER_TICK).unwrap_or(0), pos)
        });
        Some(Self {
            request: u32_at(ANSWER_REQUEST)?,
            session,
            level: flags & ANSWER_LEVEL != 0,
            player,
            heap_used: u64_at(ANSWER_HEAP)?,
            chunk_packets: u64_at(ANSWER_CHUNK_PACKETS)?,
            chunk_nanos: u64_at(ANSWER_CHUNK_NANOS)?,
            frames_fed: u64_at(ANSWER_FRAMES)?,
            feed_nanos: u64_at(ANSWER_FEED_NANOS)?,
            chunks,
            blocks,
            entities,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds an answer the way `McrNet.answerProbe` lays it out.
    pub fn answer(probe: &WorldProbe) -> Vec<u8> {
        let ints = 2 * probe.chunks.len() + probe.blocks.len() + probe.entities.len();
        let mut buf = vec![0u8; ANSWER_DATA + 4 * ints];
        put_header(&mut buf, 0, probe.session);
        let mut flags = 0;
        if probe.level {
            flags |= ANSWER_LEVEL;
        }
        if let Some((tick, pos)) = probe.player {
            flags |= ANSWER_PLAYER;
            put(&mut buf, ANSWER_PLAYER_TICK, tick.to_ne_bytes());
            for (i, v) in pos.iter().enumerate() {
                put(&mut buf, ANSWER_POS + 8 * i, v.to_ne_bytes());
            }
        }
        put(&mut buf, ANSWER_REQUEST, probe.request.to_ne_bytes());
        put(&mut buf, ANSWER_FLAGS, flags.to_ne_bytes());
        put(&mut buf, ANSWER_HEAP, probe.heap_used.to_ne_bytes());
        put(
            &mut buf,
            ANSWER_CHUNK_PACKETS,
            probe.chunk_packets.to_ne_bytes(),
        );
        put(
            &mut buf,
            ANSWER_CHUNK_NANOS,
            probe.chunk_nanos.to_ne_bytes(),
        );
        put(&mut buf, ANSWER_FRAMES, probe.frames_fed.to_ne_bytes());
        put(&mut buf, ANSWER_FEED_NANOS, probe.feed_nanos.to_ne_bytes());
        put(
            &mut buf,
            ANSWER_CHUNKS,
            (probe.chunks.len() as u32).to_ne_bytes(),
        );
        put(
            &mut buf,
            ANSWER_BLOCKS,
            (probe.blocks.len() as u32).to_ne_bytes(),
        );
        put(
            &mut buf,
            ANSWER_ENTITIES,
            (probe.entities.len() as u32).to_ne_bytes(),
        );
        let mut at = ANSWER_DATA;
        let ints = probe
            .chunks
            .iter()
            .flat_map(|&(x, z)| [x, z])
            .chain(probe.blocks.iter().copied())
            .chain(probe.entities.iter().copied());
        for v in ints {
            put(&mut buf, at, v.to_ne_bytes());
            at += 4;
        }
        buf
    }

    #[test]
    fn varints_match_mcr_buf() {
        for v in [
            0,
            1,
            127,
            128,
            255,
            25565,
            2_097_151,
            i32::MAX,
            -1,
            i32::MIN,
        ] {
            let mut want = Vec::new();
            mcr_buf::write_varint(&mut want, v);
            let mut buf = [0u8; 5];
            let n = put_varint(&mut buf, v);
            assert_eq!(&buf[..n], want.as_slice(), "{v}");
        }
    }

    #[test]
    fn probe_answers_round_trip_and_reject_lies() {
        let probe = WorldProbe {
            request: 7,
            session: 3,
            level: true,
            player: Some((99, [1.5, 64.0, -3.25])),
            heap_used: 1 << 30,
            chunk_packets: 441,
            chunk_nanos: 12_345_678,
            frames_fed: 3000,
            feed_nanos: 9_000_000,
            chunks: vec![(0, 0), (-1, 2)],
            blocks: vec![1, 0, 2008],
            entities: vec![5, 6, 7],
        };
        let bytes = answer(&probe);
        assert_eq!(WorldProbe::decode(&bytes), Some(probe.clone()));
        assert_eq!(WorldProbe::decode(&bytes[..bytes.len() - 1]), None);
        let mut lying = bytes.clone();
        lying[ANSWER_ENTITIES..ANSWER_ENTITIES + 4].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert_eq!(WorldProbe::decode(&lying), None);
        assert_eq!(WorldProbe::decode(&[0; 4]), None);
    }

    #[test]
    fn requests_clamp_positions() {
        let req = ProbeRequest {
            center: (1, -2),
            radius: 500,
            positions: vec![[1, 2, 3]; MAX_PROBE_POSITIONS + 5],
            gc: true,
            disconnect: false,
        };
        let mut buf = vec![0u8; req.len()];
        req.encode(4, &mut buf);
        assert_eq!(buf.len(), PROBE_POSITIONS + 12 * MAX_PROBE_POSITIONS);
        let u32_at = |at: usize| u32::from_ne_bytes(buf[at..at + 4].try_into().unwrap());
        assert_eq!(u32_at(PROBE_RADIUS), 64);
        assert_eq!(u32_at(PROBE_COUNT) as usize, MAX_PROBE_POSITIONS);
        assert_eq!(u32_at(PROBE_FLAGS), PROBE_GC);
        assert_eq!(u32_at(PROBE_POSITIONS + 8), 3);
    }
}
