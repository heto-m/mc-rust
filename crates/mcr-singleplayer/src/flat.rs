//! Custom superflat layers as the dedicated server's `generator-settings` JSON.

use serde_json::{Map, Value, json};

pub const MAX_INPUT: usize = 8 * 1024;
pub const MAX_LAYERS: usize = 64;
/// The tallest a flat world can be (vanilla's dimension height limit).
pub const MAX_HEIGHT: u32 = 4064;
const MAX_STRUCTURES: usize = 64;
const MAX_ID: usize = 128;
/// Vanilla's Classic Flat preset.
pub const CLASSIC_FLAT: &str =
    "minecraft:bedrock,2*minecraft:dirt,minecraft:grass_block;minecraft:plains";

/// A namespaced lowercase id (`minecraft:` added when missing), or `None`.
pub fn identifier(text: &str) -> Option<String> {
    let text = text.trim();
    let (ns, path) = text.split_once(':').unwrap_or(("minecraft", text));
    let ns_ok = !ns.is_empty()
        && ns
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
    let path_ok = !path.is_empty()
        && path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_./-".contains(&b));
    let id = format!("{ns}:{path}");
    (ns_ok && path_ok && id.len() <= MAX_ID).then_some(id)
}

struct Layer {
    block: String,
    height: u32,
}

fn check_layers(layers: &[Layer], is_block: &dyn Fn(&str) -> bool) -> Result<(), String> {
    if layers.len() > MAX_LAYERS {
        return Err(format!("at most {MAX_LAYERS} layers"));
    }
    let mut total = 0u32;
    for layer in layers {
        if layer.height > MAX_HEIGHT {
            return Err(format!("a layer is taller than {MAX_HEIGHT}"));
        }
        total += layer.height;
        if !is_block(&layer.block) {
            return Err(format!("unknown block {}", layer.block));
        }
    }
    if total > MAX_HEIGHT {
        return Err(format!("the layers are taller than {MAX_HEIGHT}"));
    }
    Ok(())
}

fn layers_json(layers: &[Layer]) -> Value {
    Value::Array(
        layers
            .iter()
            .map(|l| json!({ "block": l.block, "height": l.height }))
            .collect(),
    )
}

/// Vanilla's preset string, `[n*]block,…;biome`.
fn from_preset(text: &str, is_block: &dyn Fn(&str) -> bool) -> Result<Value, String> {
    let (layer_text, biome) = match text.split_once(';') {
        Some((l, b)) => (l, Some(b)),
        None => (text, None),
    };
    let mut layers = Vec::new();
    for part in layer_text.split(',').filter(|p| !p.trim().is_empty()) {
        if layers.len() >= MAX_LAYERS {
            return Err(format!("at most {MAX_LAYERS} layers"));
        }
        let (height, block) = match part.split_once('*') {
            Some((n, b)) => (
                n.trim()
                    .parse::<u32>()
                    .map_err(|_| format!("bad layer height in {part}"))?,
                b,
            ),
            None => (1, part),
        };
        let block = identifier(block).ok_or_else(|| format!("bad block id {}", block.trim()))?;
        layers.push(Layer { block, height });
    }
    check_layers(&layers, is_block)?;
    let mut out = Map::new();
    out.insert("layers".into(), layers_json(&layers));
    let biome = match biome.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => identifier(b).ok_or_else(|| format!("bad biome id {b}"))?,
        None => "minecraft:plains".into(),
    };
    out.insert("biome".into(), Value::String(biome));
    Ok(Value::Object(out))
}

fn from_json(text: &str, is_block: &dyn Fn(&str) -> bool) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("bad settings: {e}"))?;
    let obj = value.as_object().ok_or("the settings are not an object")?;
    let raw = obj
        .get("layers")
        .and_then(Value::as_array)
        .ok_or("the settings have no layers")?;
    if raw.len() > MAX_LAYERS {
        return Err(format!("at most {MAX_LAYERS} layers"));
    }
    let mut layers = Vec::with_capacity(raw.len());
    for layer in raw {
        let block = layer
            .get("block")
            .and_then(Value::as_str)
            .and_then(identifier)
            .ok_or("a layer has no valid block")?;
        let height = layer
            .get("height")
            .and_then(Value::as_u64)
            .and_then(|h| u32::try_from(h).ok())
            .ok_or("a layer has no valid height")?;
        layers.push(Layer { block, height });
    }
    check_layers(&layers, is_block)?;
    let mut out = Map::new();
    out.insert("layers".into(), layers_json(&layers));
    if let Some(b) = obj.get("biome") {
        let id = b
            .as_str()
            .and_then(identifier)
            .ok_or("the biome is not a valid id")?;
        out.insert("biome".into(), Value::String(id));
    }
    for key in ["features", "lakes"] {
        if let Some(v) = obj.get(key) {
            out.insert(key.into(), Value::Bool(v.as_bool().ok_or("not a boolean")?));
        }
    }
    if let Some(s) = obj.get("structure_overrides") {
        let tag_or_id = |v: &Value| -> Result<String, String> {
            let text = v.as_str().ok_or("bad structure override")?;
            match text.strip_prefix('#') {
                Some(tag) => identifier(tag).map(|t| format!("#{t}")),
                None => identifier(text),
            }
            .ok_or_else(|| "bad structure override".to_owned())
        };
        let value = match s {
            Value::Array(items) => {
                if items.len() > MAX_STRUCTURES {
                    return Err(format!("at most {MAX_STRUCTURES} structure overrides"));
                }
                Value::Array(
                    items
                        .iter()
                        .map(|v| tag_or_id(v).map(Value::String))
                        .collect::<Result<_, _>>()?,
                )
            }
            other => Value::String(tag_or_id(other)?),
        };
        out.insert("structure_overrides".into(), value);
    }
    Ok(Value::Object(out))
}

