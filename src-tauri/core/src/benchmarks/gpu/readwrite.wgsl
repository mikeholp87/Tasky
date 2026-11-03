// Memory throughput kernel: each invocation reads one vec4 (16 bytes) and
// writes one vec4, so a dispatch moves exactly 2 × buffer size bytes.

struct Params {
    row_stride: u32,
}

@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x + gid.y * params.row_stride;
    if (idx >= arrayLength(&dst)) {
        return;
    }
    dst[idx] = src[idx] * 1.0001 + vec4<f32>(1.0);
}
