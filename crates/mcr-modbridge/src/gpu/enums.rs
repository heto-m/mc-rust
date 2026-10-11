//! Renderpearl enums as Java sends them (ordinals), mapped onto wgpu.
//!
//! The name tables mirror `BridgeAbi.java`; Java checks them against the live enums at boot.

pub const GPU_FORMATS: &[&str] = &[
    "R8_UNORM",
    "R8_SNORM",
    "RG8_UNORM",
    "RG8_SNORM",
    "RGB8_UNORM",
    "RGB8_SNORM",
    "RGBA8_UNORM",
    "RGBA8_SNORM",
    "R16_UNORM",
    "R16_SNORM",
    "RG16_UNORM",
    "RG16_SNORM",
    "RGB16_UNORM",
    "RGB16_SNORM",
    "RGBA16_UNORM",
    "RGBA16_SNORM",
    "R8_UINT",
    "R8_SINT",
    "RG8_UINT",
    "RG8_SINT",
    "RGB8_UINT",
    "RGB8_SINT",
    "RGBA8_UINT",
    "RGBA8_SINT",
    "R16_UINT",
    "R16_SINT",
    "RG16_UINT",
    "RG16_SINT",
    "RGB16_UINT",
    "RGB16_SINT",
    "RGBA16_UINT",
    "RGBA16_SINT",
    "R32_UINT",
    "R32_SINT",
    "RG32_UINT",
    "RG32_SINT",
    "RGB32_UINT",
    "RGB32_SINT",
    "RGBA32_UINT",
    "RGBA32_SINT",
    "R16_FLOAT",
    "RG16_FLOAT",
    "RGB16_FLOAT",
    "RGBA16_FLOAT",
    "R32_FLOAT",
    "RG32_FLOAT",
    "RGB32_FLOAT",
    "RGBA32_FLOAT",
    "RGB10A2_UNORM",
    "RGB10A2_UINT",
    "RG11B10_FLOAT",
    "D32_FLOAT",
    "D32_FLOAT_S8_UINT",
    "D24_UNORM_S8_UINT",
    "D16_UNORM",
    "S8_UINT",
];
pub const BLEND_FACTORS: &[&str] = &[
    "CONSTANT_ALPHA",
    "CONSTANT_COLOR",
    "DST_ALPHA",
    "DST_COLOR",
    "ONE",
    "ONE_MINUS_CONSTANT_ALPHA",
    "ONE_MINUS_CONSTANT_COLOR",
    "ONE_MINUS_DST_ALPHA",
    "ONE_MINUS_DST_COLOR",
    "ONE_MINUS_SRC_ALPHA",
    "ONE_MINUS_SRC_COLOR",
    "SRC_ALPHA",
    "SRC_ALPHA_SATURATE",
    "SRC_COLOR",
    "ZERO",
];
pub const BLEND_OPS: &[&str] = &["ADD", "SUBTRACT", "REVERSE_SUBTRACT", "MIN", "MAX"];
pub const COMPARE_OPS: &[&str] = &[
    "ALWAYS_PASS",
    "LESS_THAN",
    "LESS_THAN_OR_EQUAL",
    "EQUAL",
    "NOT_EQUAL",
    "GREATER_THAN_OR_EQUAL",
    "GREATER_THAN",
    "NEVER_PASS",
];
pub const POLYGON_MODES: &[&str] = &["FILL", "WIREFRAME"];
pub const TOPOLOGIES: &[&str] = &[
    "LINES",
    "DEBUG_LINES",
    "DEBUG_LINE_STRIP",
    "POINTS",
    "TRIANGLES",
    "TRIANGLE_STRIP",
    "TRIANGLE_FAN",
    "QUADS",
];
pub const UNIFORM_TYPES: &[&str] = &["COMBINED_IMAGE_SAMPLER", "UNIFORM_BUFFER", "TEXEL_BUFFER"];
pub const ADDRESS_MODES: &[&str] = &["REPEAT", "CLAMP_TO_EDGE"];
pub const FILTER_MODES: &[&str] = &["NEAREST", "LINEAR"];

pub const TOPOLOGY_TRIANGLE_FAN: u32 = 6;

