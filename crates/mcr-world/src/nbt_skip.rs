use mcr_buf::take;

use crate::{Result, WorldError};

const MAX_DEPTH: u32 = 512;

/// Consumes one optional network-format NBT value (nameless root, a lone End tag meaning none)
/// and returns its raw bytes, empty when absent.
pub(crate) fn take_optional_nbt<'a>(input: &mut &'a [u8]) -> Result<&'a [u8]> {
    let start = *input;
    let tag = take(input, 1)?[0];
    if tag == 0 {
        return Ok(&[]);
    }
    skip_payload(input, tag, 0)?;
    Ok(&start[..start.len() - input.len()])
}

fn be_len(input: &mut &[u8], width: usize) -> Result<usize> {
    let raw = take(input, 4)?;
    let n = i32::from_be_bytes(raw.try_into().expect("4 bytes"));
    let n = usize::try_from(n).map_err(|_| WorldError::Nbt("negative length"))?;
    n.checked_mul(width)
        .ok_or(WorldError::Nbt("length overflow"))
}

fn skip_payload(input: &mut &[u8], tag: u8, depth: u32) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(WorldError::Nbt("nesting too deep"));
    }
    match tag {
        1 => drop(take(input, 1)?),
        2 => drop(take(input, 2)?),
        3 | 5 => drop(take(input, 4)?),
        4 | 6 => drop(take(input, 8)?),
        7 => {
            let n = be_len(input, 1)?;
            take(input, n)?;
        }
        8 => skip_string(input)?,
        9 => {
            let item = take(input, 1)?[0];
            let raw = take(input, 4)?;
            let count = i32::from_be_bytes(raw.try_into().expect("4 bytes"));
            if count > 0 {
                if item == 0 {
                    return Err(WorldError::Nbt("non-empty list of end tags"));
                }
                for _ in 0..count {
                    skip_payload(input, item, depth + 1)?;
                }
            }
        }
        10 => loop {
            let child = take(input, 1)?[0];
            if child == 0 {
                break;
            }
            skip_string(input)?;
            skip_payload(input, child, depth + 1)?;
        },
        11 => {
            let n = be_len(input, 4)?;
            take(input, n)?;
        }
        12 => {
            let n = be_len(input, 8)?;
            take(input, n)?;
        }
        _ => return Err(WorldError::Nbt("unknown tag type")),
    }
    Ok(())
}

fn skip_string(input: &mut &[u8]) -> Result<()> {
    let raw = take(input, 2)?;
    let n = u16::from_be_bytes([raw[0], raw[1]]) as usize;
    take(input, n)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_is_one_byte() {
        let mut input: &[u8] = &[0, 9, 9];
        assert!(take_optional_nbt(&mut input).unwrap().is_empty());
        assert_eq!(input, [9, 9]);
    }

    #[test]
    fn skips_nested_compound() {
        let mut nbt = vec![10u8, 8, 0, 1, b'a', 0, 2, b'h', b'i'];
        nbt.extend([9, 0, 1, b'l', 3, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 2]);
        nbt.extend([11, 0, 1, b'i', 0, 0, 0, 1, 1, 2, 3, 4]);
        nbt.push(0);
        let mut data = nbt.clone();
        data.push(0xee);
        let mut input = data.as_slice();
        assert_eq!(take_optional_nbt(&mut input).unwrap(), nbt);
        assert_eq!(input, [0xee]);
    }

    #[test]
    fn malformed_input_is_an_error() {
        for bad in [
            &[10u8][..],
            &[10, 1, 0, 1, b'a'],
            &[13],
            &[7, 0xff, 0xff, 0xff, 0xff],
            &[9, 0, 0, 0, 0, 1],
            &[9, 9, 0x7f, 0xff, 0xff, 0xff],
        ] {
            let mut input = bad;
            assert!(take_optional_nbt(&mut input).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let mut data = vec![10u8];
        for _ in 0..2000 {
            data.extend([10u8, 0, 1, b'a']);
        }
        let mut input = data.as_slice();
        assert!(take_optional_nbt(&mut input).is_err());
    }
}
