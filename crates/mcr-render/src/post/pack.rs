//! Resource-pack post chains. A pack's `post_effect/<chain>.json` is followed only when it has the
//! built-in chain's shape (same programs, targets and filters); then its uniform values retune our
//! WGSL passes. Anything else, such as pack GLSL or a different graph, keeps the built-in chain.

use serde_json::Value;

use super::{Pass, PostChain, Program, Slot, SpiderEye};

const MAX_JSON_BYTES: usize = 64 * 1024;

/// Pass lists for the chains a resource pack re-tuned.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostOverrides {
    chains: Vec<(PostChain, Vec<Pass>)>,
}

impl PostOverrides {
    /// Reads `json` as the pack's version of `chain`; returns whether it was followed.
    pub fn set(&mut self, chain: PostChain, json: &str) -> bool {
        self.chains.retain(|(c, _)| *c != chain);
        match follow(chain, json) {
            Some(passes) => {
                self.chains.push((chain, passes));
                true
            }
            None => false,
        }
    }

    /// Whether a pack file retunes `chain`.
    pub fn follows(&self, chain: PostChain) -> bool {
        self.chains.iter().any(|(c, _)| *c == chain)
    }

    pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }

    pub(super) fn passes(&self, chain: PostChain) -> &[Pass] {
        self.chains
            .iter()
            .find(|(c, _)| *c == chain)
            .map_or_else(|| chain.passes(), |(_, p)| p)
    }
}

fn slot_name(slot: Slot) -> &'static str {
    match slot {
        Slot::Main => "main",
        Slot::Swap => "swap",
        Slot::Temp => "temp",
        Slot::LargeBlur => "large_blur",
        Slot::SmallBlur => "small_blur",
    }
}

fn target_is(value: &Value, slot: Slot) -> bool {
    value
        .as_str()
        .is_some_and(|t| t.strip_prefix("minecraft:").unwrap_or(t) == slot_name(slot))
}

fn shaders(program: Program) -> Option<(&'static str, &'static str)> {
    let quad = "minecraft:core/screenquad";
    Some(match program {
        Program::BoxBlur { .. } => ("minecraft:post/box_blur", quad),
        Program::Convolve { .. } => ("minecraft:post/color_convolve", quad),
        Program::Bits { .. } => ("minecraft:post/bits", quad),
        Program::Invert { .. } => ("minecraft:post/invert", quad),
        Program::Blit { .. } => ("minecraft:post/blit", quad),
        Program::SpiderClip(_) => ("minecraft:post/spiderclip", "minecraft:post/rotscale"),
        Program::Sobel | Program::OutlineBlur { .. } => return None,
    })
}

fn follow(chain: PostChain, json: &str) -> Option<Vec<Pass>> {
    if json.len() > MAX_JSON_BYTES || chain == PostChain::EntityOutline {
        return None;
    }
    let root: Value = serde_json::from_str(json).ok()?;
    if let Some(targets) = root.get("targets") {
        let targets = targets.as_object()?;
        if targets
            .values()
            .any(|t| t.as_object().is_none_or(|o| !o.is_empty()))
        {
            return None;
        }
    }
    let specs = root.get("passes")?.as_array()?;
    let built_in = chain.passes();
    if specs.len() != built_in.len() {
        return None;
    }
    specs
        .iter()
        .zip(built_in)
        .map(|(spec, base)| follow_pass(spec, base))
        .collect()
}

fn follow_pass(spec: &Value, base: &Pass) -> Option<Pass> {
    let (fragment, vertex) = shaders(base.program)?;
    if spec.get("fragment_shader")?.as_str()? != fragment
        || spec.get("vertex_shader")?.as_str()? != vertex
        || !target_is(spec.get("output")?, base.output)
    {
        return None;
    }
    let inputs = spec.get("inputs")?.as_array()?;
    let expected = [Some(base.input), base.aux];
    if inputs.len() != expected.iter().flatten().count() {
        return None;
    }
    for (input, slot) in inputs.iter().zip(expected.iter().flatten()) {
        let bilinear = input
            .get("bilinear")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !target_is(input.get("target")?, *slot)
            || (slot == &base.input && bilinear != base.linear)
        {
            return None;
        }
    }
    let uniforms = spec.get("uniforms")?;
    Some(Pass {
        program: tuned(base.program, uniforms)?,
        ..*base
    })
}

fn values<const N: usize>(
    uniforms: &Value,
    block: &str,
    name: &str,
    lo: f32,
    hi: f32,
) -> Option<[f32; N]> {
    let entry = uniforms
        .get(block)?
        .as_array()?
        .iter()
        .find(|u| u.get("name").and_then(Value::as_str) == Some(name))?;
    let value = entry.get("value")?;
    let numbers: Vec<f64> = match value {
        Value::Array(items) => items.iter().map(Value::as_f64).collect::<Option<_>>()?,
        other => vec![other.as_f64()?],
    };
    if numbers.len() != N {
        return None;
    }
    Some(std::array::from_fn(|i| (numbers[i] as f32).clamp(lo, hi)))
}

