//! Chunks, light, block changes, particles and sounds.

use std::sync::OnceLock;

use mcr_buf::{Result, read_len, read_varint, read_varlong, take, write_varint, write_varlong};

use mcr_buf::{Encode, IdOr};
use mcr_nbt::{Limits, Value};

use super::items::{self, Codec};
use crate::play::clientbound::world::{LevelEvent, Sound, SoundSource};
use crate::types::Particle;
use crate::types::particle::{BlockParticleKind, ITEM};
use crate::version::io::{self, Depth};
use crate::version::translator::{Action, Cx, Era, Maps};

const BLOCK_ENTRIES: usize = 4096;
const BIOME_ENTRIES: usize = 64;
const NEW_DIRECT_BITS: u8 = 16;
const MAX_SECTION_BUFFER: usize = 2 * 1024 * 1024;
const MAX_LIGHT_SECTIONS: usize = 4096 / 16 + 2;

fn state(m: &Maps, old: i32) -> Result<i32> {
    m.states.forward(old).ok_or(io::unknown("block state", old))
}

fn long_count(bits: u8, entries: usize) -> usize {
    entries.div_ceil(64 / usize::from(bits))
}

fn read_long(r: &mut &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(take(r, 8)?.try_into().expect("8 bytes")))
}

/// Canonical block states holding a fluid, one bit each.
fn fluid_states() -> &'static [u64] {
    static FLUIDS: OnceLock<Vec<u64>> = OnceLock::new();
    FLUIDS.get_or_init(|| {
        let data = mcr_data::v26_3();
        let mut bits = vec![0u64; data.state_count().div_ceil(64)];
        for id in 0..data.state_count() {
            if data
                .block_state(id as u32)
                .is_some_and(|s| s.fluid.is_some())
            {
                bits[id / 64] |= 1 << (id % 64);
            }
        }
        bits
    })
}

fn has_fluid(state: i32) -> bool {
    let id = state as usize;
    fluid_states()
        .get(id / 64)
        .is_some_and(|w| w & (1 << (id % 64)) != 0)
}

const MAX_LANE_TARGETS: usize = 4;

/// Entries of a packed container whose palette index is flagged in `fluid`.
fn count_flagged(data: &[u8], bits: u8, entries: usize, fluid: &[bool]) -> u16 {
    let mut targets = [0u64; MAX_LANE_TARGETS];
    let mut n = 0;
    for (index, _) in fluid.iter().enumerate().filter(|(_, f)| **f) {
        if n == MAX_LANE_TARGETS {
            return count_flagged_scalar(data, bits, entries, fluid);
        }
        targets[n] = index as u64;
        n += 1;
    }
    count_lanes(data, bits, entries, &targets[..n])
}

fn count_lanes(data: &[u8], bits: u8, entries: usize, targets: &[u64]) -> u16 {
    let count = match bits {
        1 => lanes::<1>(data, entries, targets),
        2 => lanes::<2>(data, entries, targets),
        3 => lanes::<3>(data, entries, targets),
        4 => lanes::<4>(data, entries, targets),
        5 => lanes::<5>(data, entries, targets),
        6 => lanes::<6>(data, entries, targets),
        7 => lanes::<7>(data, entries, targets),
        _ => lanes::<8>(data, entries, targets),
    };
    count as u16
}

/// Counts entries equal to any target, comparing every `B`-bit lane of a word at once.
fn lanes<const B: usize>(data: &[u8], entries: usize, targets: &[u64]) -> usize {
    let per_long = 64 / B;
    let lanes = (0..per_long).fold(0u64, |m, k| m | 1 << (k * B));
    let differs = |word: u64, pattern: u64| {
        let x = word ^ pattern;
        (1..B).fold(x, |d, s| d | x >> s) & lanes
    };
    let words = data.as_chunks::<8>().0;
    let (full, rem) = (entries / per_long, entries % per_long);
    let full = full.min(words.len());
    let mut count = 0;
    for &t in targets {
        let pattern = t * lanes;
        let mismatched: u32 = words[..full]
            .iter()
            .map(|w| differs(u64::from_be_bytes(*w), pattern).count_ones())
            .sum();
        count += full * per_long - mismatched as usize;
        if rem > 0
            && let Some(w) = words.get(full)
        {
            let valid = (1u64 << (rem * B)) - 1;
            count += rem - (differs(u64::from_be_bytes(*w), pattern) & valid).count_ones() as usize;
        }
    }
    count
}

