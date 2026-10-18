//! Wire shapes of modded registry entries (data component types, particle types, entity data
//! serializers), reported by the mod bridge, so values of types Rust has no codec for can be stepped
//! over and carried as bytes.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use mcr_buf::{Decode, DecodeError, Result, read_len, read_varint, take};

use super::component::DataComponentPatch;
use super::item::{ItemStack, ItemStackTemplate};

const MAX_TOKENS: usize = 1024;
const MAX_DEPTH: u32 = 24;
const MAX_FIXED: usize = 4096;
const MAX_SEQ: usize = 64;
/// Steps one value may take; a list of empty elements is otherwise unbounded.
const FUEL: u32 = 1 << 16;
const MAX_STRING_BYTES: usize = 32767 * 3;
/// A modded value carried as bytes, at most this long.
pub const MAX_VALUE_BYTES: usize = 1 << 21;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Bool,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    VarInt,
    VarLong,
    Str,
    Nbt,
    Bytes,
    HolderSet,
    Item,
    Template,
    Patch,
    Unit,
    Fixed(usize),
    LengthPrefixed,
    Opt(Box<Shape>),
    List(Box<Shape>),
    FixedList(usize, Box<Shape>),
    Map(Box<Shape>, Box<Shape>),
    Either(Box<Shape>, Box<Shape>),
    Holder(Box<Shape>),
    Seq(Vec<Shape>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("bad wire shape: {0}")]
