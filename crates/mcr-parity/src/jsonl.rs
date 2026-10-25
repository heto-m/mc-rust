//! Case lines are `{"id": n, "case": {...}}`; result lines are `{"id": n, "out": {...}}`.

use std::collections::BTreeMap;
use std::io::{BufRead, BufWriter, Write};
use std::path::Path;

use serde_json::{Value, json};

use crate::{ParityError, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub id: u32,
    pub case: Value,
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> ParityError + '_ {
    move |source| ParityError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn write_lines(path: &Path, lines: impl Iterator<Item = Value>) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let file = std::fs::File::create(path).map_err(io_err(path))?;
    let mut out = BufWriter::new(file);
    for line in lines {
        serde_json::to_writer(&mut out, &line).map_err(|source| ParityError::Json {
            path: path.display().to_string(),
            line: 0,
            source,
        })?;
        out.write_all(b"\n").map_err(io_err(path))?;
    }
    out.flush().map_err(io_err(path))
}

fn read_lines(path: &Path) -> Result<Vec<Value>> {
    let file = std::fs::File::open(path).map_err(io_err(path))?;
    let mut values = Vec::new();
    for (n, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.map_err(io_err(path))?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(
            serde_json::from_str(&line).map_err(|source| ParityError::Json {
                path: path.display().to_string(),
                line: n + 1,
                source,
            })?,
        );
    }
    Ok(values)
}

fn id_of(path: &Path, v: &Value) -> Result<u32> {
    v.get("id")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
        .ok_or_else(|| ParityError::Invalid(format!("{}: line without an id", path.display())))
}

pub fn write_cases(path: &Path, cases: &[Case]) -> Result<()> {
    write_lines(
        path,
        cases.iter().map(|c| json!({"id": c.id, "case": c.case})),
    )
}

pub fn read_cases(path: &Path) -> Result<Vec<Case>> {
    read_lines(path)?
        .into_iter()
        .map(|mut v| {
            Ok(Case {
                id: id_of(path, &v)?,
                case: v.get_mut("case").map(Value::take).unwrap_or(Value::Null),
            })
        })
        .collect()
}

pub fn write_outputs(path: &Path, cases: &[Case], outs: Vec<Value>) -> Result<()> {
    write_lines(
        path,
        cases
            .iter()
            .zip(outs)
            .map(|(c, out)| json!({"id": c.id, "out": out})),
    )
}

/// Outputs by id; a repeated id keeps the first.
pub fn read_outputs(path: &Path) -> Result<BTreeMap<u32, Value>> {
    let mut map = BTreeMap::new();
    for mut v in read_lines(path)? {
        let id = id_of(path, &v)?;
        let out = v.get_mut("out").map(Value::take).unwrap_or(Value::Null);
        map.entry(id).or_insert(out);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_and_outputs_round_trip() {
        let dir = std::env::temp_dir().join(format!("mcr-parity-jsonl-{}", std::process::id()));
        let cases = vec![
            Case {
                id: 0,
                case: json!({"text": "a\nb"}),
            },
            Case {
                id: 1,
                case: json!({"text": "§l😀"}),
            },
        ];
        let path = dir.join("cases.jsonl");
        write_cases(&path, &cases).unwrap();
        assert_eq!(read_cases(&path).unwrap(), cases);
        let out = dir.join("out.jsonl");
        write_outputs(&out, &cases, vec![json!(1), json!({"w": 2.5})]).unwrap();
        let map = read_outputs(&out).unwrap();
        assert_eq!(map[&1], json!({"w": 2.5}));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