const R8_UNORM: u32 = 0;
const RGBA8_UNORM: u32 = 6;
const R32_FLOAT: u32 = 44;
const D32_FLOAT: u32 = 51;
const D32_FLOAT_S8_UINT: u32 = 52;
const D24_UNORM_S8_UINT: u32 = 53;
const D16_UNORM: u32 = 54;

/// A texture format, or `None` for formats wgpu cannot store (the 3-component ones).
pub fn texture_format(code: u32) -> Option<wgpu::TextureFormat> {
    use wgpu::TextureFormat as T;
    Some(match code {
        R8_UNORM => T::R8Unorm,
        1 => T::R8Snorm,
        2 => T::Rg8Unorm,
        3 => T::Rg8Snorm,
        RGBA8_UNORM => T::Rgba8Unorm,
        7 => T::Rgba8Snorm,
        8 => T::R16Unorm,
        9 => T::R16Snorm,
        10 => T::Rg16Unorm,
        11 => T::Rg16Snorm,
        14 => T::Rgba16Unorm,
        15 => T::Rgba16Snorm,
        16 => T::R8Uint,
        17 => T::R8Sint,
        18 => T::Rg8Uint,
        19 => T::Rg8Sint,
        22 => T::Rgba8Uint,
        23 => T::Rgba8Sint,
        24 => T::R16Uint,
        25 => T::R16Sint,
        26 => T::Rg16Uint,
        27 => T::Rg16Sint,
        30 => T::Rgba16Uint,
        31 => T::Rgba16Sint,
        32 => T::R32Uint,
        33 => T::R32Sint,
        34 => T::Rg32Uint,
        35 => T::Rg32Sint,
        38 => T::Rgba32Uint,
        39 => T::Rgba32Sint,
        40 => T::R16Float,
        41 => T::Rg16Float,
        43 => T::Rgba16Float,
        R32_FLOAT => T::R32Float,
        45 => T::Rg32Float,
        47 => T::Rgba32Float,
        48 => T::Rgb10a2Unorm,
        49 => T::Rgb10a2Uint,
        50 => T::Rg11b10Ufloat,
        D32_FLOAT => T::Depth32Float,
        D32_FLOAT_S8_UINT => T::Depth32FloatStencil8,
        D24_UNORM_S8_UINT => T::Depth24PlusStencil8,
        D16_UNORM => T::Depth16Unorm,
        55 => T::Stencil8,
        _ => return None,
    })
}

/// Bytes per texel as Java packs them (`GpuFormat.blockSize()`).
pub fn block_size(code: u32) -> Option<u32> {
    let (component, count): (u32, u32) = match code {
        0..=7 => (1, [1, 1, 2, 2, 3, 3, 4, 4][code as usize]),
        8..=15 => (2, [1, 1, 2, 2, 3, 3, 4, 4][code as usize - 8]),
        16..=23 => (1, [1, 1, 2, 2, 3, 3, 4, 4][code as usize - 16]),
        24..=31 => (2, [1, 1, 2, 2, 3, 3, 4, 4][code as usize - 24]),
        32..=39 => (4, [1, 1, 2, 2, 3, 3, 4, 4][code as usize - 32]),
        40..=43 => (2, code - 39),
        44..=47 => (4, code - 43),
        48..=51 | 53 => (4, 1),
        52 => (8, 1),
        54 => (2, 1),
        55 => (1, 1),
        _ => return None,
    };
    Some(component * count)
}

