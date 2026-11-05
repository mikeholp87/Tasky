//! The WGSL compute shaders of `gpu-compute-v1` and exact CPU references for
//! checking their output. Every platform runs these same shaders.
//!
//! Changing a shader, a constant or a reference changes what a GPU score
//! means: bump the method id.

/// Invocations per workgroup, in the shaders and in the dispatch maths.
pub const WORKGROUP_SIZE: u32 = 256;

/// Integer operations per loop iteration in [`INTEGER_WGSL`]: multiply, add,
/// shift, xor, multiply.
pub const INT_OPS_PER_ITER: f64 = 5.0;
/// Floating-point operations per loop iteration in [`FLOAT_WGSL`]: one fused
/// multiply-add (two operations) on each of four lanes.
pub const FLOAT_OPS_PER_ITER: f64 = 8.0;
/// Bytes moved per element by [`MEMORY_WGSL`]: one `vec4<u32>` read and one
/// written.
pub const MEMORY_BYTES_PER_ELEMENT: f64 = 32.0;

/// Fixed seed mixed into every integer chain.
pub const INT_SEED: u32 = 0x5EED_C0DE;
/// The contraction `v = fma(v, A, B)`: its fixed point is `B / (1 - A)`.
pub const FLOAT_A: f32 = 0.99;
pub const FLOAT_B: [f32; 4] = [0.01, 0.02, 0.03, 0.04];

/// A dependent chain of 32-bit multiply, add, shift and xor per invocation.
/// u32 arithmetic wraps, so the result is bit-exact on every device.
pub const INTEGER_WGSL: &str = r#"
struct Params { iterations: u32, seed: u32, count: u32, pad: u32 }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= params.count) { return; }
    var x: u32 = (index * 0x9E3779B9u) ^ params.seed;
    for (var i: u32 = 0u; i < params.iterations; i = i + 1u) {
        x = x * 1664525u + 1013904223u;
        x = x ^ (x >> 13u);
        x = x * 0x85EBCA6Bu;
    }
    out[index] = x;
}
"#;

/// Four independent fused multiply-add chains per invocation (a contraction
/// toward a known fixed point, so the result can be checked analytically).
pub const FLOAT_WGSL: &str = r#"
struct Params { iterations: u32, seed: u32, count: u32, pad: u32 }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> out: array<vec4<f32>>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let index = gid.x;
    if (index >= params.count) { return; }
    var v = vec4<f32>(
        f32(index & 1023u) / 1024.0,
        f32((index >> 10u) & 1023u) / 1024.0,
        f32((index >> 5u) & 1023u) / 1024.0,
        f32((index >> 2u) & 1023u) / 1024.0);
    let a = vec4<f32>(0.99);
    let b = vec4<f32>(0.01, 0.02, 0.03, 0.04);
    for (var i: u32 = 0u; i < params.iterations; i = i + 1u) {
        v = fma(v, a, b);
    }
    out[index] = v;
}
"#;

/// Reads one buffer and writes another, adding one to every lane; a grid
/// stride loop lets one dispatch cover a buffer of any size.
pub const MEMORY_WGSL: &str = r#"
struct Params { iterations: u32, seed: u32, count: u32, pad: u32 }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> src: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read_write> dst: array<vec4<u32>>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let stride = groups.x * 256u;
    var i = gid.x;
    loop {
        if (i >= params.count) { break; }
        dst[i] = src[i] + vec4<u32>(1u);
        i = i + stride;
    }
}
"#;

/// CPU reference for [`INTEGER_WGSL`].
pub fn integer_reference(index: u32, seed: u32, iterations: u32) -> u32 {
    let mut x = index.wrapping_mul(0x9E37_79B9) ^ seed;
    for _ in 0..iterations {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        x ^= x >> 13;
        x = x.wrapping_mul(0x85EB_CA6B);
    }
    x
}

/// Start value of the four float lanes for invocation `index`. Every value is
/// `k / 1024`, exactly representable in `f32`.
pub fn float_start(index: u32) -> [f32; 4] {
    let lane = |shift: u32| ((index >> shift) & 1023) as f32 / 1024.0;
    [lane(0), lane(10), lane(5), lane(2)]
}

/// Mathematical result of `iterations` steps of `v = A·v + B` from the start
/// value, in double precision: `F + Aⁿ(v₀ − F)` with `F = B / (1 − A)`.
pub fn float_expected(index: u32, iterations: u32) -> [f64; 4] {
    let a = f64::from(FLOAT_A);
    let start = float_start(index);
    let mut out = [0.0; 4];
    for lane in 0..4 {
        let fixed = f64::from(FLOAT_B[lane]) / (1.0 - a);
        out[lane] = fixed + a.powi(iterations as i32) * (f64::from(start[lane]) - fixed);
    }
    out
}

