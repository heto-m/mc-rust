//! Stringified NBT: a formatter for debugging and a parser for the modern (1.21.5+) syntax subset.

use std::fmt::Write;

use crate::{Compound, List, Value};

const MAX_DEPTH: usize = 512;

pub fn to_string(value: &Value) -> String {
    let mut s = String::new();
    write_value(&mut s, value);
    s
}

fn is_bare(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+')
}

fn write_quoted(out: &mut String, s: &str) {
    let q = if s.contains('"') && !s.contains('\'') {
        '\''
    } else {
        '"'
    };
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => {
                let _ = write!(out, "\\x{:02X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(q);
}

fn write_key(out: &mut String, k: &str) {
    if !k.is_empty() && k.chars().all(is_bare) {
        out.push_str(k);
    } else {
        write_quoted(out, k);
    }
}

fn write_seq<T>(out: &mut String, items: &[T], mut f: impl FnMut(&mut String, &T)) {
    for (i, x) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        f(out, x);
    }
}

fn write_value(out: &mut String, v: &Value) {
    match v {
        Value::Byte(x) => {
            let _ = write!(out, "{x}b");
        }
        Value::Short(x) => {
            let _ = write!(out, "{x}s");
        }
        Value::Int(x) => {
            let _ = write!(out, "{x}");
        }
        Value::Long(x) => {
            let _ = write!(out, "{x}L");
        }
        Value::Float(x) => {
            let _ = write!(out, "{x:?}f");
        }
        Value::Double(x) => {
            let _ = write!(out, "{x:?}d");
        }
        Value::ByteArray(a) => {
            out.push_str("[B;");
            write_seq(out, a, |o, x| {
                let _ = write!(o, "{x}b");
            });
            out.push(']');
        }
        Value::IntArray(a) => {
            out.push_str("[I;");
            write_seq(out, a, |o, x| {
                let _ = write!(o, "{x}");
            });
            out.push(']');
        }
        Value::LongArray(a) => {
            out.push_str("[L;");
            write_seq(out, a, |o, x| {
                let _ = write!(o, "{x}L");
            });
            out.push(']');
        }
        Value::String(s) => write_quoted(out, s),
        Value::List(l) => write_list(out, l),
        Value::Compound(c) => write_compound(out, c),
    }
}

fn write_compound(out: &mut String, c: &Compound) {
    out.push('{');
    for (i, (k, v)) in c.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_key(out, k);
        out.push(':');
        write_value(out, v);
    }
    out.push('}');
}

fn write_list(out: &mut String, l: &List) {
    out.push('[');
    match l {
        List::Empty => {}
        List::Byte(v) => write_seq(out, v, |o, x| write_value(o, &Value::Byte(*x))),
        List::Short(v) => write_seq(out, v, |o, x| write_value(o, &Value::Short(*x))),
        List::Int(v) => write_seq(out, v, |o, x| write_value(o, &Value::Int(*x))),
        List::Long(v) => write_seq(out, v, |o, x| write_value(o, &Value::Long(*x))),
        List::Float(v) => write_seq(out, v, |o, x| write_value(o, &Value::Float(*x))),
        List::Double(v) => write_seq(out, v, |o, x| write_value(o, &Value::Double(*x))),
        List::ByteArray(v) => {
            write_seq(out, v, |o, x| write_value(o, &Value::ByteArray(x.clone())))
        }
        List::String(v) => write_seq(out, v, |o, x| write_quoted(o, x)),
        List::List(v) => write_seq(out, v, write_list),
        List::Compound(v) => write_seq(out, v, write_compound),
        List::IntArray(v) => write_seq(out, v, |o, x| write_value(o, &Value::IntArray(x.clone()))),
        List::LongArray(v) => {
            write_seq(out, v, |o, x| write_value(o, &Value::LongArray(x.clone())))
        }
    }
    out.push(']');
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("snbt error at byte {pos}: {msg}")]
pub struct SnbtError {
    pub pos: usize,
    pub msg: &'static str,
}

type PResult<T> = Result<T, SnbtError>;

