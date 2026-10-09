use std::fmt;

use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};
use serde_json::Value;

use crate::error::{AssetError, Result};
use crate::identifier::Identifier;

pub type Properties<'a> = &'a [(&'a str, &'a str)];

pub type VariantKey = Vec<(String, String)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub model: Identifier,
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub uvlock: bool,
    pub weight: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Term {
    pub value: String,
    pub negated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition {
    Properties(Vec<(String, Vec<Term>)>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

impl Condition {
    pub fn matches(&self, props: Properties) -> bool {
        match self {
            Condition::Properties(tests) => tests.iter().all(|(key, terms)| {
                let actual = props.iter().find(|(k, _)| k == key).map(|(_, v)| *v);
                terms.iter().any(|t| match actual {
                    Some(v) => (v == t.value) != t.negated,
                    None => false,
                })
            }),
            Condition::And(c) => c.iter().all(|c| c.matches(props)),
            Condition::Or(c) => c.iter().any(|c| c.matches(props)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub when: Option<Condition>,
    pub apply: Vec<Variant>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockStateDef {
    pub variants: Vec<(VariantKey, Vec<Variant>)>,
    pub multipart: Vec<Selector>,
}

struct Ordered<V>(Vec<(String, V)>);

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Vis<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for Vis<V> {
            type Value = Ordered<V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    out.push(entry);
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(Vis(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
struct VariantJson {
    model: String,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    z: i32,
    #[serde(default)]
    uvlock: bool,
    weight: Option<u32>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum VariantListJson {
    One(VariantJson),
    Many(Vec<VariantJson>),
}

#[derive(Deserialize)]
struct SelectorJson {
    when: Option<Value>,
    apply: VariantListJson,
}

#[derive(Deserialize)]
struct BlockStateJson {
    variants: Option<Ordered<VariantListJson>>,
    multipart: Option<Vec<SelectorJson>>,
}

fn quadrants(degrees: i32, path: &str) -> Result<u8> {
    match degrees.rem_euclid(360) {
        0 => Ok(0),
        90 => Ok(1),
        180 => Ok(2),
        270 => Ok(3),
        _ => Err(AssetError::invalid(
            path,
            "rotation must be a multiple of 90",
        )),
    }
}

fn convert_variants(list: VariantListJson, path: &str) -> Result<Vec<Variant>> {
    let items = match list {
        VariantListJson::One(v) => vec![v],
        VariantListJson::Many(v) => v,
    };
    if items.is_empty() {
        return Err(AssetError::invalid(path, "empty variant list"));
    }
    items
        .into_iter()
        .map(|v| {
            Ok(Variant {
                model: Identifier::parse(&v.model),
                x: quadrants(v.x, path)?,
                y: quadrants(v.y, path)?,
                z: quadrants(v.z, path)?,
                uvlock: v.uvlock,
                weight: v.weight.unwrap_or(1).max(1),
            })
        })
        .collect()
}

fn parse_terms(value: &Value, path: &str) -> Result<Vec<Term>> {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        _ => {
            return Err(AssetError::invalid(
                path,
                "condition value must be a string",
            ));
        }
    };
    text.split('|')
        .map(|t| match t.strip_prefix('!') {
            Some(v) => Term {
                value: v.to_owned(),
                negated: true,
            },
            None => Term {
                value: t.to_owned(),
                negated: false,
            },
        })
        .map(|t| {
            if t.value.is_empty() {
                Err(AssetError::invalid(path, "empty condition term"))
            } else {
                Ok(t)
            }
        })
        .collect()
}

fn parse_condition(value: &Value, path: &str) -> Result<Condition> {
    let obj = value
        .as_object()
        .ok_or_else(|| AssetError::invalid(path, "condition must be an object"))?;
    for (key, ctor) in [
        ("AND", Condition::And as fn(Vec<Condition>) -> Condition),
        ("OR", Condition::Or),
    ] {
        if let Some(list) = obj.get(key) {
            let list = list
                .as_array()
                .ok_or_else(|| AssetError::invalid(path, "AND/OR needs an array"))?;
            return list
                .iter()
                .map(|c| parse_condition(c, path))
                .collect::<Result<_>>()
                .map(ctor);
        }
    }
    obj.iter()
        .map(|(k, v)| Ok((k.clone(), parse_terms(v, path)?)))
        .collect::<Result<_>>()
        .map(Condition::Properties)
}

fn parse_key(key: &str) -> Vec<(String, String)> {
    key.split(',')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

impl BlockStateDef {
    pub fn parse(bytes: &[u8], path: &str) -> Result<Self> {
        let json: BlockStateJson =
            serde_json::from_slice(bytes).map_err(|source| AssetError::Json {
                path: path.to_owned(),
                source,
            })?;
        let mut def = BlockStateDef::default();
        for (key, list) in json.variants.map(|o| o.0).unwrap_or_default() {
            def.variants
                .push((parse_key(&key), convert_variants(list, path)?));
        }
        for sel in json.multipart.unwrap_or_default() {
            def.multipart.push(Selector {
                when: sel
                    .when
                    .as_ref()
                    .map(|w| parse_condition(w, path))
                    .transpose()?,
                apply: convert_variants(sel.apply, path)?,
            });
        }
        if def.variants.is_empty() && def.multipart.is_empty() {
            return Err(AssetError::invalid(path, "neither variants nor multipart"));
        }
        Ok(def)
    }

    /// Each returned slice is one weighted choice group, in definition order.
    pub fn select<'a>(&'a self, props: Properties) -> Vec<&'a [Variant]> {
        if let Some((_, list)) = self.variants.iter().find(|(key, _)| {
            key.iter()
                .all(|(k, v)| props.iter().any(|(pk, pv)| pk == k && pv == v))
        }) {
            return vec![list];
        }
        self.multipart
            .iter()
            .filter(|s| s.when.as_ref().is_none_or(|c| c.matches(props)))
            .map(|s| s.apply.as_slice())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> BlockStateDef {
        BlockStateDef::parse(json.as_bytes(), "t").unwrap()
    }

    #[test]
    fn variants_match_subset_keys_and_lists() {
        let def = parse(
            r#"{"variants":{
                "facing=north,lit=true":{"model":"a","y":90,"uvlock":true},
                "facing=north":[{"model":"b","weight":3},{"model":"c"}],
                "":{"model":"d"}}}"#,
        );
        let hit = def.select(&[("facing", "north"), ("lit", "true")]);
        assert_eq!(hit[0][0].model.path(), "a");
        assert_eq!(hit[0][0].y, 1);
        assert!(hit[0][0].uvlock);
        let hit = def.select(&[("facing", "north"), ("lit", "false")]);
        assert_eq!(hit[0].len(), 2);
        assert_eq!(hit[0][0].weight, 3);
        let hit = def.select(&[("facing", "south")]);
        assert_eq!(hit[0][0].model.path(), "d");
    }

    #[test]
    fn multipart_conditions() {
        let def = parse(
            r#"{"multipart":[
                {"apply":{"model":"post"}},
                {"when":{"north":"true"},"apply":{"model":"n"}},
                {"when":{"OR":[{"east":"side|up"},{"south":"!none"}]},"apply":{"model":"o"}},
                {"when":{"AND":[{"west":true},{"up":"false"}]},"apply":[{"model":"w1"},{"model":"w2"}]}]}"#,
        );
        let names = |p: Properties| -> Vec<String> {
            def.select(p)
                .iter()
                .map(|g| g[0].model.path().to_owned())
                .collect()
        };
        assert_eq!(names(&[("north", "false")]), ["post"]);
        assert_eq!(
            names(&[("north", "true"), ("east", "up")]),
            ["post", "n", "o"]
        );
        assert_eq!(names(&[("south", "low")]), ["post", "o"]);
        assert_eq!(names(&[("south", "none")]), ["post"]);
        let both = def.select(&[("west", "true"), ("up", "false")]);
        assert_eq!(both.len(), 2);
        assert_eq!(both[1].len(), 2);
    }

    #[test]
    fn rejects_bad_rotation() {
        assert!(BlockStateDef::parse(br#"{"variants":{"":{"model":"a","y":45}}}"#, "t").is_err());
    }
}