fn count_flagged_scalar(data: &[u8], bits: u8, entries: usize, fluid: &[bool]) -> u16 {
    let per_long = 64 / usize::from(bits);
    let mask = (1u64 << bits) - 1;
    let mut count = 0u16;
    let mut left = entries;
    for word in data.as_chunks::<8>().0 {
        let word = u64::from_be_bytes(*word);
        for k in 0..per_long.min(left) {
            let index = ((word >> (k * usize::from(bits))) & mask) as usize;
            count += u16::from(fluid.get(index).copied().unwrap_or(false));
        }
        left = left.saturating_sub(per_long);
    }
    count
}

/// One paletted container; block containers remap ids and widen the direct palette to 26.3's 16 bits.
/// With `fluids`, returns how many entries hold a fluid (the count 1.21.11 did not send).
fn container(m: &Maps, r: &mut &[u8], w: &mut Vec<u8>, blocks: bool, fluids: bool) -> Result<u16> {
    let (entries, min_bits, max_indirect) = if blocks {
        (BLOCK_ENTRIES, 4, 8)
    } else {
        (BIOME_ENTRIES, 1, 3)
    };
    let declared = take(r, 1)?[0];
    if declared == 0 {
        w.push(0);
        if !blocks {
            io::varint(r, w)?;
            return Ok(0);
        }
        let new = state(m, read_varint(r)?)?;
        write_varint(w, new);
        return Ok(if fluids && has_fluid(new) {
            BLOCK_ENTRIES as u16
        } else {
            0
        });
    }
    if declared <= max_indirect {
        w.push(declared);
        let bits = declared.max(min_bits);
        let len = read_len(r, 1 << bits)?;
        write_varint(w, len as i32);
        let mut fluid = [false; 256];
        for flag in fluid.iter_mut().take(len) {
            if blocks {
                let new = state(m, read_varint(r)?)?;
                write_varint(w, new);
                *flag = fluids && has_fluid(new);
            } else {
                io::varint(r, w)?;
            }
        }
        let data = take(r, long_count(bits, entries) * 8)?;
        w.extend_from_slice(data);
        return Ok(if fluid[..len].contains(&true) {
            count_flagged(data, bits, entries, &fluid[..len])
        } else {
            0
        });
    }
    if !blocks || declared > 32 {
        if declared > 32 {
            return Err(io::unknown("palette bits", declared.into()));
        }
        w.push(declared);
        io::bytes(r, w, long_count(declared, entries) * 8)?;
        return Ok(0);
    }
    let per_long = 64 / usize::from(declared);
    let mask = (1u64 << declared) - 1;
    let mut values = [0u16; BLOCK_ENTRIES];
    let mut fluid_count = 0u16;
    let mut i = 0;
    for _ in 0..long_count(declared, entries) {
        let word = read_long(r)?;
        for k in 0..per_long {
            if i == entries {
                break;
            }
            let old = ((word >> (k * usize::from(declared))) & mask) as i32;
            let new = state(m, old)?;
            fluid_count += u16::from(fluids && has_fluid(new));
            values[i] = new as u16;
            i += 1;
        }
    }
    w.push(NEW_DIRECT_BITS);
    let per_long = 64 / usize::from(NEW_DIRECT_BITS);
    for chunk in values.chunks(per_long) {
        let mut word = 0u64;
        for (k, &v) in chunk.iter().enumerate() {
            word |= u64::from(v) << (k * usize::from(NEW_DIRECT_BITS));
        }
        w.extend_from_slice(&word.to_be_bytes());
    }
    Ok(fluid_count)
}

