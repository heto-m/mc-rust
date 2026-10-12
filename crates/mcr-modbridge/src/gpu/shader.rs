//! SPIR-V from the frontend → `spv_fixup` → naga IR, validated before wgpu sees it.

use wgpu::naga;

use super::info::{PipelineInfo, Stage};
use super::spv;

#[derive(Debug, thiserror::Error)]
pub enum ShaderError {
    #[error("{stage:?} shader: {source}")]
    Fixup { stage: Stage, source: spv::SpvError },
    #[error("{stage:?} shader: naga could not parse the SPIR-V: {message}")]
    Parse { stage: Stage, message: String },
    #[error("{stage:?} shader: naga validation failed: {message}")]
    Validate { stage: Stage, message: String },
}

#[derive(Debug)]
pub struct Translated {
    pub stage: Stage,
    pub module: naga::Module,
    pub info: naga::valid::ModuleInfo,
    /// The fixed-up SPIR-V the module came from.
    pub words: Vec<u32>,
}

/// The front end's Y-down clip space becomes wgpu's Y-up one: naga flips `gl_Position.y`. Without
/// the flip, geometry lands mirrored, which is top-down in a renderpearl target.
pub fn spv_options(adjust: bool) -> naga::front::spv::Options {
    naga::front::spv::Options {
        adjust_coordinate_space: adjust,
        strict_capabilities: false,
        block_ctx_dump_prefix: None,
    }
}

pub fn translate(info: &PipelineInfo) -> Result<Vec<Translated>, ShaderError> {
    let kinds = info.uniform_kinds();
    let mut out = Vec::with_capacity(info.shaders.len());
    for shader in &info.shaders {
        let stage = shader.stage;
        let words = spv::fixup(&shader.spirv, &kinds)
            .map_err(|source| ShaderError::Fixup { stage, source })?;
        out.push(parse(stage, words, true)?);
    }
    Ok(out)
}

/// Parses and validates fixed-up SPIR-V, with or without naga's clip-space Y flip.
pub fn parse(stage: Stage, words: Vec<u32>, adjust: bool) -> Result<Translated, ShaderError> {
    let module = naga::front::spv::Frontend::new(words.iter().copied(), &spv_options(adjust))
        .parse()
        .map_err(|e| ShaderError::Parse {
            stage,
            message: e.to_string(),
        })?;
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let module_info = validator
        .validate(&module)
        .map_err(|e| ShaderError::Validate {
            stage,
            message: format!("{:?}", e.into_inner()),
        })?;
    Ok(Translated {
        stage,
        module,
        info: module_info,
        words,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dumps() -> Option<Vec<PathBuf>> {
        let dir = mcr_jvm::dev_cache_dir().join("spv/26.3");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "mcrp"))
            .collect();
        files.sort();
        (!files.is_empty()).then_some(files)
    }

    /// Needs `.cache/spv/26.3` (`MCR_JVM_TESTS=1 MCR_GPU_DUMP=1 cargo test -p mcr-modbridge --test gpu_dump`).
    #[test]
    fn every_vanilla_pipeline_translates() {
        let Some(files) = dumps() else {
            eprintln!("skipped: no pipeline dumps under .cache/spv/26.3");
            return;
        };
        let mut ok = 0;
        let mut texel = 0;
        let mut failures = Vec::new();
        for path in &files {
            let bytes = std::fs::read(path).unwrap();
            let info = PipelineInfo::parse(&bytes).unwrap();
            match translate(&info) {
                Ok(_) => ok += 1,
                Err(ShaderError::Fixup {
                    source: spv::SpvError::TexelBuffer,
                    ..
                }) => texel += 1,
                Err(e) => failures.push(format!("{}: {e}", info.name)),
            }
        }
        eprintln!(
            "{ok} pipelines translate, {texel} use texel buffers, {} fail",
            failures.len()
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