pub struct ShapeError(&'static str);

impl Shape {
    /// Parses the bridge's prefix notation (see `McrCodecs` in the bridge mod).
    pub fn parse(text: &str) -> std::result::Result<Self, ShapeError> {
        let tokens: Vec<&str> = text.split_ascii_whitespace().take(MAX_TOKENS + 1).collect();
        if tokens.len() > MAX_TOKENS {
            return Err(ShapeError("too long"));
        }
        let mut rest = &tokens[..];
        let shape = Self::parse_one(&mut rest, 0)?;
        if !rest.is_empty() {
            return Err(ShapeError("trailing tokens"));
        }
        Ok(shape)
    }

    fn parse_one(rest: &mut &[&str], depth: u32) -> std::result::Result<Self, ShapeError> {
        if depth > MAX_DEPTH {
            return Err(ShapeError("too deep"));
        }
        let (&head, tail) = rest.split_first().ok_or(ShapeError("truncated"))?;
        *rest = tail;
        let sub = |rest: &mut &[&str]| Self::parse_one(rest, depth + 1).map(Box::new);
        let number = |rest: &mut &[&str], max: usize| {
            let (&n, tail) = rest.split_first().ok_or(ShapeError("truncated"))?;
            *rest = tail;
            n.parse::<usize>()
                .ok()
                .filter(|&n| n <= max)
                .ok_or(ShapeError("bad count"))
        };
        Ok(match head {
            "bool" => Self::Bool,
            "i8" => Self::I8,
            "i16" => Self::I16,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "vi" => Self::VarInt,
            "vl" => Self::VarLong,
            "str" => Self::Str,
            "nbt" => Self::Nbt,
            "bytes" => Self::Bytes,
            "hs" => Self::HolderSet,
            "item" => Self::Item,
            "tmpl" => Self::Template,
            "patch" => Self::Patch,
            "unit" => Self::Unit,
            "len" => Self::LengthPrefixed,
            "fix" => Self::Fixed(number(rest, MAX_FIXED)?),
            "opt" => Self::Opt(sub(rest)?),
            "list" => Self::List(sub(rest)?),
            "nlist" => {
                let n = number(rest, MAX_FIXED)?;
                Self::FixedList(n, sub(rest)?)
            }
            "map" => {
                let k = sub(rest)?;
                Self::Map(k, sub(rest)?)
            }
            "either" => {
                let l = sub(rest)?;
                Self::Either(l, sub(rest)?)
            }
            "holder" => Self::Holder(sub(rest)?),
            "seq" => {
                let n = number(rest, MAX_SEQ)?;
                let mut parts = Vec::with_capacity(n);
                for _ in 0..n {
                    parts.push(Self::parse_one(rest, depth + 1)?);
                }
                Self::Seq(parts)
            }
            _ => return Err(ShapeError("unknown token")),
        })
    }

    /// Steps over one value of this shape; returns the bytes it took.
    pub fn skip<'a>(&self, input: &mut &'a [u8]) -> Result<&'a [u8]> {
        let start = *input;
        let mut fuel = FUEL;
        self.skip_inner(input, &mut fuel)?;
        let taken = start.len() - input.len();
        if taken > MAX_VALUE_BYTES {
            return Err(DecodeError::Other("modded value too large".into()));
        }
        Ok(&start[..taken])
    }

    fn skip_inner(&self, input: &mut &[u8], fuel: &mut u32) -> Result<()> {
        *fuel = fuel
            .checked_sub(1)
            .ok_or_else(|| DecodeError::Other("modded value too complex".into()))?;
        match self {
            Self::Bool | Self::I8 => {
                take(input, 1)?;
            }
            Self::I16 => {
                take(input, 2)?;
            }
            Self::I32 | Self::F32 => {
                take(input, 4)?;
            }
            Self::I64 | Self::F64 => {
                take(input, 8)?;
            }
            Self::Fixed(n) => {
                take(input, *n)?;
            }
            Self::VarInt => {
                read_varint(input)?;
            }
            Self::VarLong => {
                mcr_buf::read_varlong(input)?;
            }
            Self::Str => {
                let len = read_len(input, MAX_STRING_BYTES)?;
                take(input, len)?;
            }
            Self::Bytes | Self::LengthPrefixed => {
                let len = read_len(input, input.len())?;
                take(input, len)?;
            }
            Self::Nbt => match mcr_nbt::raw::network_root(input)? {
                None => {
                    take(input, 1)?;
                }
                Some(root) => {
                    let size = root.size()?;
                    take(input, 1 + size)?;
                }
            },
            Self::HolderSet => match read_len(input, input.len() + 1)? {
                0 => Self::Str.skip_inner(input, fuel)?,
                n => {
                    for _ in 1..n {
                        read_varint(input)?;
                    }
                }
            },
            Self::Item => {
                ItemStack::decode(input)?;
            }
            Self::Template => {
                ItemStackTemplate::decode(input)?;
            }
            Self::Patch => {
                DataComponentPatch::decode(input)?;
            }
            Self::Unit => {}
            Self::Opt(inner) => {
                if bool::decode(input)? {
                    inner.skip_inner(input, fuel)?;
                }
            }
            Self::List(inner) => {
                let n = read_len(input, usize::MAX)?;
                for _ in 0..n {
                    inner.skip_inner(input, fuel)?;
                }
            }
            Self::FixedList(n, inner) => {
                for _ in 0..*n {
                    inner.skip_inner(input, fuel)?;
                }
            }
            Self::Map(k, v) => {
                let n = read_len(input, usize::MAX)?;
                for _ in 0..n {
                    k.skip_inner(input, fuel)?;
                    v.skip_inner(input, fuel)?;
                }
            }
            Self::Either(l, r) => {
                if bool::decode(input)? {
                    l.skip_inner(input, fuel)?;
                } else {
                    r.skip_inner(input, fuel)?;
                }
            }
            Self::Holder(direct) => {
                if read_varint(input)? == 0 {
                    direct.skip_inner(input, fuel)?;
                }
            }
            Self::Seq(parts) => {
                for p in parts {
                    p.skip_inner(input, fuel)?;
                }
            }
        }
        Ok(())
    }
}

/// Modded entries by registry id; `None` where the bridge found no shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModdedCodecs {
    pub components: HashMap<i32, Option<Shape>>,
    pub particles: HashMap<i32, Option<Shape>>,
    pub entity_data: HashMap<i32, Option<Shape>>,
}