/// 26.1 added a fluid count after each section's block count.
fn sections(m: &Maps, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let len = read_len(r, MAX_SECTION_BUFFER)?;
    let mut buf = take(r, len)?;
    let mut out = Vec::with_capacity(len + len / 8);
    while !buf.is_empty() {
        if m.era == Era::Stacks {
            io::bytes(&mut buf, &mut out, 2)?;
            let at = out.len();
            out.extend_from_slice(&[0, 0]);
            let fluids = container(m, &mut buf, &mut out, true, true)?;
            out[at..at + 2].copy_from_slice(&fluids.to_be_bytes());
        } else {
            io::bytes(&mut buf, &mut out, 4)?;
            container(m, &mut buf, &mut out, true, false)?;
        }
        container(m, &mut buf, &mut out, false, false)?;
    }
    write_varint(w, out.len() as i32);
    w.extend_from_slice(&out);
    Ok(())
}

/// 26.2 sent bit sets as big-endian longs, 26.3 as bytes with trailing zeros dropped.
pub fn bit_set(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let longs = read_len(r, r.len() / 8)?;
    let mut bytes = Vec::with_capacity(longs * 8);
    for _ in 0..longs {
        bytes.extend_from_slice(&read_long(r)?.to_le_bytes());
    }
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    write_varint(w, bytes.len() as i32);
    w.extend_from_slice(&bytes);
    Ok(())
}

fn light(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    for _ in 0..4 {
        bit_set(r, w)?;
    }
    for _ in 0..2 {
        for _ in 0..io::len(r, w, MAX_LIGHT_SECTIONS)? {
            let n = io::len(r, w, 2048)?;
            io::bytes(r, w, n)?;
        }
    }
    Ok(())
}

pub fn level_chunk_with_light(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    for _ in 0..io::len(r, w, 16)? {
        io::varint(r, w)?;
        let words = io::len(r, w, 4096)?;
        io::bytes(r, w, words * 8)?;
    }
    sections(cx.m, r, w)?;
    let n = read_len(r, r.len())?;
    let mut body = Vec::new();
    let mut kept = 0;
    for _ in 0..n {
        let xz_y = take(r, 3)?;
        let mut entry = Vec::with_capacity(16);
        entry.extend_from_slice(xz_y);
        if block_entity(cx.m, r, &mut entry, true)? {
            body.extend_from_slice(&entry);
            kept += 1;
        }
    }
    write_varint(w, kept);
    w.extend_from_slice(&body);
    light(r, w)?;
    Ok(Action::Rewritten)
}

/// A block entity type and tag; `false` when the type has no canonical entry (26.1's beds).
fn block_entity(m: &Maps, r: &mut &[u8], w: &mut Vec<u8>, optional: bool) -> Result<bool> {
    let old = read_varint(r)?;
    let Some(kind) = m.block_entity.forward(old) else {
        io::nbt(r, &mut Vec::new(), optional)?;
        return Ok(false);
    };
    write_varint(w, kind);
    if Some(kind) != m.canonical.decorated_pot {
        io::nbt(r, w, optional)?;
        return Ok(true);
    }
    let nbt_error = |e: mcr_nbt::NbtError| mcr_buf::DecodeError::Other(format!("nbt: {e}"));
    match mcr_nbt::read_network(r, Limits::NETWORK).map_err(nbt_error)? {
        Some(Value::Compound(mut tag)) => {
            items::pot_sherds(&mut tag);
            mcr_nbt::write_network(w, Some(&Value::Compound(tag)));
        }
        Some(_) => return Err(io::unknown("block entity tag", 0)),
        None if optional => w.push(mcr_nbt::tag::END),
        None => return Err(io::unknown("block entity tag", 0)),
    }
    Ok(true)
}

pub fn block_entity_data(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    Ok(if block_entity(cx.m, r, w, false)? {
        Action::Rewritten
    } else {
        Action::Drop
    })
}

pub fn light_update(_: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::varint(r, w)?;
    light(r, w)?;
    Ok(Action::Rewritten)
}

pub fn block_update(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    write_varint(w, state(cx.m, read_varint(r)?)?);
    Ok(Action::Rewritten)
}

pub fn section_blocks_update(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    for _ in 0..io::len(r, w, BLOCK_ENTRIES)? {
        let packed = read_varlong(r)?;
        let new = state(cx.m, (packed >> 12) as i32)?;
        write_varlong(w, i64::from(new) << 12 | (packed & 0xfff));
    }
    Ok(Action::Rewritten)
}

pub fn block_event(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 10)?;
    io::remap(r, w, "block", |id| cx.m.block.forward(id))?;
    Ok(Action::Rewritten)
}