pub fn parse(text: &str) -> PResult<Value> {
    let mut p = Parser { s: text, pos: 0 };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.pos != text.len() {
        return Err(p.err("trailing characters"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn err(&self, msg: &'static str) -> SnbtError {
        SnbtError { pos: self.pos, msg }
    }

    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.ws();
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: char, msg: &'static str) -> PResult<()> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(self.err(msg))
        }
    }

    fn value(&mut self, depth: usize) -> PResult<Value> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        self.ws();
        match self.peek() {
            Some('{') => self.compound(depth).map(Value::Compound),
            Some('[') => self.list_or_array(depth),
            Some('"' | '\'') => self.quoted().map(Value::String),
            Some(c) if is_bare(c) => {
                let tok = self.bare();
                Ok(classify(tok))
            }
            _ => Err(self.err("expected value")),
        }
    }

    fn bare(&mut self) -> &str {
        let start = self.pos;
        while self.peek().is_some_and(is_bare) {
            self.bump();
        }
        &self.s[start..self.pos]
    }

    fn key(&mut self) -> PResult<String> {
        self.ws();
        match self.peek() {
            Some('"' | '\'') => self.quoted(),
            Some(c) if is_bare(c) => Ok(self.bare().to_owned()),
            _ => Err(self.err("expected key")),
        }
    }

    fn compound(&mut self, depth: usize) -> PResult<Compound> {
        self.expect('{', "expected '{'")?;
        let mut c = Compound::new();
        if self.eat('}') {
            return Ok(c);
        }
        loop {
            let k = self.key()?;
            self.expect(':', "expected ':'")?;
            let v = self.value(depth + 1)?;
            c.insert(k, v);
            if self.eat('}') {
                return Ok(c);
            }
            self.expect(',', "expected ',' or '}'")?;
        }
    }

    fn list_or_array(&mut self, depth: usize) -> PResult<Value> {
        self.expect('[', "expected '['")?;
        let rest = &self.s[self.pos..];
        let kind = rest.as_bytes();
        if kind.len() >= 2 && kind[1] == b';' && matches!(kind[0] | 0x20, b'b' | b'i' | b'l') {
            let kind = kind[0] | 0x20;
            self.pos += 2;
            return self.array(kind, depth);
        }
        let mut items = Vec::new();
        if !self.eat(']') {
            loop {
                items.push(self.value(depth + 1)?);
                if self.eat(']') {
                    break;
                }
                self.expect(',', "expected ',' or ']'")?;
            }
        }
        Ok(Value::List(build_list(items)))
    }

    fn array(&mut self, kind: u8, depth: usize) -> PResult<Value> {
        let mut ints: Vec<i64> = Vec::new();
        if !self.eat(']') {
            loop {
                self.ws();
                let at = self.pos;
                let n = match self.value(depth + 1)? {
                    Value::Byte(x) => i64::from(x),
                    Value::Short(x) => i64::from(x),
                    Value::Int(x) => i64::from(x),
                    Value::Long(x) => x,
                    _ => {
                        self.pos = at;
                        return Err(self.err("expected integer"));
                    }
                };
                ints.push(n);
                if self.eat(']') {
                    break;
                }
                self.expect(',', "expected ',' or ']'")?;
            }
        }
        let bad = |p: &Self| p.err("array element out of range");
        Ok(match kind {
            b'b' => Value::ByteArray(
                ints.iter()
                    .map(|&n| i8::try_from(n))
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad(self))?,
            ),
            b'i' => Value::IntArray(
                ints.iter()
                    .map(|&n| i32::try_from(n))
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad(self))?,
            ),
            _ => Value::LongArray(ints),
        })
    }

    fn quoted(&mut self) -> PResult<String> {
        let q = self.bump().unwrap();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err("unterminated string")),
                Some(c) if c == q => return Ok(out),
                Some('\\') => out.push(self.escape()?),
                Some(c) => out.push(c),
            }
        }
    }

    fn hex(&mut self, digits: usize) -> PResult<char> {
        let end = self.pos + digits;
        let h = self
            .s
            .get(self.pos..end)
            .ok_or_else(|| self.err("bad escape"))?;
        let n = u32::from_str_radix(h, 16).map_err(|_| self.err("bad escape"))?;
        self.pos = end;
        char::from_u32(n).ok_or_else(|| self.err("bad escape"))
    }

    fn escape(&mut self) -> PResult<char> {
        Ok(match self.bump().ok_or_else(|| self.err("bad escape"))? {
            c @ ('\\' | '"' | '\'') => c,
            'b' => '\u{8}',
            'f' => '\u{c}',
            'n' => '\n',
            'r' => '\r',
            's' => ' ',
            't' => '\t',
            'x' => self.hex(2)?,
            'u' => self.hex(4)?,
            'U' => self.hex(8)?,
            _ => return Err(self.err("bad escape")),
        })
    }
}

