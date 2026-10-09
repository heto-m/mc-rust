use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackFormat {
    pub major: u32,
    pub minor: u32,
}

impl PackFormat {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub const MIN: Self = Self::new(0, 0);
    pub const MAX: Self = Self::new(u32::MAX, u32::MAX);
    pub const CURRENT: Self = Self::new(97, 1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatRange {
    pub min: PackFormat,
    pub max: PackFormat,
}

impl FormatRange {
    pub const ANY: Self = Self {
        min: PackFormat::MIN,
        max: PackFormat::MAX,
    };

    pub fn contains(&self, format: PackFormat) -> bool {
        self.min <= format && format <= self.max
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub directory: String,
    pub formats: FormatRange,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackMeta {
    pub description: String,
    pub pack_format: Option<PackFormat>,
    pub supported: Option<FormatRange>,
    pub overlays: Vec<Overlay>,
}

#[derive(Deserialize)]
struct RawMeta {
    pack: Option<Value>,
    overlays: Option<RawOverlays>,
}

#[derive(Deserialize)]
struct RawOverlays {
    #[serde(default)]
    entries: Vec<Value>,
}

fn format_lower(v: &Value) -> Option<PackFormat> {
    match v {
        Value::Number(n) => Some(PackFormat::new(n.as_u64()? as u32, 0)),
        Value::Array(a) => Some(PackFormat::new(
            a.first()?.as_u64()? as u32,
            a.get(1).and_then(Value::as_u64).unwrap_or(0) as u32,
        )),
        _ => None,
    }
}

fn format_upper(v: &Value) -> Option<PackFormat> {
    match v {
        Value::Number(n) => Some(PackFormat::new(n.as_u64()? as u32, u32::MAX)),
        Value::Array(a) => Some(PackFormat::new(
            a.first()?.as_u64()? as u32,
            a.get(1)
                .and_then(Value::as_u64)
                .map_or(u32::MAX, |m| m as u32),
        )),
        _ => None,
    }
}

fn range_from(obj: &Value) -> Option<FormatRange> {
    let min = obj.get("min_format").and_then(format_lower);
    let max = obj.get("max_format").and_then(format_upper);
    if min.is_some() || max.is_some() {
        return Some(FormatRange {
            min: min.unwrap_or(PackFormat::MIN),
            max: max.unwrap_or(PackFormat::MAX),
        });
    }
    let legacy = obj
        .get("formats")
        .or_else(|| obj.get("supported_formats"))?;
    match legacy {
        Value::Number(_) => Some(FormatRange {
            min: format_lower(legacy)?,
            max: format_upper(legacy)?,
        }),
        Value::Array(a) => Some(FormatRange {
            min: format_lower(a.first()?)?,
            max: format_upper(a.get(1)?)?,
        }),
        Value::Object(_) => Some(FormatRange {
            min: legacy.get("min_inclusive").and_then(format_lower)?,
            max: legacy.get("max_inclusive").and_then(format_upper)?,
        }),
        _ => None,
    }
}

impl PackMeta {
    pub fn parse(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let raw: RawMeta = serde_json::from_slice(bytes)?;
        let mut meta = Self::default();
        if let Some(pack) = &raw.pack {
            meta.description = match pack.get("description") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            meta.pack_format = pack
                .get("pack_format")
                .and_then(format_lower)
                .or_else(|| pack.get("min_format").and_then(format_lower));
            meta.supported = range_from(pack);
        }
        if let Some(overlays) = raw.overlays {
            meta.overlays = overlays
                .entries
                .iter()
                .filter_map(|e| {
                    Some(Overlay {
                        directory: e.get("directory")?.as_str()?.to_owned(),
                        formats: range_from(e).unwrap_or(FormatRange::ANY),
                    })
                })
                .collect();
        }
        Ok(meta)
    }

    pub fn active_overlays(&self, target: PackFormat) -> impl Iterator<Item = &Overlay> {
        self.overlays
            .iter()
            .filter(move |o| o.formats.contains(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_and_new_fields() {
        let meta = PackMeta::parse(
            br#"{"pack":{"pack_format":34,"description":"hi","supported_formats":[30,40]},
                 "overlays":{"entries":[
                    {"formats":[35,36],"directory":"a"},
                    {"min_format":[97,0],"max_format":[97,5],"directory":"b"}]}}"#,
        )
        .unwrap();
        assert_eq!(meta.description, "hi");
        assert_eq!(meta.pack_format, Some(PackFormat::new(34, 0)));
        assert!(meta.supported.unwrap().contains(PackFormat::new(35, 3)));
        let active: Vec<_> = meta
            .active_overlays(PackFormat::CURRENT)
            .map(|o| o.directory.as_str())
            .collect();
        assert_eq!(active, ["b"]);
    }
}
