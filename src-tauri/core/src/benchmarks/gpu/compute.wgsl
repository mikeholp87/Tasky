// Compute throughput kernel.
//
// Every invocation runs `params.iterations` iterations of 4 independent
// vec4 fused multiply-adds (4 chains × 4 lanes × 2 FLOPs = 32 FLOPs per
// iteration). The multiplier and addend come from a uniform buffer, so the
// compiler cannot fold the loop; values converge towards c / (1 - m) and
// stay finite. Results are written back so the work cannot be discarded.

struct Params {
    m: vec4<f32>,
    c: vec4<f32>,
    iterations: u32,
    row_stride: u32,
}

@group(0) @binding(0) var<storage, read_write> data: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x + gid.y * params.row_stride;
    if (idx >= arrayLength(&data)) {
        return;
    }
    var a = data[idx];
    var b = a * 0.5;
    var c = a * 0.25;
    var d = a * 0.125;
    let m = params.m;
    let k = params.c;
    for (var i = 0u; i < params.iterations; i = i + 1u) {
        a = fma(a, m, k);
        b = fma(b, m, k);
        c = fma(c, m, k);
        d = fma(d, m, k);
    }
    data[idx] = a + b + c + d;
}