/// Block break particles (2001) and block-state particle bursts (3008) carry a state id. 26.3 moved
/// the sounds of splash particles (2002, 2007) and waxing (3003) into their own packets.
pub fn level_event(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let event = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
    w.extend_from_slice(&event.to_be_bytes());
    let packed = take(r, 8)?;
    w.extend_from_slice(packed);
    let data = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
    let data = match event {
        2001 | 3008 => state(cx.m, data)?,
        _ => data,
    };
    w.extend_from_slice(&data.to_be_bytes());
    io::bool(r, w)?;
    let pos = <mcr_buf::BlockPos as mcr_buf::Decode>::decode(&mut &packed[..])?;
    match event {
        2002 | 2007 => cx.emit("level_event", |w| {
            LevelEvent {
                event: if event == 2002 { 1053 } else { 1054 },
                pos,
                data: 0,
                global: false,
            }
            .encode(w)
        })?,
        3003 => {
            if let Some(sound) = cx.m.canonical.wax_on_sound {
                cx.emit("sound", |w| {
                    Sound {
                        sound: IdOr::Id(sound),
                        source: SoundSource::Blocks,
                        x: pos.x * 8 + 4,
                        y: pos.y * 8 + 4,
                        z: pos.z * 8 + 4,
                        volume: 1.0,
                        pitch: 1.0,
                        seed: 0,
                    }
                    .encode(w)
                })?;
            }
        }
        _ => {}
    }
    Ok(Action::Rewritten)
}

/// A particle type id and its options; item particles nest stacks, so they take the caller's depth.
pub fn particle(m: &'static Maps, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
    let new = io::remap(r, w, "particle type", |id| m.particle.forward(id))?;
    if BlockParticleKind::from_id(new).is_some() {
        write_varint(w, state(m, read_varint(r)?)?);
    } else if new == ITEM {
        Codec::fwd(m).template(r, w, d)?;
    } else {
        let start = *r;
        Particle::decode_options(new, r)?;
        w.extend_from_slice(&start[..start.len() - r.len()]);
    }
    Ok(())
}

/// 26.3 moved the particle first, split max speed per axis, made the count a VarInt and added the
/// randomization type.
pub fn level_particles(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let head = take(r, 2 + 24 + 12)?;
    let speed = take(r, 4)?;
    let count = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
    particle(cx.m, r, w, Depth::top())?;
    w.extend_from_slice(head);
    for _ in 0..3 {
        w.extend_from_slice(speed);
    }
    write_varint(w, count.max(0));
    write_varint(w, 0);
    Ok(Action::Rewritten)
}

/// 26.2 explosions always play their sound.
pub fn explode(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 24 + 4 + 4)?;
    if io::bool(r, w)? {
        io::bytes(r, w, 24)?;
    }
    particle(cx.m, r, w, Depth::top())?;
    Codec::fwd(cx.m).sound(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        particle(cx.m, r, w, Depth::top())?;
        io::bytes(r, w, 8)?;
        io::varint(r, w)?;
    }
    w.push(1);
    Ok(Action::Rewritten)
}

pub fn sound(cx: &mut Cx<'_>, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    Codec::fwd(cx.m).sound(r, w)?;
    io::rest(r, w);
    Ok(Action::Rewritten)
}

#[cfg(test)]
mod fluid_count_tests {
    use super::*;

    #[test]
    fn lane_counts_match_the_scalar_count() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for bits in 1..=8u8 {
            for entries in [BIOME_ENTRIES, BLOCK_ENTRIES] {
                let data: Vec<u8> = (0..long_count(bits, entries) * 8)
                    .map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        seed as u8
                    })
                    .collect();
                let palette = 1usize << bits;
                for flagged in [vec![0], vec![1], vec![0, palette - 1], vec![2, 3, 5]] {
                    let mut fluid = vec![false; palette];
                    for i in flagged {
                        fluid[i.min(palette - 1)] = true;
                    }
                    assert_eq!(
                        count_flagged(&data, bits, entries, &fluid),
                        count_flagged_scalar(&data, bits, entries, &fluid),
                        "{bits} bits, {entries} entries"
                    );
                }
            }
        }
    }
}
