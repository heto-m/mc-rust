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

impl Value {
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

    pub fn as_i8(&self) -> Option<i8> {
        match *self {
            Value::Byte(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_i16(&self) -> Option<i16> {
        match *self {
            Value::Short(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match *self {
            Value::Int(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match *self {
            Value::Long(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Value::Float(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::Double(v) => Some(v),
            _ => None,
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