fn build_list(items: Vec<Value>) -> List {
    let Some(first) = items.first().map(Value::tag) else {
        return List::Empty;
    };
    if items.iter().any(|v| v.tag() != first) {
        let wrapped = items
            .into_iter()
            .map(|v| match v {
                Value::Compound(c) => c,
                v => [("", v)].into_iter().collect(),
            })
            .collect();
        return List::Compound(wrapped);
    }
    macro_rules! collect {
        ($variant:ident) => {
            List::$variant(
                items
                    .into_iter()
                    .map(|v| match v {
                        Value::$variant(x) => x,
                        _ => unreachable!(),
                    })
                    .collect(),
            )
        };
    }
    match items[0] {
        Value::Byte(_) => collect!(Byte),
        Value::Short(_) => collect!(Short),
        Value::Int(_) => collect!(Int),
        Value::Long(_) => collect!(Long),
        Value::Float(_) => collect!(Float),
        Value::Double(_) => collect!(Double),
        Value::ByteArray(_) => List::ByteArray(
            items
                .into_iter()
                .filter_map(|v| match v {
                    Value::ByteArray(x) => Some(x),
                    _ => None,
                })
                .collect(),
        ),
        Value::String(_) => collect!(String),
        Value::Compound(_) => collect!(Compound),
        Value::IntArray(_) => collect!(IntArray),
        Value::LongArray(_) => collect!(LongArray),
        Value::List(_) => collect!(List),
    }
}

fn classify(tok: &str) -> Value {
    match tok {
        "true" => return Value::Byte(1),
        "false" => return Value::Byte(0),
        _ => {}
    }
    number(tok).unwrap_or_else(|| Value::String(tok.to_owned()))
}

fn number(tok: &str) -> Option<Value> {
    let (neg, body) = match tok.as_bytes().first()? {
        b'-' => (true, &tok[1..]),
        b'+' => (false, &tok[1..]),
        _ => (false, tok),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') {
        return None;
    }
    let lower = body.to_ascii_lowercase();
    let (radix, digits) = if let Some(h) = lower.strip_prefix("0x") {
        (16, h)
    } else if let Some(b) = lower
        .strip_prefix("0b")
        .filter(|b| b.starts_with(['0', '1']))
    {
        (2, b)
    } else {
        (10, lower.as_str())
    };
    let clean = |s: &str| s.replace('_', "");
    if radix != 10 {
        let (digits, suffix) = match digits.as_bytes().last()? {
            b's' | b'l' if digits.len() > 1 => (
                &digits[..digits.len() - 1],
                digits.as_bytes()[digits.len() - 1],
            ),
            _ => (digits, 0),
        };
        let n = i64::from_str_radix(&clean(digits), radix).ok()?;
        return int_of(if neg { -n } else { n }, suffix);
    }
    let last = *digits.as_bytes().last()?;
    let (core, suffix) = match last {
        b'b' | b's' | b'l' | b'f' | b'd' => (&digits[..digits.len() - 1], last),
        _ => (digits, 0),
    };
    if core.is_empty() {
        return None;
    }
    let core = clean(core);
    if core.bytes().all(|b| b.is_ascii_digit()) {
        if matches!(suffix, b'f' | b'd') {
            let f: f64 = core.parse().ok()?;
            let f = if neg { -f } else { f };
            return Some(if suffix == b'f' {
                Value::Float(f as f32)
            } else {
                Value::Double(f)
            });
        }
        let n: i64 = if neg {
            format!("-{core}").parse().ok()?
        } else {
            core.parse().ok()?
        };
        return int_of(n, suffix);
    }
    if suffix == b'b' || suffix == b's' || suffix == b'l' {
        return None;
    }
    if !core
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'+' | b'-'))
    {
        return None;
    }
    if !core.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    if suffix == b'f' {
        let f: f32 = core.parse().ok()?;
        Some(Value::Float(if neg { -f } else { f }))
    } else {
        let f: f64 = core.parse().ok()?;
        Some(Value::Double(if neg { -f } else { f }))
    }
}