/// Compact, validated `generator-settings` from a preset string or the generator JSON; `is_block` checks block ids.
pub fn generator_settings(text: &str, is_block: &dyn Fn(&str) -> bool) -> Result<String, String> {
    let text = text.trim();
    if text.len() > MAX_INPUT {
        return Err("the superflat settings are too long".into());
    }
    let value = if text.starts_with('{') {
        from_json(text, is_block)?
    } else {
        from_preset(text, is_block)?
    };
    Ok(value.to_string())
}

/// The layers and biome of generator JSON as a preset string, when they fit one.
pub fn preset_string(settings: &str) -> Option<String> {
    let value: Value = serde_json::from_str(settings).ok()?;
    let layers = value.get("layers")?.as_array()?;
    let mut parts = Vec::with_capacity(layers.len());
    for layer in layers.iter().take(MAX_LAYERS) {
        let block = layer.get("block")?.as_str()?;
        match layer.get("height")?.as_u64()? {
            1 => parts.push(block.to_owned()),
            n => parts.push(format!("{n}*{block}")),
        }
    }
    let biome = value
        .get("biome")
        .and_then(Value::as_str)
        .unwrap_or("minecraft:plains");
    Some(format!("{};{biome}", parts.join(",")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any(_: &str) -> bool {
        true
    }

    #[test]
    fn preset_strings() {
        let json = generator_settings(CLASSIC_FLAT, &any).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["layers"][1]["height"], 2);
        assert_eq!(v["layers"][1]["block"], "minecraft:dirt");
        assert_eq!(v["biome"], "minecraft:plains");
        assert_eq!(preset_string(&json).unwrap(), CLASSIC_FLAT);
        let v: Value =
            serde_json::from_str(&generator_settings("bedrock,3*stone;desert", &any).unwrap())
                .unwrap();
        assert_eq!(v["layers"][1]["block"], "minecraft:stone");
        assert_eq!(v["biome"], "minecraft:desert");
    }

    #[test]
    fn hostile_presets_are_refused() {
        for bad in [
            "minecraft:Stone",
            "9999*minecraft:stone",
            "2000*stone,2100*stone",
            "x*stone",
            "stone;Bad Biome",
            "../../etc:passwd",
            &"stone,".repeat(65),
            &"a".repeat(MAX_INPUT + 1),
        ] {
            assert!(generator_settings(bad, &any).is_err(), "{bad:.40}");
        }
        assert!(generator_settings("minecraft:nope", &|b| b == "minecraft:stone").is_err());
        assert!(
            generator_settings("", &any).is_ok(),
            "a void world has no layers"
        );
    }

    #[test]
    fn json_is_validated_and_compacted() {
        let java = r##"{"layers":[{"block":"minecraft:bedrock","height":1},{"block":"minecraft:sandstone","height":52}],
            "biome":"minecraft:desert","features":true,"lakes":false,
            "structure_overrides":["minecraft:villages","#minecraft:x"],"evil":"x"}"##;
        let out = generator_settings(java, &any).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v.get("evil").is_none());
        assert_eq!(v["features"], true);
        assert_eq!(v["structure_overrides"][1], "#minecraft:x");
        assert!(!out.contains('\n'));
        for bad in [
            r#"{"layers":[{"block":"minecraft:stone","height":-1}]}"#,
            r#"{"layers":[{"block":"STONE","height":1}]}"#,
            r#"{"layers":"x"}"#,
            r#"{"layers":[],"features":"yes"}"#,
            r#"{"layers":[],"structure_overrides":[1]}"#,
            r#"[1]"#,
            "{",
        ] {
            assert!(generator_settings(bad, &any).is_err(), "{bad}");
        }
    }
}