/// Largest difference between a device's float result and the mathematical
/// one that is accepted. Single-precision rounding through the chain stays
/// below about 3e-5, so this leaves ample margin while still catching a
/// shader that did not run or computed something else.
pub const FLOAT_TOLERANCE: f64 = 1e-3;

/// Initial content of the memory buffers, per 32-bit lane.
pub fn memory_init(lane: u32) -> u32 {
    lane.wrapping_mul(2_654_435_761) ^ 0xA5A5_5A5A
}

#[cfg(test)]
mod tests {
    use super::*;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    /// Parses and validates a shader with naga, as wgpu will, without a GPU.
    fn check(source: &str) {
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("WGSL parse error: {}", e.emit_to_string(source)));
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("WGSL validation error: {e:?}"));
        assert_eq!(module.entry_points.len(), 1);
        assert_eq!(
            module.entry_points[0].workgroup_size,
            [WORKGROUP_SIZE, 1, 1]
        );
    }

    #[test]
    fn all_shaders_are_valid_wgsl() {
        check(INTEGER_WGSL);
        check(FLOAT_WGSL);
        check(MEMORY_WGSL);
    }

    #[test]
    fn integer_reference_matches_a_hand_computed_step() {
        // iterations = 0: only the initial mix.
        assert_eq!(integer_reference(1, 0, 0), 0x9E37_79B9);
        assert_eq!(integer_reference(0, 7, 0), 7);
        // One step from x = 0: x = 1013904223; x ^= x >> 13; x *= 0x85EBCA6B.
        let mut x: u32 = 1_013_904_223;
        x ^= x >> 13;
        x = x.wrapping_mul(0x85EB_CA6B);
        assert_eq!(integer_reference(0, 0, 1), x);
        // More iterations change the result, and the chain is deterministic.
        assert_ne!(
            integer_reference(5, INT_SEED, 10),
            integer_reference(5, INT_SEED, 11)
        );
        assert_eq!(
            integer_reference(5, INT_SEED, 10),
            integer_reference(5, INT_SEED, 10)
        );
        assert_ne!(
            integer_reference(5, INT_SEED, 10),
            integer_reference(6, INT_SEED, 10)
        );
    }

    #[test]
    fn float_lanes_start_exactly_representable_and_inside_zero_one() {
        for index in [0, 1, 1023, 1024, 123_456, 1_048_575] {
            for v in float_start(index) {
                assert!((0.0..1.0).contains(&v));
                assert_eq!(f64::from(v) * 1024.0, (f64::from(v) * 1024.0).round());
            }
        }
    }

    #[test]
    fn the_float_contraction_converges_to_its_fixed_point() {
        // The fixed point of v = A v + B is about k for lane k. It is not
        // exactly k because 0.99 and 0.01 are not exactly representable in
        // f32; the reference uses the real f32 constants, as the shader does.
        let far = float_expected(77, 5_000);
        for (lane, v) in far.iter().enumerate() {
            assert!((v - (lane as f64 + 1.0)).abs() < 1e-5, "{v}");
            let exact = f64::from(FLOAT_B[lane]) / (1.0 - f64::from(FLOAT_A));
            assert!((v - exact).abs() < 1e-12, "{v} vs {exact}");
        }
        // With no iterations it is the start value.
        let start = float_start(77);
        let zero = float_expected(77, 0);
        for lane in 0..4 {
            assert_eq!(zero[lane], f64::from(start[lane]));
        }
    }

    /// The accepted tolerance really does cover single-precision rounding on
    /// the exact operation the shader performs (a fused multiply-add).
    #[test]
    fn single_precision_fma_stays_within_the_tolerance() {
        for iterations in [1u32, 10, 100, 1_000, 5_000] {
            for index in [0u32, 1, 4_095, 54_321, 1_000_003] {
                let mut v = float_start(index);
                for _ in 0..iterations {
                    for lane in 0..4 {
                        v[lane] = v[lane].mul_add(FLOAT_A, FLOAT_B[lane]);
                    }
                }
                let want = float_expected(index, iterations);
                for lane in 0..4 {
                    let err = (f64::from(v[lane]) - want[lane]).abs();
                    assert!(err < FLOAT_TOLERANCE / 10.0, "{iterations} it, err {err}");
                }
            }
        }
    }

    #[test]
    fn memory_init_is_deterministic_and_varied() {
        assert_eq!(memory_init(3), memory_init(3));
        let distinct: std::collections::HashSet<u32> = (0..1000).map(memory_init).collect();
        assert_eq!(distinct.len(), 1000);
    }
}