static ACTIVE: RwLock<Option<Arc<ModdedCodecs>>> = RwLock::new(None);

/// Makes `codecs` the table decoding consults; `None` clears it (vanilla).
pub fn install(codecs: Option<ModdedCodecs>) {
    *ACTIVE.write().unwrap_or_else(PoisonError::into_inner) = codecs.map(Arc::new);
}

pub fn active() -> Option<Arc<ModdedCodecs>> {
    ACTIVE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub(crate) enum Lookup {
    Unknown,
    NoShape,
    Shape(Arc<ModdedCodecs>),
}

pub(crate) fn lookup(pick: fn(&ModdedCodecs) -> &HashMap<i32, Option<Shape>>, id: i32) -> Lookup {
    let Some(table) = active() else {
        return Lookup::Unknown;
    };
    match pick(&table).get(&id) {
        None => Lookup::Unknown,
        Some(None) => Lookup::NoShape,
        Some(Some(_)) => Lookup::Shape(table),
    }
}

/// Steps over a value of modded type `id` from `pick`'s table; `Err` when the type has no shape.
pub(crate) fn skip_modded<'a>(
    pick: fn(&ModdedCodecs) -> &HashMap<i32, Option<Shape>>,
    what: &'static str,
    id: i32,
    input: &mut &'a [u8],
) -> Result<&'a [u8]> {
    match lookup(pick, id) {
        Lookup::Shape(table) => match pick(&table).get(&id) {
            Some(Some(shape)) => shape.skip(input),
            _ => Err(unknown(what, id)),
        },
        Lookup::NoShape => Err(DecodeError::Other(format!(
            "modded {what} {id} has no wire shape"
        ))),
        Lookup::Unknown => Err(unknown(what, id)),
    }
}

fn unknown(what: &'static str, id: i32) -> DecodeError {
    DecodeError::InvalidValue {
        what,
        value: id.into(),
    }
}

