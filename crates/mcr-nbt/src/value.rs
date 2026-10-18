use std::fmt;

use crate::{Compound, tag};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(List),
    Compound(Compound),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

/// Homogeneous list; the element type is the variant, `Empty` has none.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum List {
    #[default]
    Empty,
    Byte(Vec<i8>),
    Short(Vec<i16>),
    Int(Vec<i32>),
    Long(Vec<i64>),
    Float(Vec<f32>),
    Double(Vec<f64>),
    ByteArray(Vec<Vec<i8>>),
    String(Vec<String>),
    List(Vec<List>),
    Compound(Vec<Compound>),
    IntArray(Vec<Vec<i32>>),
    LongArray(Vec<Vec<i64>>),
}

impl List {
    pub fn len(&self) -> usize {
        match self {
            List::Empty => 0,
            List::Byte(v) => v.len(),
            List::Short(v) => v.len(),
            List::Int(v) => v.len(),
            List::Long(v) => v.len(),
            List::Float(v) => v.len(),
            List::Double(v) => v.len(),
            List::ByteArray(v) => v.len(),
            List::String(v) => v.len(),
            List::List(v) => v.len(),
            List::Compound(v) => v.len(),
            List::IntArray(v) => v.len(),
            List::LongArray(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn element_tag(&self) -> u8 {
        match self {
            List::Empty => tag::END,
            List::Byte(_) => tag::BYTE,
            List::Short(_) => tag::SHORT,
            List::Int(_) => tag::INT,
            List::Long(_) => tag::LONG,
            List::Float(_) => tag::FLOAT,
            List::Double(_) => tag::DOUBLE,
            List::ByteArray(_) => tag::BYTE_ARRAY,
            List::String(_) => tag::STRING,
            List::List(_) => tag::LIST,
            List::Compound(_) => tag::COMPOUND,
            List::IntArray(_) => tag::INT_ARRAY,
            List::LongArray(_) => tag::LONG_ARRAY,
        }
    }

    /// A typed list when every value has one tag, else each value wrapped in a compound under `""`
    /// (vanilla's heterogeneous list form).
    pub fn from_values(values: Vec<Value>) -> List {
        let Some(first) = values.first() else {
            return List::Empty;
        };
        let tag = first.tag();
        if values.iter().any(|v| v.tag() != tag) {
            return List::Compound(
                values
                    .into_iter()
                    .map(|v| {
                        let mut wrapper = Compound::new();
                        wrapper.insert("", v);
                        wrapper
                    })
                    .collect(),
            );
        }
        macro_rules! typed {
            ($variant:ident) => {
                List::$variant(
                    values
                        .into_iter()
                        .filter_map(|v| match v {
                            Value::$variant(x) => Some(x),
                            _ => None,
                        })
                        .collect(),
                )
            };
        }
        match first {
            Value::Byte(_) => typed!(Byte),
            Value::Short(_) => typed!(Short),
            Value::Int(_) => typed!(Int),
            Value::Long(_) => typed!(Long),
            Value::Float(_) => typed!(Float),
            Value::Double(_) => typed!(Double),
            Value::ByteArray(_) => typed!(ByteArray),
            Value::String(_) => typed!(String),
            Value::List(_) => typed!(List),
            Value::Compound(_) => typed!(Compound),
            Value::IntArray(_) => typed!(IntArray),
            Value::LongArray(_) => typed!(LongArray),
        }
    }

    pub fn as_compounds(&self) -> &[Compound] {
        match self {
            List::Compound(v) => v,
            _ => &[],
        }
    }

    pub fn as_strings(&self) -> &[String] {
        match self {
            List::String(v) => v,
            _ => &[],
        }
    }

    pub fn as_ints(&self) -> &[i32] {
        match self {
            List::Int(v) => v,
            _ => &[],
        }
    }
}

impl List {
    pub fn sort_vanilla_order(&mut self) {
        match self {
            List::List(lists) => lists.iter_mut().for_each(List::sort_vanilla_order),
            List::Compound(compounds) => compounds.iter_mut().for_each(|c| {
                c.values_mut().for_each(Value::sort_vanilla_order);
                c.sort_vanilla_order();
            }),
            _ => {}
        }
    }
}

impl Value {
    /// Applies [`Compound::sort_vanilla_order`] to every compound in the tree.
    pub fn sort_vanilla_order(&mut self) {
        match self {
            Value::Compound(c) => {
                c.values_mut().for_each(Value::sort_vanilla_order);
                c.sort_vanilla_order();
            }
            Value::List(list) => list.sort_vanilla_order(),
            _ => {}
        }
    }

    pub fn tag(&self) -> u8 {
        match self {
            Value::Byte(_) => tag::BYTE,
            Value::Short(_) => tag::SHORT,
            Value::Int(_) => tag::INT,
            Value::Long(_) => tag::LONG,
            Value::Float(_) => tag::FLOAT,
            Value::Double(_) => tag::DOUBLE,
            Value::ByteArray(_) => tag::BYTE_ARRAY,
            Value::String(_) => tag::STRING,
            Value::List(_) => tag::LIST,
            Value::Compound(_) => tag::COMPOUND,
            Value::IntArray(_) => tag::INT_ARRAY,
            Value::LongArray(_) => tag::LONG_ARRAY,
        }
    }

    /// Any numeric tag, as vanilla's `NumericTag` reads it.
    fn number(&self) -> Option<Number> {
        Some(match *self {
            Value::Byte(v) => Number::Int(v.into()),
            Value::Short(v) => Number::Int(v.into()),
            Value::Int(v) => Number::Int(v.into()),
            Value::Long(v) => Number::Int(v),
            Value::Float(v) => Number::Float(v.into()),
            Value::Double(v) => Number::Float(v),
            _ => return None,
        })
    }

    pub fn as_i8(&self) -> Option<i8> {
        self.as_i64().map(|v| v as i8)
    }

    pub fn as_i16(&self) -> Option<i16> {
        self.as_i64().map(|v| v as i16)
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self.number()? {
            Number::Int(v) => Some(v as i32),
            Number::Float(v) => Some(v as i32),
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self.number()? {
            Number::Int(v) => Some(v),
            Number::Float(v) => Some(v as i64),
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|v| v as f32)
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self.number()? {
            Number::Int(v) => Some(v as f64),
            Number::Float(v) => Some(v),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&Compound> {
        match self {
            Value::Compound(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&List> {
        match self {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_byte_array(&self) -> Option<&[i8]> {
        match self {
            Value::ByteArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_int_array(&self) -> Option<&[i32]> {
        match self {
            Value::IntArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_long_array(&self) -> Option<&[i64]> {
        match self {
            Value::LongArray(v) => Some(v),
            _ => None,
        }
    }

    pub fn into_compound(self) -> Option<Compound> {
        match self {
            Value::Compound(v) => Some(v),
            _ => None,
        }
    }

    pub fn to_snbt(&self) -> String {
        crate::snbt::to_string(self)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_snbt())
    }
}

macro_rules! from_impl {
    ($($t:ty => $v:ident),*) => {$(
        impl From<$t> for Value {
            fn from(x: $t) -> Self {
                Value::$v(x)
            }
        }
    )*};
}

from_impl!(i8 => Byte, i16 => Short, i32 => Int, i64 => Long, f32 => Float, f64 => Double,
    String => String, Compound => Compound, List => List);

impl From<bool> for Value {
    fn from(x: bool) -> Self {
        Value::Byte(i8::from(x))
    }
}

impl From<&str> for Value {
    fn from(x: &str) -> Self {
        Value::String(x.to_owned())
    }
}

impl From<Vec<i32>> for Value {
    fn from(x: Vec<i32>) -> Self {
        Value::IntArray(x)
    }
}

impl From<Vec<i64>> for Value {
    fn from(x: Vec<i64>) -> Self {
        Value::LongArray(x)
    }
}
