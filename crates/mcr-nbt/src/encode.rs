use crate::{Compound, List, Value, mutf8, tag};

/// Over-long strings degrade to empty like vanilla's `StringFallbackDataOutput`.
pub(crate) fn write_name(out: &mut Vec<u8>, s: &str) {
    let len = mutf8::encoded_len(s);
    if len > usize::from(u16::MAX) {
        out.extend_from_slice(&0u16.to_be_bytes());
        return;
    }
    out.extend_from_slice(&(len as u16).to_be_bytes());
    mutf8::encode_into(s, out);
}

fn write_len(out: &mut Vec<u8>, len: usize) {
    out.extend_from_slice(&(len as i32).to_be_bytes());
}

macro_rules! put_be {
    ($out:expr, $slice:expr) => {{
        for x in $slice {
            $out.extend_from_slice(&x.to_be_bytes());
        }
    }};
}

pub(crate) fn write_value(out: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Byte(x) => out.push(*x as u8),
        Value::Short(x) => out.extend_from_slice(&x.to_be_bytes()),
        Value::Int(x) => out.extend_from_slice(&x.to_be_bytes()),
        Value::Long(x) => out.extend_from_slice(&x.to_be_bytes()),
        Value::Float(x) => out.extend_from_slice(&x.to_be_bytes()),
        Value::Double(x) => out.extend_from_slice(&x.to_be_bytes()),
        Value::ByteArray(a) => write_bytes(out, a),
        Value::String(s) => write_name(out, s),
        Value::List(l) => write_list(out, l),
        Value::Compound(c) => write_compound(out, c),
        Value::IntArray(a) => {
            write_len(out, a.len());
            put_be!(out, a);
        }
        Value::LongArray(a) => {
            write_len(out, a.len());
            put_be!(out, a);
        }
    }
}

fn write_bytes(out: &mut Vec<u8>, a: &[i8]) {
    write_len(out, a.len());
    out.extend(a.iter().map(|&b| b as u8));
}

pub(crate) fn write_compound(out: &mut Vec<u8>, c: &Compound) {
    for (k, v) in c.iter() {
        out.push(v.tag());
        write_name(out, k);
        write_value(out, v);
    }
    out.push(tag::END);
}

fn write_list(out: &mut Vec<u8>, l: &List) {
    out.push(l.element_tag());
    write_len(out, l.len());
    match l {
        List::Empty => {}
        List::Byte(v) => out.extend(v.iter().map(|&b| b as u8)),
        List::Short(v) => put_be!(out, v),
        List::Int(v) => put_be!(out, v),
        List::Long(v) => put_be!(out, v),
        List::Float(v) => put_be!(out, v),
        List::Double(v) => put_be!(out, v),
        List::ByteArray(v) => v.iter().for_each(|a| write_bytes(out, a)),
        List::String(v) => v.iter().for_each(|s| write_name(out, s)),
        List::List(v) => v.iter().for_each(|l| write_list(out, l)),
        List::Compound(v) => v.iter().for_each(|c| write_compound(out, c)),
        List::IntArray(v) => v.iter().for_each(|a| {
            write_len(out, a.len());
            put_be!(out, a);
        }),
        List::LongArray(v) => v.iter().for_each(|a| {
            write_len(out, a.len());
            put_be!(out, a);
        }),
    }
}