/// Whether `id` names a modded entry of `pick`'s table, with or without a shape.
pub(crate) fn is_modded(pick: fn(&ModdedCodecs) -> &HashMap<i32, Option<Shape>>, id: i32) -> bool {
    !matches!(lookup(pick, id), Lookup::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip(text: &str, bytes: &[u8]) -> Result<usize> {
        let shape = Shape::parse(text).unwrap();
        let mut input = bytes;
        shape.skip(&mut input).map(<[u8]>::len)
    }

    #[test]
    fn parses_nested_shapes() {
        let s = Shape::parse("seq 3 list str holder seq 2 str opt f32 hs").unwrap();
        assert_eq!(
            s,
            Shape::Seq(vec![
                Shape::List(Box::new(Shape::Str)),
                Shape::Holder(Box::new(Shape::Seq(vec![
                    Shape::Str,
                    Shape::Opt(Box::new(Shape::F32))
                ]))),
                Shape::HolderSet,
            ])
        );
    }

    #[test]
    fn refuses_bad_shapes() {
        for bad in [
            "",
            "seq 2 vi",
            "vi vi",
            "fix 99999",
            "seq 65 vi",
            "frob",
            "list",
        ] {
            assert!(Shape::parse(bad).is_err(), "{bad:?}");
        }
        let deep = "opt ".repeat(40) + "vi";
        assert!(Shape::parse(&deep).is_err());
        let long = "seq 64 ".to_owned() + &"vi ".repeat(2000);
        assert!(Shape::parse(&long).is_err());
    }

    #[test]
    fn skips_primitive_and_composite_values() {
        assert_eq!(skip("vi", &[0x80, 0x01, 9]).unwrap(), 2);
        assert_eq!(skip("str", &[3, b'a', b'b', b'c', 9]).unwrap(), 4);
        assert_eq!(skip("opt i32", &[0, 9]).unwrap(), 1);
        assert_eq!(skip("opt i32", &[1, 0, 0, 0, 7]).unwrap(), 5);
        assert_eq!(
            skip("list seq 2 bool f32", &[2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
            11
        );
        assert_eq!(skip("either vi str", &[0, 1, b'x']).unwrap(), 3);
        assert_eq!(skip("holder vi", &[5]).unwrap(), 1);
        assert_eq!(skip("holder vi", &[0, 5]).unwrap(), 2);
        assert_eq!(skip("hs", &[0, 1, b'x']).unwrap(), 3);
        assert_eq!(skip("hs", &[3, 1, 2]).unwrap(), 3);
        assert_eq!(skip("len", &[2, 1, 2, 3]).unwrap(), 3);
        assert_eq!(skip("nlist 2 i8", &[1, 2, 3]).unwrap(), 2);
        assert_eq!(skip("map str vi", &[1, 1, b'k', 4]).unwrap(), 4);
    }

    #[test]
    fn skips_network_nbt_including_end() {
        assert_eq!(skip("nbt", &[0, 9]).unwrap(), 1);
        assert_eq!(skip("nbt", &[3, 0, 0, 0, 1, 9]).unwrap(), 5);
        assert_eq!(skip("nbt", &[10, 1, 0, 1, b'a', 5, 0, 9]).unwrap(), 7);
    }

    #[test]
    fn modded_types_decode_through_the_installed_table() {
        use crate::types::Particle;
        use crate::types::component::{DataComponent, ModdedComponent};
        use crate::types::entity_data::EntityDataValues;
        use mcr_buf::{from_bytes_exact, to_bytes};

        let mut codecs = ModdedCodecs::default();
        codecs
            .components
            .insert(9001, Some(Shape::parse("seq 2 vi str").unwrap()));
        codecs.components.insert(9002, None);
        codecs
            .particles
            .insert(9003, Some(Shape::parse("f32").unwrap()));
        codecs
            .entity_data
            .insert(9004, Some(Shape::parse("opt vi").unwrap()));
        install(Some(codecs));

        // One added component of type 9001 (7, "ab"), nothing removed.
        let patch = [1, 0, 0xa9, 0x46, 7, 2, b'a', b'b'];
        let decoded: DataComponentPatch = from_bytes_exact(&patch).unwrap();
        assert_eq!(
            decoded.added,
            [DataComponent::Modded(ModdedComponent {
                type_id: 9001,
                bytes: vec![7, 2, b'a', b'b'],
            })]
        );
        assert_eq!(decoded.added[0].name(), "modded");
        assert_eq!(to_bytes(&decoded), patch);
        let no_shape = [1, 0, 0xaa, 0x46, 7];
        assert!(from_bytes_exact::<DataComponentPatch>(&no_shape).is_err());
        let removed = [0, 1, 0xaa, 0x46];
        assert_eq!(
            from_bytes_exact::<DataComponentPatch>(&removed)
                .unwrap()
                .removed,
            [9002]
        );

        let particle = [0xab, 0x46, 0, 0, 0x80, 0x3f];
        let p: Particle = from_bytes_exact(&particle).unwrap();
        assert_eq!(p.type_id(), 9003);
        assert_eq!(to_bytes(&p), particle);

        // Entry 3 of the modded serializer is stepped over, entry 4 (a byte) kept.
        let data = [3, 0xac, 0x46, 1, 5, 4, 0, 9, 0xff];
        let values: EntityDataValues = from_bytes_exact(&data).unwrap();
        assert_eq!(values.entries.len(), 1);
        assert_eq!(values.entries[0].index, 4);
    }

    #[test]
    fn hostile_values_fail_cleanly() {
        assert!(skip("str", &[0xff, 0xff, 0xff, 0xff, 0x0f]).is_err());
        assert!(skip("list unit", &[0xff, 0xff, 0xff, 0xff, 0x07]).is_err());
        assert!(skip("list vi", &[0xff, 0xff, 0x03]).is_err());
        assert!(skip("bytes", &[5, 1]).is_err());
        assert!(skip("seq 2 vi vi", &[1]).is_err());
    }
}