/// Vertex attribute formats; 3-component 8/16-bit ones widen to 4 (vanilla pads them to 4 bytes).
pub fn vertex_format(code: u32) -> Option<(wgpu::VertexFormat, u32)> {
    use wgpu::VertexFormat as V;
    let format = match code {
        0 => V::Unorm8,
        1 => V::Snorm8,
        2 => V::Unorm8x2,
        3 => V::Snorm8x2,
        4 | 6 => V::Unorm8x4,
        5 | 7 => V::Snorm8x4,
        8 => V::Unorm16,
        9 => V::Snorm16,
        10 => V::Unorm16x2,
        11 => V::Snorm16x2,
        12 | 14 => V::Unorm16x4,
        13 | 15 => V::Snorm16x4,
        16 => V::Uint8,
        17 => V::Sint8,
        18 => V::Uint8x2,
        19 => V::Sint8x2,
        20 | 22 => V::Uint8x4,
        21 | 23 => V::Sint8x4,
        24 => V::Uint16,
        25 => V::Sint16,
        26 => V::Uint16x2,
        27 => V::Sint16x2,
        28 | 30 => V::Uint16x4,
        29 | 31 => V::Sint16x4,
        32 => V::Uint32,
        33 => V::Sint32,
        34 => V::Uint32x2,
        35 => V::Sint32x2,
        36 => V::Uint32x3,
        37 => V::Sint32x3,
        38 => V::Uint32x4,
        39 => V::Sint32x4,
        40 => V::Float16,
        41 => V::Float16x2,
        42 | 43 => V::Float16x4,
        44 => V::Float32,
        45 => V::Float32x2,
        46 => V::Float32x3,
        47 => V::Float32x4,
        48 => V::Unorm10_10_10_2,
        _ => return None,
    };
    Some((format, block_size(code)?))
}

pub fn blend_factor(code: u32) -> Option<wgpu::BlendFactor> {
    use wgpu::BlendFactor as B;
    Some(match code {
        0 | 1 => B::Constant,
        2 => B::DstAlpha,
        3 => B::Dst,
        4 => B::One,
        5 | 6 => B::OneMinusConstant,
        7 => B::OneMinusDstAlpha,
        8 => B::OneMinusDst,
        9 => B::OneMinusSrcAlpha,
        10 => B::OneMinusSrc,
        11 => B::SrcAlpha,
        12 => B::SrcAlphaSaturated,
        13 => B::Src,
        14 => B::Zero,
        _ => return None,
    })
}

pub fn blend_op(code: u32) -> Option<wgpu::BlendOperation> {
    use wgpu::BlendOperation as O;
    Some(match code {
        0 => O::Add,
        1 => O::Subtract,
        2 => O::ReverseSubtract,
        3 => O::Min,
        4 => O::Max,
        _ => return None,
    })
}

pub fn compare(code: u32) -> Option<wgpu::CompareFunction> {
    use wgpu::CompareFunction as C;
    Some(match code {
        0 => C::Always,
        1 => C::Less,
        2 => C::LessEqual,
        3 => C::Equal,
        4 => C::NotEqual,
        5 => C::GreaterEqual,
        6 => C::Greater,
        7 => C::Never,
        _ => return None,
    })
}

/// Vanilla's Vulkan mapping: lines and quads arrive as index-expanded triangles; fans are converted.
pub fn topology(code: u32) -> Option<wgpu::PrimitiveTopology> {
    use wgpu::PrimitiveTopology as P;
    Some(match code {
        0 | 4 | 6 | 7 => P::TriangleList,
        1 => P::LineList,
        2 => P::LineStrip,
        3 => P::PointList,
        5 => P::TriangleStrip,
        _ => return None,
    })
}

pub fn address_mode(code: u32) -> wgpu::AddressMode {
    if code == 0 {
        wgpu::AddressMode::Repeat
    } else {
        wgpu::AddressMode::ClampToEdge
    }
}

