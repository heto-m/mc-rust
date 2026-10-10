// Translucent draws are sorted on the CPU; occluded sections keep their place with zero instances.

@group(0) @binding(0) var<uniform> trans_count: vec4<u32>;
@group(0) @binding(1) var<storage, read> visibility: array<u32>;
@group(0) @binding(2) var<storage, read_write> trans_cmds: array<DrawIndexedIndirect>;
// `pad0` holds the section slot.
@group(0) @binding(3) var<storage, read> trans_draws: array<DrawData>;

@compute @workgroup_size(64)
fn cull_translucent(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= trans_count.x {
        return;
    }
    if visibility[trans_draws[i].pad0] == 0u {
        trans_cmds[i].instance_count = 0u;
    }
}
