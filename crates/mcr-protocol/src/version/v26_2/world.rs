//! Chunks, light, block changes, particles and sounds.

use mcr_buf::{Result, read_len, read_varint, read_varlong, take, write_varint, write_varlong};

use super::{items, maps};
use crate::types::Particle;
use crate::types::particle::{BlockParticleKind, ITEM};
use crate::version::io::{self, Depth};
use crate::version::translator::{Action, Translator};

const BLOCK_ENTRIES: usize = 4096;
const BIOME_ENTRIES: usize = 64;
const NEW_DIRECT_BITS: u8 = 16;
const MAX_SECTION_BUFFER: usize = 2 * 1024 * 1024;
const MAX_LIGHT_SECTIONS: usize = 4096 / 16 + 2;

fn state(old: i32) -> Result<i32> {
    maps()
        .states
        .forward(old)
        .ok_or(io::unknown("block state", old))
}

fn long_count(bits: u8, entries: usize) -> usize {
    entries.div_ceil(64 / usize::from(bits))
}

fn read_long(r: &mut &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(take(r, 8)?.try_into().expect("8 bytes")))
}

/// One paletted container; block containers remap ids and widen the direct palette to 26.3's 16 bits.
fn container(r: &mut &[u8], w: &mut Vec<u8>, blocks: bool) -> Result<()> {
    let (entries, min_bits, max_indirect) = if blocks {
        (BLOCK_ENTRIES, 4, 8)
    } else {
        (BIOME_ENTRIES, 1, 3)
    };
    let declared = take(r, 1)?[0];
    if declared == 0 {
        w.push(0);
        if blocks {
            write_varint(w, state(read_varint(r)?)?);
        } else {
            io::varint(r, w)?;
        }
        return Ok(());
    }
    if declared <= max_indirect {
        w.push(declared);
        let bits = declared.max(min_bits);
        let len = read_len(r, 1 << bits)?;
        write_varint(w, len as i32);
        for _ in 0..len {
            if blocks {
                write_varint(w, state(read_varint(r)?)?);
            } else {
                io::varint(r, w)?;
            }
        }
        return io::bytes(r, w, long_count(bits, entries) * 8);
    }
    if !blocks || declared > 32 {
        if declared > 32 {
            return Err(io::unknown("palette bits", declared.into()));
        }
        w.push(declared);
        return io::bytes(r, w, long_count(declared, entries) * 8);
    }
    let per_long = 64 / usize::from(declared);
    let mask = (1u64 << declared) - 1;
    let mut values = [0u16; BLOCK_ENTRIES];
    let mut i = 0;
    for _ in 0..long_count(declared, entries) {
        let word = read_long(r)?;
        for k in 0..per_long {
            if i == entries {
                break;
            }
            let old = ((word >> (k * usize::from(declared))) & mask) as i32;
            values[i] = state(old)? as u16;
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
    Ok(())
}

fn sections(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let len = read_len(r, MAX_SECTION_BUFFER)?;
    let mut buf = take(r, len)?;
    let mut out = Vec::with_capacity(len + len / 8);
    while !buf.is_empty() {
        io::bytes(&mut buf, &mut out, 4)?;
        container(&mut buf, &mut out, true)?;
        container(&mut buf, &mut out, false)?;
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

pub fn level_chunk_with_light(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    for _ in 0..io::len(r, w, 16)? {
        io::varint(r, w)?;
        let words = io::len(r, w, 4096)?;
        io::bytes(r, w, words * 8)?;
    }
    sections(r, w)?;
    for _ in 0..io::len(r, w, 1 << 20)? {
        io::bytes(r, w, 3)?;
        io::varint(r, w)?;
        io::copy::<mcr_nbt::OptionalNetworkNbt>(r, w)?;
    }
    light(r, w)?;
    Ok(Action::Rewritten)
}

pub fn light_update(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::varint(r, w)?;
    io::varint(r, w)?;
    light(r, w)?;
    Ok(Action::Rewritten)
}

pub fn block_update(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    write_varint(w, state(read_varint(r)?)?);
    Ok(Action::Rewritten)
}

pub fn section_blocks_update(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 8)?;
    for _ in 0..io::len(r, w, BLOCK_ENTRIES)? {
        let packed = read_varlong(r)?;
        let new = state((packed >> 12) as i32)?;
        write_varlong(w, i64::from(new) << 12 | (packed & 0xfff));
    }
    Ok(Action::Rewritten)
}

pub fn block_event(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 10)?;
    io::remap(r, w, "block", |id| maps().block.forward(id))?;
    Ok(Action::Rewritten)
}

/// Block break particles (2001) and block-state particle bursts (3008) carry a state id.
pub fn level_event(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let event = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
    w.extend_from_slice(&event.to_be_bytes());
    io::bytes(r, w, 8)?;
    let data = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
    let data = match event {
        2001 | 3008 => state(data)?,
        _ => data,
    };
    w.extend_from_slice(&data.to_be_bytes());
    io::rest(r, w);
    Ok(Action::Rewritten)
}

/// A particle type id and its options.
pub fn particle(r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
    let new = io::remap(r, w, "particle type", |id| maps().particle.forward(id))?;
    if BlockParticleKind::from_id(new).is_some() {
        write_varint(w, state(read_varint(r)?)?);
    } else if new == ITEM {
        items::template(r, w, Depth::top())?;
    } else {
        let start = *r;
        Particle::decode_options(new, r)?;
        w.extend_from_slice(&start[..start.len() - r.len()]);
    }
    Ok(())
}

/// 26.3 moved the particle first, split max speed per axis, made the count a VarInt and added the
/// randomization type.
pub fn level_particles(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    let head = take(r, 2 + 24 + 12)?;
    let speed = take(r, 4)?;
    let count = i32::from_be_bytes(take(r, 4)?.try_into().expect("4 bytes"));
    particle(r, w)?;
    w.extend_from_slice(head);
    for _ in 0..3 {
        w.extend_from_slice(speed);
    }
    write_varint(w, count.max(0));
    write_varint(w, 0);
    Ok(Action::Rewritten)
}

/// 26.2 explosions always play their sound.
pub fn explode(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    io::bytes(r, w, 24 + 4 + 4)?;
    if io::bool(r, w)? {
        io::bytes(r, w, 24)?;
    }
    particle(r, w)?;
    items::sound(r, w)?;
    for _ in 0..io::len(r, w, r.len())? {
        particle(r, w)?;
        io::bytes(r, w, 8)?;
        io::varint(r, w)?;
    }
    w.push(1);
    Ok(Action::Rewritten)
}

pub fn sound(_: &Translator, r: &mut &[u8], w: &mut Vec<u8>) -> Result<Action> {
    items::sound(r, w)?;
    io::rest(r, w);
    Ok(Action::Rewritten)
}
