//! SNBT text of a tag, which is how block predicates carry their `nbt` field.

use std::fmt::Write;

use mcr_nbt::{Compound, List, Value};

const MAX_DEPTH: usize = 64;

pub fn compound(c: &Compound) -> String {
    let mut out = String::new();
    write_compound(&mut out, c, 0);
    out
}

fn write_value(out: &mut String, v: &Value, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match v {
        Value::Byte(v) => _ = write!(out, "{v}b"),
        Value::Short(v) => _ = write!(out, "{v}s"),
        Value::Int(v) => _ = write!(out, "{v}"),
        Value::Long(v) => _ = write!(out, "{v}L"),
        Value::Float(v) => _ = write!(out, "{}f", java_float(f64::from(*v), f64::from(*v), true)),
        Value::Double(v) => _ = write!(out, "{}d", java_float(*v, *v, false)),
        Value::String(s) => quote(out, s),
        Value::ByteArray(a) => {
            out.push_str("[B;");
            join(out, a.iter(), |o, b| _ = write!(o, "{b}B"));
            out.push(']');
        }
        Value::IntArray(a) => {
            out.push_str("[I;");
            join(out, a.iter(), |o, b| _ = write!(o, "{b}"));
            out.push(']');
        }
        Value::LongArray(a) => {
            out.push_str("[L;");
            join(out, a.iter(), |o, b| _ = write!(o, "{b}L"));
            out.push(']');
        }
        Value::List(l) => write_list(out, l, depth),
        Value::Compound(c) => write_compound(out, c, depth),
    }
}

fn join<T>(out: &mut String, items: impl Iterator<Item = T>, mut f: impl FnMut(&mut String, T)) {
    for (i, item) in items.enumerate() {
        if i != 0 {
            out.push(',');
        }
        f(out, item);
    }
}

fn write_list(out: &mut String, l: &List, depth: usize) {
    out.push('[');
    match l {
        List::Empty => {}
        List::Byte(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::Byte(*x), depth + 1)
        }),
        List::Short(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::Short(*x), depth + 1)
        }),
        List::Int(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::Int(*x), depth + 1)
        }),
        List::Long(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::Long(*x), depth + 1)
        }),
        List::Float(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::Float(*x), depth + 1)
        }),
        List::Double(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::Double(*x), depth + 1)
        }),
        List::String(v) => join(out, v.iter(), |o, x| quote(o, x)),
        List::ByteArray(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::ByteArray(x.clone()), depth + 1);
        }),
        List::IntArray(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::IntArray(x.clone()), depth + 1);
        }),
        List::LongArray(v) => join(out, v.iter(), |o, x| {
            write_value(o, &Value::LongArray(x.clone()), depth + 1);
        }),
        List::List(v) => join(out, v.iter(), |o, x| write_list(o, x, depth + 1)),
        List::Compound(v) => join(out, v.iter(), |o, x| write_compound(o, x, depth + 1)),
    }
    out.push(']');
}

fn write_compound(out: &mut String, c: &Compound, depth: usize) {
    out.push('{');
    let mut entries: Vec<_> = c.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (i, (k, v)) in entries.into_iter().enumerate() {
        if i != 0 {
            out.push(',');
        }
        if unquoted_key(k) {
            out.push_str(k);
        } else {
            quote(out, k);
        }
        out.push(':');
        write_value(out, v, depth + 1);
    }
    out.push('}');
}

fn unquoted_key(k: &str) -> bool {
    let mut chars = k.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '.' || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        && !k.eq_ignore_ascii_case("true")
        && !k.eq_ignore_ascii_case("false")
}

fn quote(out: &mut String, s: &str) {
    let mut body = String::new();
    let mut quote = None;
    for c in s.chars() {
        match c {
            '\\' => body.push_str("\\\\"),
            '"' | '\'' => {
                let q = *quote.get_or_insert(if c == '"' { '\'' } else { '"' });
                if q == c {
                    body.push('\\');
                }
                body.push(c);
            }
            '\u{8}' => body.push_str("\\b"),
            '\t' => body.push_str("\\t"),
            '\n' => body.push_str("\\n"),
            '\u{c}' => body.push_str("\\f"),
            '\r' => body.push_str("\\r"),
            c if c < ' ' => _ = write!(body, "\\x{:02x}", c as u32),
            c => body.push(c),
        }
    }
    let q = quote.unwrap_or('"');
    out.push(q);
    out.push_str(&body);
    out.push(q);
}

/// Java's `Float.toString` / `Double.toString` shape: plain decimals between 1e-3 and 1e7, else scientific.
fn java_float(v: f64, shortest_source: f64, single: bool) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let digits = if single {
        format!("{:e}", shortest_source as f32)
    } else {
        format!("{v:e}")
    };
    let abs = v.abs();
    if abs == 0.0 || (1e-3..1e7).contains(&abs) {
        let plain = if single {
            format!("{}", shortest_source as f32)
        } else {
            format!("{v}")
        };
        return if plain.contains('.') {
            plain
        } else {
            format!("{plain}.0")
        };
    }
    let (mantissa, exp) = digits.split_once('e').unwrap_or((&digits, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_owned()
    } else {
        format!("{mantissa}.0")
    };
    format!("{mantissa}E{exp}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_java() {
        let mut c = Compound::new();
        c.insert("b", Value::Byte(1));
        c.insert("f", Value::Float(1.5));
        c.insert("s", Value::String("it's".into()));
        c.insert("Key Z", Value::Int(3));
        assert_eq!(compound(&c), r#"{"Key Z":3,b:1b,f:1.5f,s:"it's"}"#);
    }
}