fn tuned(program: Program, u: &Value) -> Option<Program> {
    Some(match program {
        Program::BoxBlur { .. } => Program::BoxBlur {
            dir: values(u, "BlurConfig", "BlurDir", -4.0, 4.0)?,
            radius: values::<1>(u, "BlurConfig", "Radius", 0.0, 32.0)?[0],
        },
        Program::Convolve { .. } => {
            let row = |name| values::<3>(u, "ColorConfig", name, -8.0, 8.0);
            Program::Convolve {
                rows: [row("RedMatrix")?, row("GreenMatrix")?, row("BlueMatrix")?],
            }
        }
        Program::Bits { .. } => Program::Bits {
            levels: values::<1>(u, "BitsConfig", "Resolution", 1.0, 256.0)?[0],
            mosaic: values::<1>(u, "BitsConfig", "MosaicSize", 1.0, 64.0)?[0],
        },
        Program::Invert { .. } => Program::Invert {
            amount: values::<1>(u, "InvertConfig", "InverseAmount", -2.0, 2.0)?[0],
        },
        Program::Blit { .. } => Program::Blit {
            tint: values(u, "BlitConfig", "ColorModulate", 0.0, 8.0)?,
        },
        Program::SpiderClip(_) => Program::SpiderClip(SpiderEye {
            scale: values(u, "RotScaleConfig", "InScale", -64.0, 64.0)?,
            offset: values(u, "RotScaleConfig", "InOffset", -64.0, 64.0)?,
            degrees: values::<1>(u, "RotScaleConfig", "InRotation", -3600.0, 3600.0)?[0],
            clip: values(u, "SpiderConfig", "Scissor", -4.0, 4.0)?,
            fade: values(u, "SpiderConfig", "Vignette", -4.0, 4.0)?,
        }),
        Program::Sobel | Program::OutlineBlur { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVERT: &str = r#"{
        "targets": { "swap": {} },
        "passes": [
            { "vertex_shader": "minecraft:core/screenquad", "fragment_shader": "minecraft:post/invert",
              "inputs": [ { "sampler_name": "In", "target": "minecraft:main" } ], "output": "swap",
              "uniforms": { "InvertConfig": [ { "name": "InverseAmount", "type": "float", "value": 0.25 } ] } },
            { "vertex_shader": "minecraft:core/screenquad", "fragment_shader": "minecraft:post/blit",
              "inputs": [ { "sampler_name": "In", "target": "swap" } ], "output": "minecraft:main",
              "uniforms": { "BlitConfig": [ { "name": "ColorModulate", "type": "vec4", "value": [ 1.0, 0.5, 0.5, 9.0 ] } ] } }
        ]
    }"#;

    #[test]
    fn matching_chain_retunes_the_parameters() {
        let mut o = PostOverrides::default();
        assert!(o.set(PostChain::Invert, INVERT));
        let passes = o.passes(PostChain::Invert);
        assert_eq!(passes[0].program, Program::Invert { amount: 0.25 });
        assert_eq!(
            passes[1].program,
            Program::Blit {
                tint: [1.0, 0.5, 0.5, 8.0]
            }
        );
        assert_eq!(o.passes(PostChain::Creeper), PostChain::Creeper.passes());
    }

    #[test]
    fn built_in_json_reproduces_the_built_in_chains() {
        let mut o = PostOverrides::default();
        let json = INVERT
            .replace("0.25", "0.8")
            .replace("0.5, 0.5, 9.0", "1.0, 1.0, 1.0");
        assert!(o.set(PostChain::Invert, &json));
        assert_eq!(o.passes(PostChain::Invert), PostChain::Invert.passes());
    }

    #[test]
    fn other_shapes_keep_the_built_in_chain() {
        let mut o = PostOverrides::default();
        let renamed = INVERT.replace("minecraft:post/invert", "pack:post/custom");
        let rerouted = INVERT.replace("\"output\": \"swap\"", "\"output\": \"temp\"");
        let sized = INVERT.replace("\"swap\": {}", "\"swap\": { \"width\": 64 }");
        let missing = INVERT.replace("InverseAmount", "Other");
        let short = r#"{ "passes": [] }"#;
        for json in [
            renamed.as_str(),
            &rerouted,
            &sized,
            &missing,
            short,
            "not json",
            "[]",
        ] {
            assert!(!o.set(PostChain::Invert, json), "{json}");
        }
        assert!(o.is_empty());
        assert!(!o.set(PostChain::EntityOutline, INVERT));
    }

    #[test]
    fn vanilla_chain_files_reproduce_the_built_in_chains() {
        let jar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/versions/26.3/client.jar");
        let Ok(resources) = mcr_assets::ResourceManager::vanilla(&jar) else {
            eprintln!("skipping: no client jar");
            return;
        };
        for chain in PostChain::ALL {
            let path = format!("assets/minecraft/post_effect/{}.json", chain.name());
            let json = String::from_utf8(resources.read(&path).unwrap().unwrap()).unwrap();
            let mut o = PostOverrides::default();
            assert!(o.set(chain, &json), "{chain:?}");
            assert_eq!(o.passes(chain), chain.passes(), "{chain:?}");
        }
    }

    #[test]
    fn hostile_values_are_clamped_and_large_files_ignored() {
        let mut o = PostOverrides::default();
        let wild = INVERT.replace("0.25", "1e300");
        assert!(o.set(PostChain::Invert, &wild));
        assert_eq!(
            o.passes(PostChain::Invert)[0].program,
            Program::Invert { amount: 2.0 }
        );
        let huge = format!("{INVERT}{}", " ".repeat(MAX_JSON_BYTES));
        assert!(!o.set(PostChain::Invert, &huge));
    }
}