pub fn filter_mode(code: u32) -> wgpu::FilterMode {
    if code == 0 {
        wgpu::FilterMode::Nearest
    } else {
        wgpu::FilterMode::Linear
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_storable_format_has_a_matching_block_size() {
        for code in 0..GPU_FORMATS.len() as u32 {
            let name = GPU_FORMATS[code as usize];
            let size = block_size(code).unwrap();
            if let Some(f) = texture_format(code) {
                if let Some(bytes) = f.block_copy_size(None) {
                    assert_eq!(bytes, size, "{name}");
                }
            } else {
                assert!(
                    name.starts_with("RGB") && !name.starts_with("RGBA"),
                    "{name}"
                );
            }
            if let Some((v, bytes)) = vertex_format(code) {
                assert_eq!(bytes, size, "{name}");
                assert!(v.size() >= u64::from(size), "{name}");
            }
        }
        assert_eq!(block_size(GPU_FORMATS.len() as u32), None);
    }

    #[test]
    fn widened_vertex_formats_stay_inside_padded_attributes() {
        assert_eq!(vertex_format(5), Some((wgpu::VertexFormat::Snorm8x4, 3)));
        assert_eq!(vertex_format(46), Some((wgpu::VertexFormat::Float32x3, 12)));
        assert_eq!(
            topology(TOPOLOGY_TRIANGLE_FAN),
            Some(wgpu::PrimitiveTopology::TriangleList)
        );
    }

    #[test]
    fn state_tables_map_by_name() {
        use wgpu::{
            BlendFactor as B, BlendOperation as O, CompareFunction as C, PrimitiveTopology as P,
        };
        let factors = [
            ("CONSTANT_ALPHA", B::Constant),
            ("CONSTANT_COLOR", B::Constant),
            ("DST_ALPHA", B::DstAlpha),
            ("DST_COLOR", B::Dst),
            ("ONE", B::One),
            ("ONE_MINUS_CONSTANT_ALPHA", B::OneMinusConstant),
            ("ONE_MINUS_CONSTANT_COLOR", B::OneMinusConstant),
            ("ONE_MINUS_DST_ALPHA", B::OneMinusDstAlpha),
            ("ONE_MINUS_DST_COLOR", B::OneMinusDst),
            ("ONE_MINUS_SRC_ALPHA", B::OneMinusSrcAlpha),
            ("ONE_MINUS_SRC_COLOR", B::OneMinusSrc),
            ("SRC_ALPHA", B::SrcAlpha),
            ("SRC_ALPHA_SATURATE", B::SrcAlphaSaturated),
            ("SRC_COLOR", B::Src),
            ("ZERO", B::Zero),
        ];
        for (name, want) in factors {
            let code = BLEND_FACTORS.iter().position(|n| *n == name).unwrap() as u32;
            assert_eq!(blend_factor(code), Some(want), "{name}");
        }
        let ops = [
            ("ADD", O::Add),
            ("SUBTRACT", O::Subtract),
            ("REVERSE_SUBTRACT", O::ReverseSubtract),
            ("MIN", O::Min),
            ("MAX", O::Max),
        ];
        for (name, want) in ops {
            let code = BLEND_OPS.iter().position(|n| *n == name).unwrap() as u32;
            assert_eq!(blend_op(code), Some(want), "{name}");
        }
        let compares = [
            ("ALWAYS_PASS", C::Always),
            ("LESS_THAN", C::Less),
            ("LESS_THAN_OR_EQUAL", C::LessEqual),
            ("EQUAL", C::Equal),
            ("NOT_EQUAL", C::NotEqual),
            ("GREATER_THAN_OR_EQUAL", C::GreaterEqual),
            ("GREATER_THAN", C::Greater),
            ("NEVER_PASS", C::Never),
        ];
        for (name, want) in compares {
            let code = COMPARE_OPS.iter().position(|n| *n == name).unwrap() as u32;
            assert_eq!(compare(code), Some(want), "{name}");
        }
        let topologies = [
            ("LINES", P::TriangleList),
            ("DEBUG_LINES", P::LineList),
            ("DEBUG_LINE_STRIP", P::LineStrip),
            ("POINTS", P::PointList),
            ("TRIANGLES", P::TriangleList),
            ("TRIANGLE_STRIP", P::TriangleStrip),
            ("TRIANGLE_FAN", P::TriangleList),
            ("QUADS", P::TriangleList),
        ];
        for (name, want) in topologies {
            let code = TOPOLOGIES.iter().position(|n| *n == name).unwrap() as u32;
            assert_eq!(topology(code), Some(want), "{name}");
        }
        assert_eq!(
            (blend_factor(99), blend_op(99), compare(99), topology(99)),
            (None, None, None, None)
        );
        assert_eq!(address_mode(0), wgpu::AddressMode::Repeat);
        assert_eq!(address_mode(1), wgpu::AddressMode::ClampToEdge);
        assert_eq!(filter_mode(1), wgpu::FilterMode::Linear);
        assert_eq!(
            texture_format(53),
            Some(wgpu::TextureFormat::Depth24PlusStencil8)
        );
        assert_eq!(texture_format(4), None, "RGB texture formats are rejected");
    }
}
