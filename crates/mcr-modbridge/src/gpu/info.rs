//! Parser for the pipeline description `PipelineInfo.java` writes (renderpearl's `CreateInfo`).

use super::spv::UniformKind;

pub const MAGIC: u32 = 0x5052_434d;
pub const FORMAT: u32 = 1;
const MAX_SHADERS: u32 = 2;
const MAX_LIST: u32 = 64;
const MAX_SPV_BYTES: u32 = 4 << 20;
const MAX_NAME: u32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

#[derive(Debug, Clone)]
pub struct Shader {
    pub stage: Stage,
    pub spirv: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexBuffer {
    pub slot: u32,
    pub stride: u32,
    pub step_rate: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute {
    pub slot: u32,
    pub location: u32,
    pub offset: u32,
    pub format: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uniform {
    pub kind: UniformKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Depth {
    pub compare: u32,
    pub write: bool,
    pub bias_slope: f32,
    pub bias_constant: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blend {
    pub color: [u32; 3],
    pub alpha: [u32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorTarget {
    pub format: u32,
    pub write_mask: u32,
    pub blend: Option<Blend>,
}

#[derive(Debug, Clone)]
pub struct PipelineInfo {
    pub name: String,
    pub shaders: Vec<Shader>,
    pub vertex_buffers: Vec<VertexBuffer>,
    pub attributes: Vec<Attribute>,
    pub uniforms: Vec<Uniform>,
    pub push_constants_size: u32,
    pub depth: Option<Depth>,
    pub wireframe: bool,
    pub cull: bool,
    pub topology: u32,
    pub color_targets: Vec<Option<ColorTarget>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InfoError {
    #[error("pipeline description truncated at byte {0}")]
    Truncated(usize),
    #[error("pipeline description has magic {0:#x} / format {1}")]
    Version(u32, u32),
    #[error("{what} count {count} is over the limit")]
    TooMany { what: &'static str, count: u32 },
    #[error("unknown {what} code {code}")]
    Code { what: &'static str, code: u32 },
    #[error("name is not UTF-8")]
    Utf8,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Result<u32, InfoError> {
        let b = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or(InfoError::Truncated(self.at))?;
        self.at += 4;
        Ok(u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, InfoError> {
        self.u32().map(f32::from_bits)
    }

    fn bytes(&mut self, max: u32) -> Result<&'a [u8], InfoError> {
        let len = self.u32()?;
        if len > max {
            return Err(InfoError::TooMany {
                what: "bytes",
                count: len,
            });
        }
        let len = len as usize;
        let b = self
            .bytes
            .get(self.at..self.at + len)
            .ok_or(InfoError::Truncated(self.at))?;
        self.at += (len + 3) & !3;
        Ok(b)
    }

    fn string(&mut self) -> Result<String, InfoError> {
        let b = self.bytes(MAX_NAME)?;
        String::from_utf8(b.to_vec()).map_err(|_| InfoError::Utf8)
    }

    fn count(&mut self, what: &'static str, max: u32) -> Result<u32, InfoError> {
        let count = self.u32()?;
        if count > max {
            return Err(InfoError::TooMany { what, count });
        }
        Ok(count)
    }
}

fn code(what: &'static str, code: u32, len: usize) -> Result<u32, InfoError> {
    if (code as usize) < len {
        Ok(code)
    } else {
        Err(InfoError::Code { what, code })
    }
}

impl PipelineInfo {
    pub fn parse(bytes: &[u8]) -> Result<Self, InfoError> {
        use super::enums::*;
        let mut r = Reader { bytes, at: 0 };
        let (magic, format) = (r.u32()?, r.u32()?);
        if magic != MAGIC || format != FORMAT {
            return Err(InfoError::Version(magic, format));
        }
        let name = r.string()?;
        let mut shaders = Vec::new();
        for _ in 0..r.count("shader", MAX_SHADERS)? {
            let stage = match r.u32()? {
                0 => Stage::Vertex,
                1 => Stage::Fragment,
                other => {
                    return Err(InfoError::Code {
                        what: "stage",
                        code: other,
                    });
                }
            };
            let spirv = super::spv::words(r.bytes(MAX_SPV_BYTES)?)
                .map_err(|_| InfoError::Truncated(r.at))?;
            shaders.push(Shader { stage, spirv });
        }
        let mut vertex_buffers = Vec::new();
        for _ in 0..r.count("vertex buffer", 16)? {
            vertex_buffers.push(VertexBuffer {
                slot: r.u32()?,
                stride: r.u32()?,
                step_rate: r.u32()?,
            });
        }
        let mut attributes = Vec::new();
        for _ in 0..r.count("attribute", MAX_LIST)? {
            attributes.push(Attribute {
                slot: r.u32()?,
                location: r.u32()?,
                offset: r.u32()?,
                format: code("vertex format", r.u32()?, GPU_FORMATS.len())?,
            });
        }
        let mut uniforms = Vec::new();
        for _ in 0..r.count("uniform", MAX_LIST)? {
            let kind = match r.u32()? {
                0 => UniformKind::CombinedSampler,
                1 => UniformKind::Buffer,
                2 => UniformKind::TexelBuffer,
                other => {
                    return Err(InfoError::Code {
                        what: "uniform type",
                        code: other,
                    });
                }
            };
            let _format = r.u32()?;
            uniforms.push(Uniform {
                kind,
                name: r.string()?,
            });
        }
        let push_constants_size = r.u32()?;
        let has_depth = r.u32()? != 0;
        let depth = Depth {
            compare: code("compare op", r.u32()?, COMPARE_OPS.len())?,
            write: r.u32()? != 0,
            bias_slope: r.f32()?,
            bias_constant: r.f32()?,
        };
        let wireframe = code("polygon mode", r.u32()?, POLYGON_MODES.len())? == 1;
        let cull = r.u32()? != 0;
        let topology = code("topology", r.u32()?, TOPOLOGIES.len())?;
        let mut color_targets = Vec::new();
        for _ in 0..r.count("color target", 8)? {
            let present = r.u32()? != 0;
            let format = code("color format", r.u32()?, GPU_FORMATS.len())?;
            let write_mask = r.u32()? & 0xF;
            let has_blend = r.u32()? != 0;
            let mut eq = [0u32; 6];
            for (i, v) in eq.iter_mut().enumerate() {
                let what = if i % 3 == 2 {
                    "blend op"
                } else {
                    "blend factor"
                };
                let len = if i % 3 == 2 {
                    BLEND_OPS.len()
                } else {
                    BLEND_FACTORS.len()
                };
                *v = code(what, r.u32()?, len)?;
            }
            color_targets.push(present.then_some(ColorTarget {
                format,
                write_mask,
                blend: has_blend.then_some(Blend {
                    color: [eq[0], eq[1], eq[2]],
                    alpha: [eq[3], eq[4], eq[5]],
                }),
            }));
        }
        Ok(Self {
            name,
            shaders,
            vertex_buffers,
            attributes,
            uniforms,
            push_constants_size,
            depth: has_depth.then_some(depth),
            wireframe,
            cull,
            topology,
            color_targets,
        })
    }

    /// The byte layout `PipelineInfo.java` writes; Rust uses it for tests and benches.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let u = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_ne_bytes());
        let bytes = |out: &mut Vec<u8>, b: &[u8]| {
            u(out, b.len() as u32);
            out.extend_from_slice(b);
            out.resize(out.len().next_multiple_of(4), 0);
        };
        u(&mut out, MAGIC);
        u(&mut out, FORMAT);
        bytes(&mut out, self.name.as_bytes());
        u(&mut out, self.shaders.len() as u32);
        for s in &self.shaders {
            u(&mut out, u32::from(s.stage == Stage::Fragment));
            let b: Vec<u8> = s.spirv.iter().flat_map(|w| w.to_le_bytes()).collect();
            bytes(&mut out, &b);
        }
        u(&mut out, self.vertex_buffers.len() as u32);
        for b in &self.vertex_buffers {
            for v in [b.slot, b.stride, b.step_rate] {
                u(&mut out, v);
            }
        }
        u(&mut out, self.attributes.len() as u32);
        for a in &self.attributes {
            for v in [a.slot, a.location, a.offset, a.format] {
                u(&mut out, v);
            }
        }
        u(&mut out, self.uniforms.len() as u32);
        for un in &self.uniforms {
            let kind = match un.kind {
                UniformKind::CombinedSampler => 0,
                UniformKind::Buffer => 1,
                UniformKind::TexelBuffer => 2,
            };
            u(&mut out, kind);
            u(&mut out, u32::MAX);
            bytes(&mut out, un.name.as_bytes());
        }
        u(&mut out, self.push_constants_size);
        let d = self.depth.unwrap_or(Depth {
            compare: 0,
            write: false,
            bias_slope: 0.0,
            bias_constant: 0.0,
        });
        for v in [
            u32::from(self.depth.is_some()),
            d.compare,
            u32::from(d.write),
            d.bias_slope.to_bits(),
            d.bias_constant.to_bits(),
            u32::from(self.wireframe),
            u32::from(self.cull),
            self.topology,
            self.color_targets.len() as u32,
        ] {
            u(&mut out, v);
        }
        for target in &self.color_targets {
            let t = target.unwrap_or(ColorTarget {
                format: 0,
                write_mask: 0,
                blend: None,
            });
            let b = t.blend.unwrap_or(Blend {
                color: [0; 3],
                alpha: [0; 3],
            });
            for v in [
                u32::from(target.is_some()),
                t.format,
                t.write_mask,
                u32::from(t.blend.is_some()),
            ] {
                u(&mut out, v);
            }
            for v in b.color.iter().chain(&b.alpha) {
                u(&mut out, *v);
            }
        }
        out
    }

    pub fn uniform_kinds(&self) -> Vec<UniformKind> {
        self.uniforms.iter().map(|u| u.kind).collect()
    }

    pub fn uses_texel_buffers(&self) -> bool {
        self.uniforms
            .iter()
            .any(|u| u.kind == UniformKind::TexelBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PipelineInfo {
        PipelineInfo {
            name: "test/pipeline".into(),
            shaders: vec![Shader {
                stage: Stage::Fragment,
                spirv: vec![0x0723_0203, 0x0001_0000, 0, 1, 0],
            }],
            vertex_buffers: vec![VertexBuffer {
                slot: 0,
                stride: 16,
                step_rate: 0,
            }],
            attributes: vec![Attribute {
                slot: 0,
                location: 1,
                offset: 8,
                format: 45,
            }],
            uniforms: vec![Uniform {
                kind: UniformKind::Buffer,
                name: "Globals".into(),
            }],
            push_constants_size: 16,
            depth: Some(Depth {
                compare: 5,
                write: true,
                bias_slope: 1.5,
                bias_constant: -2.0,
            }),
            wireframe: true,
            cull: true,
            topology: 6,
            color_targets: vec![
                Some(ColorTarget {
                    format: 6,
                    write_mask: 0xF,
                    blend: Some(Blend {
                        color: [11, 9, 0],
                        alpha: [4, 14, 0],
                    }),
                }),
                None,
            ],
        }
    }

    #[test]
    fn encode_parse_round_trip() {
        let info = sample();
        let back = PipelineInfo::parse(&info.encode()).unwrap();
        assert_eq!(back.name, info.name);
        assert_eq!(back.shaders[0].spirv, info.shaders[0].spirv);
        assert_eq!(back.vertex_buffers, info.vertex_buffers);
        assert_eq!(back.attributes, info.attributes);
        assert_eq!(back.uniforms, info.uniforms);
        assert_eq!(back.depth, info.depth);
        assert_eq!(back.color_targets, info.color_targets);
        assert_eq!((back.wireframe, back.cull, back.topology), (true, true, 6));
    }

    #[test]
    fn hostile_descriptions_are_rejected() {
        let bytes = sample().encode();
        for cut in [0, 7, 12, bytes.len() / 2, bytes.len() - 1] {
            assert!(PipelineInfo::parse(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut huge = bytes.clone();
        huge[8..12].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert!(matches!(
            PipelineInfo::parse(&huge),
            Err(InfoError::TooMany { .. })
        ));
        let mut version = bytes.clone();
        version[4] = 9;
        assert!(matches!(
            PipelineInfo::parse(&version),
            Err(InfoError::Version(..))
        ));
        let mut topology = sample();
        topology.topology = 99;
        assert!(matches!(
            PipelineInfo::parse(&topology.encode()),
            Err(InfoError::Code {
                what: "topology",
                ..
            })
        ));
    }
}
