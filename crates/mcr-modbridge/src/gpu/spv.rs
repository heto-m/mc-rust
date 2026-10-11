//! Word-level SPIR-V rewrite that makes vanilla's shaders consumable by naga and wgpu.
//!
//! Vanilla's frontend binds every resource at set 0, binding = flattened uniform index `i`. This pass:
//! - moves uniform buffers to `2i`;
//! - splits each combined image sampler into an image at `2i` and a new sampler at `2i + 1` (naga has
//!   no combined samplers), including sampler function parameters;
//! - turns the push-constant block into a uniform block at `2n`, the way the GL backend emulates them;
//! - turns output arrays (OIT's `out vec4 coeff[N]`, which naga cannot express) into a private array
//!   copied into `N` consecutive outputs before every return of the entry point;
//! - drops debug-only instructions (naga rejects `OpLine` between type declarations).

use rustc_hash::{FxHashMap, FxHashSet};

const MAGIC: u32 = 0x0723_0203;
const HEADER_WORDS: usize = 5;
/// SPIR-V 1.4 widened `OpEntryPoint` interfaces to every global the entry point uses.
const VERSION_1_4: u32 = 0x0001_0400;
/// No vanilla shader declares more outputs than this in one array.
const MAX_OUTPUT_ARRAY: u32 = 8;

const OP_SOURCE_CONTINUED: u16 = 2;
const OP_SOURCE: u16 = 3;
const OP_SOURCE_EXTENSION: u16 = 4;
const OP_NAME: u16 = 5;
const OP_MEMBER_NAME: u16 = 6;
const OP_STRING: u16 = 7;
const OP_LINE: u16 = 8;
const OP_ENTRY_POINT: u16 = 15;
const OP_TYPE_SAMPLER: u16 = 26;
const OP_TYPE_SAMPLED_IMAGE: u16 = 27;
const OP_TYPE_ARRAY: u16 = 28;
const OP_TYPE_POINTER: u16 = 32;
const OP_TYPE_FUNCTION: u16 = 33;
const OP_CONSTANT: u16 = 43;
const OP_FUNCTION: u16 = 54;
const OP_FUNCTION_PARAMETER: u16 = 55;
const OP_FUNCTION_END: u16 = 56;
const OP_FUNCTION_CALL: u16 = 57;
const OP_VARIABLE: u16 = 59;
const OP_LOAD: u16 = 61;
const OP_STORE: u16 = 62;
const OP_COPY_MEMORY: u16 = 63;
const OP_COPY_MEMORY_SIZED: u16 = 64;
const OP_ACCESS_CHAIN: u16 = 65;
const OP_IN_BOUNDS_ACCESS_CHAIN: u16 = 66;
const OP_PTR_ACCESS_CHAIN: u16 = 67;
const OP_DECORATE: u16 = 71;
const OP_MEMBER_DECORATE: u16 = 72;
const OP_COPY_OBJECT: u16 = 83;
const OP_SAMPLED_IMAGE: u16 = 86;
const OP_SELECT: u16 = 169;
const OP_PHI: u16 = 245;
const OP_RETURN: u16 = 253;
const OP_RETURN_VALUE: u16 = 254;
const OP_NO_LINE: u16 = 317;
const OP_MODULE_PROCESSED: u16 = 330;

const DECORATION_LOCATION: u32 = 30;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;

const STORAGE_UNIFORM_CONSTANT: u32 = 0;
const STORAGE_OUTPUT: u32 = 3;
const STORAGE_UNIFORM: u32 = 2;
const STORAGE_PRIVATE: u32 = 6;
const STORAGE_PUSH_CONSTANT: u32 = 9;

/// How the frontend declared the uniform at each flattened index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniformKind {
    Buffer,
    CombinedSampler,
    TexelBuffer,
}

pub fn buffer_binding(index: u32) -> u32 {
    index * 2
}

pub fn texture_binding(index: u32) -> u32 {
    index * 2
}

pub fn sampler_binding(index: u32) -> u32 {
    index * 2 + 1
}