fn int_of(n: i64, suffix: u8) -> Option<Value> {
    match suffix {
        b'b' => i8::try_from(n).ok().map(Value::Byte),
        b's' => i16::try_from(n).ok().map(Value::Short),
        b'l' => Some(Value::Long(n)),
        _ => i32::try_from(n).ok().map(Value::Int),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars() {
        let v = parse(
            "{a:1b,b:2s,c:3,d:4L,e:1.5f,f:2.5,g:2.5d,h:true,i:\"x y\",j:foo,k:-7,l:0x1F,m:1e3}",
        )
        .unwrap();
        let c = v.as_compound().unwrap();
        assert_eq!(c.get_i8("a"), Some(1));
        assert_eq!(c.get_i16("b"), Some(2));
        assert_eq!(c.get_i32("c"), Some(3));
        assert_eq!(c.get_i64("d"), Some(4));
        assert_eq!(c.get_f32("e"), Some(1.5));
        assert_eq!(c.get_f64("f"), Some(2.5));
        assert_eq!(c.get_f64("g"), Some(2.5));
        assert_eq!(c.get_bool("h"), Some(true));
        assert_eq!(c.get_str("i"), Some("x y"));
        assert_eq!(c.get_str("j"), Some("foo"));
        assert_eq!(c.get_i32("k"), Some(-7));
        assert_eq!(c.get_i32("l"), Some(31));
        assert_eq!(c.get_f64("m"), Some(1000.0));
    }

    #[test]
    fn parses_containers() {
        let v = parse("{l:[1,2,3],e:[],a:[B;1b,2b],i:[I;1,2],q:[L;1L],c:[{x:1},{y:2}],m:[1,{}]}")
            .unwrap();
        let c = v.as_compound().unwrap();
        assert_eq!(c.get_list("l").unwrap().as_ints(), [1, 2, 3]);
        assert!(c.get_list("e").unwrap().is_empty());
        assert_eq!(c.get_byte_array("a"), Some(&[1i8, 2][..]));
        assert_eq!(c.get_int_array("i"), Some(&[1, 2][..]));
        assert_eq!(c.get_long_array("q"), Some(&[1i64][..]));
        assert_eq!(c.get_list("c").unwrap().as_compounds().len(), 2);
        assert_eq!(c.get_list("m").unwrap().as_compounds().len(), 2);
    }

    #[test]
    fn round_trips_formatter_output() {
        let mut c = Compound::new();
        c.insert("quote\"key", "it's \"q\"\n\t\\ \u{1F600} \u{1}");
        c.insert("f", 0.1f32);
        c.insert("d", -1e300f64);
        c.insert("big", i64::MIN);
        c.insert("s", i16::MIN);
        c.insert("nested", List::List(vec![List::Int(vec![1]), List::Empty]));
        c.insert("ba", Value::ByteArray(vec![-1, 2]));
        c.insert("strs", List::String(vec!["a".into(), "b c".into()]));
        let v = Value::Compound(c);
        assert_eq!(parse(&v.to_snbt()).unwrap(), v);
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for s in [
            "",
            "{",
            "{a}",
            "{a:}",
            "[1,",
            "\"abc",
            "{a:1,}",
            "[B;1,x]",
            "{a:1} x",
            "\"\\q\"",
            "[I;99999999999]",
        ] {
            assert!(parse(s).is_err(), "{s}");
        }
        assert!(parse(&"[".repeat(100_000)).is_err());
    }

    #[test]
    #[ignore = "needs the generated vanilla data dir"]
    fn parses_vanilla_data_dir() {
        let dir = std::env::var("MCR_SNBT_DIR").unwrap_or_else(|_| {
            "/home/heto/Development/mc-rust/.cache/versions/26.3/generated/data".into()
        });
        if !std::path::Path::new(&dir).exists() {
            return;
        }
        let mut n = 0;
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(p) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&p) else {
                continue;
            };
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x == "snbt") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                    n += 1;
                }
            }
        }
        eprintln!("parsed {n} snbt files");
    }
}