/// The push-constant block sits after every declared uniform.
pub fn push_constant_binding(uniform_count: u32) -> u32 {
    uniform_count * 2
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpvError {
    #[error("not SPIR-V (bad magic or truncated header)")]
    Header,
    #[error("instruction at word {0} is malformed")]
    Malformed(usize),
    #[error("binding {0} has no uniform description")]
    UnknownBinding(u32),
    #[error("binding {binding} is {found:?} in the shader but {declared:?} in the pipeline")]
    KindMismatch {
        binding: u32,
        found: UniformKind,
        declared: UniformKind,
    },
    #[error("texel buffer uniforms are not supported")]
    TexelBuffer,
    #[error("descriptor set {0} is not 0")]
    DescriptorSet(u32),
    #[error("combined sampler %{0} is used by opcode {1}, which the rewrite does not handle")]
    UnsupportedUse(u32, u16),
    #[error("output array %{0} is unsupported (length or location)")]
    OutputArray(u32),
    #[error("the id bound overflowed")]
    Bound,
}

#[derive(Clone, Copy)]
struct Inst {
    start: usize,
    len: usize,
    op: u16,
}

fn instructions(words: &[u32]) -> Result<Vec<Inst>, SpvError> {
    let mut out = Vec::with_capacity(words.len() / 4);
    let mut at = HEADER_WORDS;
    while at < words.len() {
        let len = (words[at] >> 16) as usize;
        let op = (words[at] & 0xFFFF) as u16;
        if len == 0 || at + len > words.len() {
            return Err(SpvError::Malformed(at));
        }
        out.push(Inst { start: at, len, op });
        at += len;
    }
    Ok(out)
}

struct Ids {
    bound: u32,
}

impl Ids {
    fn next(&mut self) -> Result<u32, SpvError> {
        let id = self.bound;
        self.bound = self.bound.checked_add(1).ok_or(SpvError::Bound)?;
        Ok(id)
    }
}

fn emit(out: &mut Vec<u32>, op: u16, operands: &[u32]) {
    out.push(((operands.len() as u32 + 1) << 16) | u32::from(op));
    out.extend_from_slice(operands);
}

/// A pointer type to a combined sampler and the image/sampler pointer pair replacing it.
struct SplitPointer {
    image_type: u32,
    image_ptr: u32,
}

/// An `Output` array variable rewritten as a private array plus one output per element.
struct OutputArray {
    array_ptr: u32,
    elem: u32,
    index_type: u32,
    private_array_ptr: u32,
    private_elem_ptr: u32,
    output_elem_ptr: u32,
    indices: Vec<u32>,
    outputs: Vec<u32>,
    location: u32,
}

pub fn fixup(input: &[u32], uniforms: &[UniformKind]) -> Result<Vec<u32>, SpvError> {
    if input.len() < HEADER_WORDS || input[0] != MAGIC {
        return Err(SpvError::Header);
    }
    let version = input[1];
    let insts = instructions(input)?;
    let mut ids = Ids { bound: input[3] };
    let w = |i: &Inst, k: usize| input[i.start + k];

    let mut sampled_image_types: FxHashMap<u32, u32> = FxHashMap::default();
    let mut sampler_type: Option<u32> = None;
    let mut pointers: FxHashMap<u32, (u32, u32)> = FxHashMap::default();
    let mut arrays: FxHashMap<u32, (u32, u32)> = FxHashMap::default();
    let mut constants: FxHashMap<u32, (u32, u32)> = FxHashMap::default();
    let mut split_ptrs: FxHashMap<u32, SplitPointer> = FxHashMap::default();
    let mut first_split_ptr: Option<usize> = None;
    let mut bindings: FxHashMap<u32, u32> = FxHashMap::default();
    let mut locations: FxHashMap<u32, u32> = FxHashMap::default();
    let mut entry_fn: Option<u32> = None;
    for (n, i) in insts.iter().enumerate() {
        match i.op {
            OP_TYPE_SAMPLED_IMAGE if i.len == 3 => {
                sampled_image_types.insert(w(i, 1), w(i, 2));
            }
            OP_TYPE_SAMPLER if i.len == 2 && first_split_ptr.is_none() => {
                sampler_type.get_or_insert(w(i, 1));
            }
            OP_TYPE_ARRAY if i.len == 4 => {
                arrays.insert(w(i, 1), (w(i, 2), w(i, 3)));
            }
            OP_CONSTANT if i.len == 4 => {
                constants.insert(w(i, 2), (w(i, 1), w(i, 3)));
            }
            OP_TYPE_POINTER if i.len == 4 => {
                pointers.insert(w(i, 1), (w(i, 2), w(i, 3)));
                if w(i, 2) == STORAGE_UNIFORM_CONSTANT
                    && let Some(&image_type) = sampled_image_types.get(&w(i, 3))
                {
                    first_split_ptr.get_or_insert(n);
                    split_ptrs.insert(
                        w(i, 1),
                        SplitPointer {
                            image_type,
                            image_ptr: 0,
                        },
                    );
                }
            }
            OP_DECORATE if i.len >= 4 => match w(i, 2) {
                DECORATION_BINDING => {
                    bindings.insert(w(i, 1), w(i, 3));
                }
                DECORATION_LOCATION => {
                    locations.insert(w(i, 1), w(i, 3));
                }
                DECORATION_DESCRIPTOR_SET if w(i, 3) != 0 => {
                    return Err(SpvError::DescriptorSet(w(i, 3)));
                }
                _ => {}
            },
            OP_ENTRY_POINT if i.len >= 3 => {
                entry_fn.get_or_insert(w(i, 2));
            }
            _ => {}
        }
    }

    let needs_sampler_type = !split_ptrs.is_empty() && sampler_type.is_none();
    let sampler_type = match sampler_type {
        Some(t) => t,
        None if needs_sampler_type => ids.next()?,
        None => 0,
    };
    let sampler_ptr = if split_ptrs.is_empty() {
        0
    } else {
        ids.next()?
    };
    let mut image_ptrs: FxHashMap<u32, u32> = FxHashMap::default();
    let mut ptr_order: Vec<u32> = split_ptrs.keys().copied().collect();
    ptr_order.sort_unstable();
    for ptr in ptr_order {
        let split = split_ptrs.get_mut(&ptr).expect("key from the same map");
        split.image_ptr = match image_ptrs.get(&split.image_type) {
            Some(&p) => p,
            None => {
                let p = ids.next()?;
                image_ptrs.insert(split.image_type, p);
                p
            }
        };
    }

    // Combined-sampler pointers (globals and parameters) -> their sampler half.
    let mut split_values: FxHashMap<u32, u32> = FxHashMap::default();
    let mut new_bindings: FxHashMap<u32, u32> = FxHashMap::default();
    let mut sampler_vars: Vec<(u32, u32)> = Vec::new();
    let mut push_var: Option<u32> = None;
    let mut output_arrays: FxHashMap<u32, OutputArray> = FxHashMap::default();
    for i in &insts {
        match i.op {
            OP_VARIABLE if i.len >= 4 => {
                let (ty, id, storage) = (w(i, 1), w(i, 2), w(i, 3));
                match storage {
                    STORAGE_UNIFORM_CONSTANT | STORAGE_UNIFORM => {
                        let Some(&binding) = bindings.get(&id) else {
                            continue;
                        };
                        let declared = *uniforms
                            .get(binding as usize)
                            .ok_or(SpvError::UnknownBinding(binding))?;
                        let found = if split_ptrs.contains_key(&ty) {
                            UniformKind::CombinedSampler
                        } else if storage == STORAGE_UNIFORM {
                            UniformKind::Buffer
                        } else {
                            UniformKind::TexelBuffer
                        };
                        if declared == UniformKind::TexelBuffer || found == UniformKind::TexelBuffer
                        {
                            return Err(SpvError::TexelBuffer);
                        }
                        if found != declared {
                            return Err(SpvError::KindMismatch {
                                binding,
                                found,
                                declared,
                            });
                        }
                        if found == UniformKind::CombinedSampler {
                            let sampler = ids.next()?;
                            split_values.insert(id, sampler);
                            sampler_vars.push((sampler, sampler_binding(binding)));
                            new_bindings.insert(id, texture_binding(binding));
                        } else {
                            new_bindings.insert(id, buffer_binding(binding));
                        }
                    }
                    STORAGE_PUSH_CONSTANT => push_var = Some(id),
                    STORAGE_OUTPUT => {
                        let Some(&(_, pointee)) = pointers.get(&ty) else {
                            continue;
                        };
                        let Some(&(elem, len_const)) = arrays.get(&pointee) else {
                            continue;
                        };
                        let (index_type, len) = constants
                            .get(&len_const)
                            .copied()
                            .ok_or(SpvError::OutputArray(id))?;
                        let location = locations.get(&id).copied();
                        let (Some(location), 1..=MAX_OUTPUT_ARRAY) = (location, len) else {
                            return Err(SpvError::OutputArray(id));
                        };
                        let mut indices = Vec::with_capacity(len as usize);
                        let mut outputs = Vec::with_capacity(len as usize);
                        for _ in 0..len {
                            indices.push(ids.next()?);
                            outputs.push(ids.next()?);
                        }
                        output_arrays.insert(
                            id,
                            OutputArray {
                                array_ptr: ty,
                                elem,
                                index_type,
                                private_array_ptr: ids.next()?,
                                private_elem_ptr: ids.next()?,
                                output_elem_ptr: ids.next()?,
                                indices,
                                outputs,
                                location,
                            },
                        );
                    }
                    _ => {}
                }
            }
            OP_FUNCTION_PARAMETER if i.len == 3 && split_ptrs.contains_key(&w(i, 1)) => {
                split_values.insert(w(i, 2), ids.next()?);
            }
            _ => {}
        }
    }

    // Access chains into an output array need private pointer types for their results.
    let mut private_ptrs: FxHashMap<u32, (u32, u32)> = FxHashMap::default();
    for i in &insts {
        if matches!(i.op, OP_ACCESS_CHAIN | OP_IN_BOUNDS_ACCESS_CHAIN)
            && i.len >= 4
            && output_arrays.contains_key(&w(i, 3))
            && !private_ptrs.contains_key(&w(i, 1))
        {
            let pointee = pointers
                .get(&w(i, 1))
                .map(|p| p.1)
                .ok_or(SpvError::OutputArray(w(i, 3)))?;
            private_ptrs.insert(w(i, 1), (ids.next()?, pointee));
        }
    }

    let last_decoration = insts
        .iter()
        .rposition(|i| matches!(i.op, OP_DECORATE | OP_MEMBER_DECORATE));
    let mut emitted_image_ptrs: FxHashSet<u32> = FxHashSet::default();
    let mut in_entry = false;
    let mut emitted_private_chains = false;
    let mut out = Vec::with_capacity(input.len() + 64);
    out.extend_from_slice(&input[..HEADER_WORDS]);
    for (n, i) in insts.iter().enumerate() {
        let ops = &input[i.start + 1..i.start + i.len];
        let copy = |out: &mut Vec<u32>| out.extend_from_slice(&input[i.start..i.start + i.len]);
        match i.op {
            OP_SOURCE_CONTINUED | OP_SOURCE | OP_SOURCE_EXTENSION | OP_STRING | OP_LINE
            | OP_NO_LINE | OP_MODULE_PROCESSED => {}
            OP_ENTRY_POINT => {
                let mut operands = Vec::with_capacity(ops.len() + sampler_vars.len());
                let name_end = 2 + ops[2..]
                    .iter()
                    .position(|w| w >> 24 == 0)
                    .map_or(0, |p| p + 1);
                operands.extend_from_slice(&ops[..name_end.min(ops.len())]);
                for &var in ops.iter().skip(name_end) {
                    match output_arrays.get(&var) {
                        Some(arr) => operands.extend_from_slice(&arr.outputs),
                        None => operands.push(var),
                    }
                }
                if version >= VERSION_1_4 {
                    operands.extend(sampler_vars.iter().map(|&(id, _)| id));
                }
                emit(&mut out, i.op, &operands);
            }
            OP_DECORATE if ops.len() >= 3 && ops[1] == DECORATION_BINDING => {
                let binding = new_bindings.get(&ops[0]).copied().unwrap_or(ops[2]);
                emit(&mut out, i.op, &[ops[0], ops[1], binding]);
            }
            OP_DECORATE
                if ops.len() >= 2
                    && ops[1] == DECORATION_LOCATION
                    && output_arrays.contains_key(&ops[0]) => {}
            OP_TYPE_POINTER if ops.len() == 3 && ops[1] == STORAGE_PUSH_CONSTANT => {
                emit(&mut out, i.op, &[ops[0], STORAGE_UNIFORM, ops[2]]);
            }
            OP_TYPE_POINTER if ops.len() == 3 && split_ptrs.contains_key(&ops[0]) => {
                copy(&mut out);
                if Some(n) == first_split_ptr {
                    if needs_sampler_type {
                        emit(&mut out, OP_TYPE_SAMPLER, &[sampler_type]);
                    }
                    emit(
                        &mut out,
                        OP_TYPE_POINTER,
                        &[sampler_ptr, STORAGE_UNIFORM_CONSTANT, sampler_type],
                    );
                }
                let split = &split_ptrs[&ops[0]];
                if emitted_image_ptrs.insert(split.image_ptr) {
                    emit(
                        &mut out,
                        OP_TYPE_POINTER,
                        &[split.image_ptr, STORAGE_UNIFORM_CONSTANT, split.image_type],
                    );
                }
            }
            OP_TYPE_POINTER if ops.len() == 3 => {
                copy(&mut out);
                let mut arrays: Vec<&OutputArray> = output_arrays
                    .values()
                    .filter(|a| a.array_ptr == ops[0])
                    .collect();
                arrays.sort_unstable_by_key(|a| a.private_array_ptr);
                let first = arrays.first().map(|a| a.private_array_ptr);
                for arr in arrays {
                    emit(
                        &mut out,
                        OP_TYPE_POINTER,
                        &[arr.private_array_ptr, STORAGE_PRIVATE, ops[2]],
                    );
                    emit(
                        &mut out,
                        OP_TYPE_POINTER,
                        &[arr.private_elem_ptr, STORAGE_PRIVATE, arr.elem],
                    );
                    emit(
                        &mut out,
                        OP_TYPE_POINTER,
                        &[arr.output_elem_ptr, STORAGE_OUTPUT, arr.elem],
                    );
                    for (k, &index) in arr.indices.iter().enumerate() {
                        emit(&mut out, OP_CONSTANT, &[arr.index_type, index, k as u32]);
                    }
                    if Some(arr.private_array_ptr) == first && !emitted_private_chains {
                        emitted_private_chains = true;
                        let mut chains: Vec<(u32, u32)> = private_ptrs.values().copied().collect();
                        chains.sort_unstable();
                        for (ptr, pointee) in chains {
                            emit(&mut out, OP_TYPE_POINTER, &[ptr, STORAGE_PRIVATE, pointee]);
                        }
                    }
                }
            }
            OP_TYPE_FUNCTION => {
                let mut operands = Vec::with_capacity(ops.len() + 2);
                operands.extend_from_slice(&ops[..2.min(ops.len())]);
                for &param in ops.iter().skip(2) {
                    match split_ptrs.get(&param) {
                        Some(split) => {
                            operands.push(split.image_ptr);
                            operands.push(sampler_ptr);
                        }
                        None => operands.push(param),
                    }
                }
                emit(&mut out, i.op, &operands);
            }
            OP_VARIABLE if ops.len() >= 3 => {
                let (ty, id, storage) = (ops[0], ops[1], ops[2]);
                if let (Some(split), Some(&sampler)) = (split_ptrs.get(&ty), split_values.get(&id))
                {
                    emit(&mut out, i.op, &[split.image_ptr, id, storage]);
                    emit(
                        &mut out,
                        i.op,
                        &[sampler_ptr, sampler, STORAGE_UNIFORM_CONSTANT],
                    );
                } else if let Some(arr) = output_arrays.get(&id) {
                    emit(
                        &mut out,
                        i.op,
                        &[arr.private_array_ptr, id, STORAGE_PRIVATE],
                    );
                    for &output in &arr.outputs {
                        emit(
                            &mut out,
                            i.op,
                            &[arr.output_elem_ptr, output, STORAGE_OUTPUT],
                        );
                    }
                } else if storage == STORAGE_PUSH_CONSTANT {
                    let mut operands = ops.to_vec();
                    operands[2] = STORAGE_UNIFORM;
                    emit(&mut out, i.op, &operands);
                } else {
                    copy(&mut out);
                }
            }
            OP_FUNCTION => {
                in_entry = ops.len() >= 2 && Some(ops[1]) == entry_fn;
                copy(&mut out);
            }
            OP_FUNCTION_END => {
                in_entry = false;
                copy(&mut out);
            }
            OP_FUNCTION_PARAMETER if ops.len() == 2 => match split_ptrs.get(&ops[0]) {
                Some(split) => {
                    emit(&mut out, i.op, &[split.image_ptr, ops[1]]);
                    emit(&mut out, i.op, &[sampler_ptr, split_values[&ops[1]]]);
                }
                None => copy(&mut out),
            },
            OP_LOAD if ops.len() >= 3 && split_values.contains_key(&ops[2]) => {
                let (result_type, result, pointer) = (ops[0], ops[1], ops[2]);
                let image_type = *sampled_image_types
                    .get(&result_type)
                    .ok_or(SpvError::UnsupportedUse(pointer, i.op))?;
                let image = ids.next()?;
                let sampler = ids.next()?;
                emit(&mut out, OP_LOAD, &[image_type, image, pointer]);
                emit(
                    &mut out,
                    OP_LOAD,
                    &[sampler_type, sampler, split_values[&pointer]],
                );
                emit(
                    &mut out,
                    OP_SAMPLED_IMAGE,
                    &[result_type, result, image, sampler],
                );
            }
            OP_ACCESS_CHAIN | OP_IN_BOUNDS_ACCESS_CHAIN
                if ops.len() >= 3 && output_arrays.contains_key(&ops[2]) =>
            {
                let mut operands = ops.to_vec();
                operands[0] = private_ptrs[&ops[0]].0;
                emit(&mut out, i.op, &operands);
            }
            OP_FUNCTION_CALL if ops.len() >= 3 => {
                let mut operands = Vec::with_capacity(ops.len() + 2);
                operands.extend_from_slice(&ops[..3]);
                for &arg in &ops[3..] {
                    operands.push(arg);
                    if let Some(&sampler) = split_values.get(&arg) {
                        operands.push(sampler);
                    }
                }
                emit(&mut out, i.op, &operands);
            }
            OP_RETURN if in_entry && !output_arrays.is_empty() => {
                let mut vars: Vec<(&u32, &OutputArray)> = output_arrays.iter().collect();
                vars.sort_unstable_by_key(|(id, _)| **id);
                for (&var, arr) in vars {
                    for (&index, &output) in arr.indices.iter().zip(&arr.outputs) {
                        let element = ids.next()?;
                        let value = ids.next()?;
                        emit(
                            &mut out,
                            OP_ACCESS_CHAIN,
                            &[arr.private_elem_ptr, element, var, index],
                        );
                        emit(&mut out, OP_LOAD, &[arr.elem, value, element]);
                        emit(&mut out, OP_STORE, &[output, value]);
                    }
                }
                copy(&mut out);
            }
            OP_STORE
            | OP_COPY_MEMORY
            | OP_COPY_MEMORY_SIZED
            | OP_ACCESS_CHAIN
            | OP_IN_BOUNDS_ACCESS_CHAIN
            | OP_PTR_ACCESS_CHAIN
            | OP_COPY_OBJECT
            | OP_SELECT
            | OP_PHI
            | OP_RETURN_VALUE => {
                let operands = if matches!(
                    i.op,
                    OP_STORE | OP_COPY_MEMORY | OP_COPY_MEMORY_SIZED | OP_RETURN_VALUE
                ) {
                    ops
                } else {
                    &ops[2.min(ops.len())..]
                };
                if let Some(&id) = operands.iter().find(|w| split_values.contains_key(w)) {
                    return Err(SpvError::UnsupportedUse(id, i.op));
                }
                copy(&mut out);
            }
            OP_NAME | OP_MEMBER_NAME => copy(&mut out),
            _ => copy(&mut out),
        }
        if Some(n) == last_decoration {
            for &(sampler, binding) in &sampler_vars {
                emit(
                    &mut out,
                    OP_DECORATE,
                    &[sampler, DECORATION_DESCRIPTOR_SET, 0],
                );
                emit(
                    &mut out,
                    OP_DECORATE,
                    &[sampler, DECORATION_BINDING, binding],
                );
            }
            if let Some(var) = push_var {
                let binding = push_constant_binding(uniforms.len() as u32);
                emit(&mut out, OP_DECORATE, &[var, DECORATION_DESCRIPTOR_SET, 0]);
                emit(&mut out, OP_DECORATE, &[var, DECORATION_BINDING, binding]);
            }
            let mut arrays: Vec<&OutputArray> = output_arrays.values().collect();
            arrays.sort_unstable_by_key(|a| a.location);
            for arr in arrays {
                for (k, &output) in arr.outputs.iter().enumerate() {
                    emit(
                        &mut out,
                        OP_DECORATE,
                        &[output, DECORATION_LOCATION, arr.location + k as u32],
                    );
                }
            }
        }
    }
    out[3] = ids.bound;
    Ok(out)
}

/// Reads a little-endian SPIR-V byte stream into words.
pub fn words(bytes: &[u8]) -> Result<Vec<u32>, SpvError> {
    if !bytes.len().is_multiple_of(4) || bytes.len() < HEADER_WORDS * 4 {
        return Err(SpvError::Header);
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect())
}

/// A minimal SPIR-V writer for hand-written test modules.
#[cfg(test)]
pub(crate) mod asm {
    pub const CAPABILITY: u16 = 17;
    pub const MEMORY_MODEL: u16 = 14;
    pub const ENTRY_POINT: u16 = 15;
    pub const EXECUTION_MODE: u16 = 16;
    pub const DECORATE: u16 = 71;
    pub const MEMBER_DECORATE: u16 = 72;
    pub const TYPE_VOID: u16 = 19;
    pub const TYPE_INT: u16 = 21;
    pub const TYPE_FLOAT: u16 = 22;
    pub const TYPE_VECTOR: u16 = 23;
    pub const TYPE_IMAGE: u16 = 25;
    pub const TYPE_SAMPLED_IMAGE: u16 = 27;
    pub const TYPE_STRUCT: u16 = 30;
    pub const TYPE_POINTER: u16 = 32;
    pub const TYPE_FUNCTION: u16 = 33;
    pub const CONSTANT: u16 = 43;
    pub const FUNCTION: u16 = 54;
    pub const FUNCTION_PARAMETER: u16 = 55;
    pub const FUNCTION_END: u16 = 56;
    pub const FUNCTION_CALL: u16 = 57;
    pub const VARIABLE: u16 = 59;
    pub const LOAD: u16 = 61;
    pub const STORE: u16 = 62;
    pub const ACCESS_CHAIN: u16 = 65;
    pub const COMPOSITE_CONSTRUCT: u16 = 80;
    pub const COMPOSITE_EXTRACT: u16 = 81;
    pub const IMAGE_SAMPLE_IMPLICIT_LOD: u16 = 87;
    pub const FADD: u16 = 129;
    pub const FMUL: u16 = 133;
    pub const LABEL: u16 = 248;
    pub const RETURN: u16 = 253;
    pub const RETURN_VALUE: u16 = 254;

    pub const BLOCK: u32 = 2;
    pub const BUILTIN: u32 = 11;
    pub const OFFSET: u32 = 35;

    pub struct Asm {
        words: Vec<u32>,
        next: u32,
    }

    impl Asm {
        pub fn new() -> Self {
            let mut a = Self {
                words: vec![super::MAGIC, 0x0001_0000, 0, 0, 0],
                next: 1,
            };
            a.op(CAPABILITY, &[1]);
            a.op(MEMORY_MODEL, &[0, 1]);
            a
        }

        pub fn id(&mut self) -> u32 {
            self.next += 1;
            self.next - 1
        }

        pub fn op(&mut self, op: u16, operands: &[u32]) {
            super::emit(&mut self.words, op, operands);
        }

        /// `OpEntryPoint model %f "main" interface…`
        pub fn entry(&mut self, model: u32, f: u32, interface: &[u32]) {
            let mut ops = vec![model, f, u32::from_le_bytes(*b"main"), 0];
            ops.extend_from_slice(interface);
            self.op(ENTRY_POINT, &ops);
        }

        pub fn finish(mut self) -> Vec<u32> {
            self.words[3] = self.next;
            self.words
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::asm::*;
    use super::*;

    struct Ids {
        tex: u32,
        ubo: u32,
        push: u32,
    }

    /// `out = sample(tex, uv) * ubo.color + push.add`, sampling through a helper taking `sampler2D`.
    fn fragment(set: u32) -> (Vec<u32>, Ids) {
        let mut a = Asm::new();
        let main = a.id();
        let (out, uv, tex, ubo, push) = (a.id(), a.id(), a.id(), a.id(), a.id());
        let (blk, pblk) = (a.id(), a.id());
        a.entry(4, main, &[out, uv]);
        a.op(EXECUTION_MODE, &[main, 7]);
        a.op(DECORATE, &[tex, DECORATION_DESCRIPTOR_SET, set]);
        a.op(DECORATE, &[tex, DECORATION_BINDING, 1]);
        a.op(DECORATE, &[ubo, DECORATION_DESCRIPTOR_SET, 0]);
        a.op(DECORATE, &[ubo, DECORATION_BINDING, 0]);
        a.op(DECORATE, &[blk, BLOCK]);
        a.op(MEMBER_DECORATE, &[blk, 0, OFFSET, 0]);
        a.op(DECORATE, &[pblk, BLOCK]);
        a.op(MEMBER_DECORATE, &[pblk, 0, OFFSET, 0]);
        a.op(DECORATE, &[out, DECORATION_LOCATION, 0]);
        a.op(DECORATE, &[uv, DECORATION_LOCATION, 0]);
        let (void, fnty, f32t, v4, v2, int) = (a.id(), a.id(), a.id(), a.id(), a.id(), a.id());
        a.op(TYPE_VOID, &[void]);
        a.op(TYPE_FUNCTION, &[fnty, void]);
        a.op(TYPE_FLOAT, &[f32t, 32]);
        a.op(TYPE_VECTOR, &[v4, f32t, 4]);
        a.op(TYPE_VECTOR, &[v2, f32t, 2]);
        a.op(TYPE_INT, &[int, 32, 1]);
        let (img, simg, psimg) = (a.id(), a.id(), a.id());
        a.op(TYPE_IMAGE, &[img, f32t, 1, 0, 0, 0, 1, 0]);
        a.op(TYPE_SAMPLED_IMAGE, &[simg, img]);
        a.op(TYPE_POINTER, &[psimg, STORAGE_UNIFORM_CONSTANT, simg]);
        a.op(VARIABLE, &[psimg, tex, STORAGE_UNIFORM_CONSTANT]);
        let (pubo, ppush, pout, pin, puv4, ppv4) = (a.id(), a.id(), a.id(), a.id(), a.id(), a.id());
        a.op(TYPE_STRUCT, &[blk, v4]);
        a.op(TYPE_POINTER, &[pubo, STORAGE_UNIFORM, blk]);
        a.op(VARIABLE, &[pubo, ubo, STORAGE_UNIFORM]);
        a.op(TYPE_STRUCT, &[pblk, v4]);
        a.op(TYPE_POINTER, &[ppush, STORAGE_PUSH_CONSTANT, pblk]);
        a.op(VARIABLE, &[ppush, push, STORAGE_PUSH_CONSTANT]);
        a.op(TYPE_POINTER, &[pout, STORAGE_OUTPUT, v4]);
        a.op(VARIABLE, &[pout, out, STORAGE_OUTPUT]);
        a.op(TYPE_POINTER, &[pin, 1, v2]);
        a.op(VARIABLE, &[pin, uv, 1]);
        a.op(TYPE_POINTER, &[puv4, STORAGE_UNIFORM, v4]);
        a.op(TYPE_POINTER, &[ppv4, STORAGE_PUSH_CONSTANT, v4]);
        let c0 = a.id();
        a.op(CONSTANT, &[int, c0, 0]);
        let helper_ty = a.id();
        a.op(TYPE_FUNCTION, &[helper_ty, v4, psimg]);

        let (helper, param, hl, hs, hc, hr) = (a.id(), a.id(), a.id(), a.id(), a.id(), a.id());
        a.op(FUNCTION, &[v4, helper, 0, helper_ty]);
        a.op(FUNCTION_PARAMETER, &[psimg, param]);
        a.op(LABEL, &[hl]);
        a.op(LOAD, &[simg, hs, param]);
        a.op(LOAD, &[v2, hc, uv]);
        a.op(IMAGE_SAMPLE_IMPLICIT_LOD, &[v4, hr, hs, hc]);
        a.op(RETURN_VALUE, &[hr]);
        a.op(FUNCTION_END, &[]);

        let (l, c, p, u, q, pc, m, r) = (
            a.id(),
            a.id(),
            a.id(),
            a.id(),
            a.id(),
            a.id(),
            a.id(),
            a.id(),
        );
        a.op(FUNCTION, &[void, main, 0, fnty]);
        a.op(LABEL, &[l]);
        a.op(FUNCTION_CALL, &[v4, c, helper, tex]);
        a.op(ACCESS_CHAIN, &[puv4, p, ubo, c0]);
        a.op(LOAD, &[v4, u, p]);
        a.op(ACCESS_CHAIN, &[ppv4, q, push, c0]);
        a.op(LOAD, &[v4, pc, q]);
        a.op(FMUL, &[v4, m, c, u]);
        a.op(FADD, &[v4, r, m, pc]);
        a.op(STORE, &[out, r]);
        a.op(RETURN, &[]);
        a.op(FUNCTION_END, &[]);
        (a.finish(), Ids { tex, ubo, push })
    }

    /// The fragment shader the GPU tests draw with (uniform 0 = color block, 1 = texture).
    pub(crate) fn test_fragment() -> Vec<u32> {
        fragment(0).0
    }

    /// `gl_Position = vec4(pos, 0, 1)`, passing `uv` (location 1) through to location 0.
    pub(crate) fn test_vertex() -> Vec<u32> {
        let mut a = Asm::new();
        let main = a.id();
        let (in_pos, in_uv, out_uv, glpos) = (a.id(), a.id(), a.id(), a.id());
        a.entry(0, main, &[in_pos, in_uv, out_uv, glpos]);
        a.op(DECORATE, &[in_pos, DECORATION_LOCATION, 0]);
        a.op(DECORATE, &[in_uv, DECORATION_LOCATION, 1]);
        a.op(DECORATE, &[out_uv, DECORATION_LOCATION, 0]);
        a.op(DECORATE, &[glpos, BUILTIN, 0]);
        let (void, fnty, f32t, v2, v4) = (a.id(), a.id(), a.id(), a.id(), a.id());
        a.op(TYPE_VOID, &[void]);
        a.op(TYPE_FUNCTION, &[fnty, void]);
        a.op(TYPE_FLOAT, &[f32t, 32]);
        a.op(TYPE_VECTOR, &[v2, f32t, 2]);
        a.op(TYPE_VECTOR, &[v4, f32t, 4]);
        let (pin, pout2, pout4) = (a.id(), a.id(), a.id());
        a.op(TYPE_POINTER, &[pin, 1, v2]);
        a.op(TYPE_POINTER, &[pout2, STORAGE_OUTPUT, v2]);
        a.op(TYPE_POINTER, &[pout4, STORAGE_OUTPUT, v4]);
        a.op(VARIABLE, &[pin, in_pos, 1]);
        a.op(VARIABLE, &[pin, in_uv, 1]);
        a.op(VARIABLE, &[pout2, out_uv, STORAGE_OUTPUT]);
        a.op(VARIABLE, &[pout4, glpos, STORAGE_OUTPUT]);
        let (c0, c1) = (a.id(), a.id());
        a.op(CONSTANT, &[f32t, c0, 0f32.to_bits()]);
        a.op(CONSTANT, &[f32t, c1, 1f32.to_bits()]);
        let (l, p, x, y, pos, u) = (a.id(), a.id(), a.id(), a.id(), a.id(), a.id());
        a.op(FUNCTION, &[void, main, 0, fnty]);
        a.op(LABEL, &[l]);
        a.op(LOAD, &[v2, p, in_pos]);
        a.op(COMPOSITE_EXTRACT, &[f32t, x, p, 0]);
        a.op(COMPOSITE_EXTRACT, &[f32t, y, p, 1]);
        a.op(COMPOSITE_CONSTRUCT, &[v4, pos, x, y, c0, c1]);
        a.op(STORE, &[glpos, pos]);
        a.op(LOAD, &[v2, u, in_uv]);
        a.op(STORE, &[out_uv, u]);
        a.op(RETURN, &[]);
        a.op(FUNCTION_END, &[]);
        a.finish()
    }

    fn parse(words: &[u32]) -> naga::Module {
        let module = naga::front::spv::Frontend::new(
            words.iter().copied(),
            &super::super::shader::spv_options(),
        )
        .parse()
        .expect("naga parses the rewritten module");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("the rewritten module validates");
        module
    }

    fn bindings(module: &naga::Module) -> Vec<(u32, &'static str)> {
        let mut out: Vec<(u32, &'static str)> = module
            .global_variables
            .iter()
            .filter_map(|(_, v)| {
                let b = v.binding.as_ref()?;
                let kind = match module.types[v.ty].inner {
                    naga::TypeInner::Image { .. } => "image",
                    naga::TypeInner::Sampler { .. } => "sampler",
                    _ => "buffer",
                };
                Some((b.binding, kind))
            })
            .collect();
        out.sort_unstable();
        out
    }

    use wgpu::naga;

    #[test]
    fn combined_samplers_split_and_bindings_renumber() {
        let (words, _) = fragment(0);
        let kinds = [UniformKind::Buffer, UniformKind::CombinedSampler];
        let fixed = fixup(&words, &kinds).unwrap();
        let module = parse(&fixed);
        assert_eq!(
            bindings(&module),
            vec![(0, "buffer"), (2, "image"), (3, "sampler"), (4, "buffer")],
            "UBO at 2i, image at 2i, sampler at 2i+1, push block at 2n"
        );
        assert!(
            module
                .global_variables
                .iter()
                .all(|(_, v)| v.space != naga::AddressSpace::Immediate),
            "push constants become a uniform block"
        );
    }

    #[test]
    fn new_ids_extend_the_bound() {
        let (words, ids) = fragment(0);
        let fixed = fixup(&words, &[UniformKind::Buffer, UniformKind::CombinedSampler]).unwrap();
        assert!(fixed[3] > words[3]);
        assert!(ids.tex != ids.ubo && ids.ubo != ids.push);
    }

    #[test]
    fn hostile_modules_are_rejected() {
        let (words, _) = fragment(0);
        let kinds = [UniformKind::Buffer, UniformKind::CombinedSampler];
        assert_eq!(fixup(&words[..3], &kinds), Err(SpvError::Header));
        let mut bad_magic = words.clone();
        bad_magic[0] = 0;
        assert_eq!(fixup(&bad_magic, &kinds), Err(SpvError::Header));
        let mut truncated = words.clone();
        truncated.truncate(words.len() - 3);
        assert!(matches!(
            fixup(&truncated, &kinds),
            Err(SpvError::Malformed(_))
        ));
        let mut zero_len = words.clone();
        zero_len[HEADER_WORDS] = 0;
        assert!(matches!(
            fixup(&zero_len, &kinds),
            Err(SpvError::Malformed(_))
        ));
        assert_eq!(fixup(&words, &kinds[..1]), Err(SpvError::UnknownBinding(1)));
        assert_eq!(
            fixup(&words, &[UniformKind::Buffer, UniformKind::TexelBuffer]),
            Err(SpvError::TexelBuffer)
        );
        assert_eq!(
            fixup(
                &words,
                &[UniformKind::CombinedSampler, UniformKind::CombinedSampler]
            ),
            Err(SpvError::KindMismatch {
                binding: 0,
                found: UniformKind::Buffer,
                declared: UniformKind::CombinedSampler,
            })
        );
        assert_eq!(
            fixup(&fragment(1).0, &kinds),
            Err(SpvError::DescriptorSet(1))
        );
        let mut overflow = words.clone();
        overflow[3] = u32::MAX;
        assert_eq!(fixup(&overflow, &kinds), Err(SpvError::Bound));
        assert_eq!(super::words(&[1, 2, 3]), Err(SpvError::Header));
    }
}
