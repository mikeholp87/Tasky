//! Dynamic high-DPI RGBA 64×64 tray icon renderer for live taskbar sparklines and bar meters.

use tm_core::models::HistoryPoint;

pub const ICON_SIZE: u32 = 64;
const PLOT_LEFT: usize = 4;
const PLOT_RIGHT: usize = 60; // exclusive, width = 56
const PLOT_TOP: usize = 6;
const PLOT_BOTTOM: usize = 58; // exclusive, height = 52

const BG_COLOR: [u8; 4] = [0x09, 0x0d, 0x14, 0xfa];
const BORDER_OUTER_COLOR: [u8; 4] = [0x38, 0xbd, 0xf8, 0x88];
const BORDER_INNER_COLOR: [u8; 4] = [0x33, 0x41, 0x55, 0xff];
const GRID_COLOR: [u8; 4] = [0x26, 0x34, 0x49, 0xff];
const TRACK_COLOR: [u8; 4] = [0x1e, 0x29, 0x3b, 0xff];
const TRACK_BORDER_COLOR: [u8; 4] = [0x28, 0x36, 0x4d, 0xff];

pub const COLOR_CPU: [u8; 3] = [0x06, 0xb6, 0xd4];
pub const CAP_CPU: [u8; 3] = [0x67, 0xe8, 0xf9];

pub const COLOR_MEM: [u8; 3] = [0xa8, 0x55, 0xf7];
pub const CAP_MEM: [u8; 3] = [0xd8, 0xb4, 0xfe];

pub const COLOR_GPU: [u8; 3] = [0x10, 0xb9, 0x81];
pub const CAP_GPU: [u8; 3] = [0x6e, 0xe7, 0xb7];

pub const COLOR_NET: [u8; 3] = [0xf5, 0x9e, 0x0b];
pub const CAP_NET: [u8; 3] = [0xfd, 0xe6, 0x8a];

const COLOR_WARN: [u8; 3] = [0xfb, 0x92, 0x3c];
const CAP_WARN: [u8; 3] = [0xfe, 0xd7, 0xaa];

const COLOR_CRIT: [u8; 3] = [0xf8, 0x71, 0x71];
const CAP_CRIT: [u8; 3] = [0xfe, 0xca, 0xca];

/// Which live visualization to render on the main system tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayGraphMode {
    CombinedBars,
    CpuSparkline,
    MemorySparkline,
    GpuSparkline,
    NetworkSparkline,
    StaticIcon,
}

/// Renders the requested [`TrayGraphMode`] into a 64×64 RGBA byte buffer, or
/// returns `None` when [`TrayGraphMode::StaticIcon`] is selected.
pub fn render_tray_icon(
    mode: TrayGraphMode,
    history: &[HistoryPoint],
    latest: &HistoryPoint,
) -> Option<Vec<u8>> {
    match mode {
        TrayGraphMode::StaticIcon => None,
        TrayGraphMode::CombinedBars => Some(render_combined_bars(
            latest.cpu,
            latest.mem_percent,
            latest.gpu_usage.unwrap_or(0.0),
            latest.net_rx + latest.net_tx,
        )),
        TrayGraphMode::CpuSparkline => {
            let samples = collect_samples(history, latest, |p| p.cpu);
            Some(render_sparkline(&samples, COLOR_CPU, true))
        }
        TrayGraphMode::MemorySparkline => {
            let samples = collect_samples(history, latest, |p| p.mem_percent);
            Some(render_sparkline(&samples, COLOR_MEM, true))
        }
        TrayGraphMode::GpuSparkline => {
            let samples = collect_samples(history, latest, |p| p.gpu_usage.unwrap_or(0.0));
            Some(render_sparkline(&samples, COLOR_GPU, true))
        }
        TrayGraphMode::NetworkSparkline => {
            let raw = collect_samples(history, latest, |p| (p.net_rx + p.net_tx) as f32);
            let peak = raw.iter().copied().fold(10_240.0_f32, f32::max); // minimum 10 KB/s ceiling
            let normalized: Vec<f32> = raw
                .into_iter()
                .map(|v| (v / peak * 100.0).clamp(0.0, 100.0))
                .collect();
            Some(render_sparkline(&normalized, COLOR_NET, false))
        }
    }
}

fn collect_samples<F>(history: &[HistoryPoint], latest: &HistoryPoint, extract: F) -> Vec<f32>
where
    F: Fn(&HistoryPoint) -> f32,
{
    let width = PLOT_RIGHT - PLOT_LEFT;
    let mut out: Vec<f32> = history.iter().rev().take(width).map(&extract).collect();
    out.reverse();
    if out.is_empty() {
        out.push(extract(latest));
    }
    out
}

struct BarSpec {
    x0: usize,
    x1: usize,
    pct: f32,
    base_rgb: [u8; 3],
    base_cap: [u8; 3],
    shift_high: bool,
}

/// Renders a high-contrast 4-bar meter badge (CPU, Memory, GPU, Network) into a 64×64 RGBA buffer.
pub fn render_combined_bars(
    cpu_pct: f32,
    mem_pct: f32,
    gpu_pct: f32,
    net_bytes_per_sec: f64,
) -> Vec<u8> {
    let mut buf = new_badge_canvas(false);

    // Map network throughput logarithmically from 1 KB/s..=50 MB/s onto 0..=100%.
    let net_kb = (net_bytes_per_sec / 1024.0).max(0.0);
    let net_pct = if net_kb <= 0.1 {
        0.0
    } else {
        ((net_kb.ln_1p() / 50_000.0_f64.ln_1p()) * 100.0).clamp(6.0, 100.0) as f32
    };

    // 4 bars, each 11px wide with 3px spacing, spanning x = 5..58 and y = 6..58.
    let bars = [
        BarSpec {
            x0: 5,
            x1: 16,
            pct: cpu_pct,
            base_rgb: COLOR_CPU,
            base_cap: CAP_CPU,
            shift_high: true,
        },
        BarSpec {
            x0: 19,
            x1: 30,
            pct: mem_pct,
            base_rgb: COLOR_MEM,
            base_cap: CAP_MEM,
            shift_high: true,
        },
        BarSpec {
            x0: 33,
            x1: 44,
            pct: gpu_pct,
            base_rgb: COLOR_GPU,
            base_cap: CAP_GPU,
            shift_high: true,
        },
        BarSpec {
            x0: 47,
            x1: 58,
            pct: net_pct,
            base_rgb: COLOR_NET,
            base_cap: CAP_NET,
            shift_high: false,
        },
    ];

    let plot_h = (PLOT_BOTTOM - PLOT_TOP) as f32;
    for bar in bars {
        let clamped = bar.pct.clamp(0.0, 100.0);
        let (rgb, cap_rgb) = if bar.shift_high {
            severity_palette(clamped, bar.base_rgb, bar.base_cap)
        } else {
            (bar.base_rgb, bar.base_cap)
        };

        // Always draw at least a 4px visible bar so all 4 metrics are clearly legible even at idle.
        let fill_px = ((clamped / 100.0) * plot_h).round().clamp(4.0, plot_h) as usize;
        let fill_top = PLOT_BOTTOM - fill_px;
        let cap_bottom = (fill_top + 3).min(PLOT_BOTTOM);

        for y in PLOT_TOP..PLOT_BOTTOM {
            let is_grid_row = y == 19 || y == 32 || y == 45;
            for x in bar.x0..bar.x1 {
                let is_bar_edge = x == bar.x0 || x + 1 == bar.x1 || y == PLOT_TOP;
                if y < fill_top {
                    if is_bar_edge || (is_grid_row && (x - bar.x0) % 2 == 0) {
                        set_pixel(&mut buf, x, y, TRACK_BORDER_COLOR);
                    } else {
                        set_pixel(&mut buf, x, y, TRACK_COLOR);
                    }
                } else if y < cap_bottom {
                    // Crisp 3px bright top cap for immediate readability at small panel sizes.
                    set_pixel(&mut buf, x, y, [cap_rgb[0], cap_rgb[1], cap_rgb[2], 0xff]);
                } else {
                    let frac = (y - cap_bottom) as f32 / (fill_px.max(1) as f32);
                    let alpha = (250.0 - frac * 45.0).round().clamp(195.0, 250.0) as u8;
                    set_pixel(&mut buf, x, y, TRACK_COLOR);
                    blend_pixel(&mut buf, x, y, [rgb[0], rgb[1], rgb[2], alpha]);
                }
            }
        }
    }

    buf
}

/// Renders a high-contrast area sparkline for `samples` (`0.0..=100.0`) into a 64×64 RGBA buffer.
pub fn render_sparkline(samples: &[f32], base_rgb: [u8; 3], shift_on_high: bool) -> Vec<u8> {
    let mut buf = new_badge_canvas(true);
    let width = PLOT_RIGHT - PLOT_LEFT;
    let height = PLOT_BOTTOM - PLOT_TOP;

    let latest_val = samples.last().copied().unwrap_or(0.0).clamp(0.0, 100.0);
    let default_cap = [
        base_rgb[0].saturating_add(65),
        base_rgb[1].saturating_add(65),
        base_rgb[2].saturating_add(65),
    ];
    let (rgb, cap_rgb) = if shift_on_high {
        severity_palette(latest_val, base_rgb, default_cap)
    } else {
        (base_rgb, default_cap)
    };

    // Build per-column Y coordinates (PLOT_TOP..PLOT_BOTTOM).
    let mut col_y = vec![PLOT_BOTTOM - 2; width];
    if !samples.is_empty() {
        let take_n = samples.len().min(width);
        let slice = &samples[samples.len() - take_n..];
        let pad = width - take_n;
        let first_val = slice[0].clamp(0.0, 100.0);
        for (i, y_out) in col_y.iter_mut().enumerate() {
            let val = if i < pad {
                first_val
            } else {
                slice[i - pad].clamp(0.0, 100.0)
            };
            let norm = val / 100.0;
            let offset = (norm * (height - 2) as f32).round() as usize;
            *y_out = (PLOT_BOTTOM - 2).saturating_sub(offset);
        }
    }

    for (col_idx, &y_val) in col_y.iter().enumerate() {
        let x = PLOT_LEFT + col_idx;
        let prev_y = if col_idx > 0 {
            col_y[col_idx - 1]
        } else {
            y_val
        };
        let stroke_top = y_val.min(prev_y);
        let stroke_bottom = (y_val.max(prev_y) + 2).min(PLOT_BOTTOM - 1);

        // High-visibility vertical gradient fill below the curve (alpha 165 down to 45).
        for y in (stroke_bottom + 1)..PLOT_BOTTOM {
            let depth = ((y - stroke_top) as f32 / (height as f32)).clamp(0.0, 1.0);
            let alpha = (165.0 - depth * 120.0).round().clamp(45.0, 165.0) as u8;
            blend_pixel(&mut buf, x, y, [rgb[0], rgb[1], rgb[2], alpha]);
        }

        // Anti-aliased upper glow 1px above the stroke.
        if stroke_top > PLOT_TOP {
            blend_pixel(
                &mut buf,
                x,
                stroke_top - 1,
                [cap_rgb[0], cap_rgb[1], cap_rgb[2], 115],
            );
        }

        // Thick 3px bright top stroke.
        for y in stroke_top..=stroke_bottom {
            let color = if y == stroke_top {
                [cap_rgb[0], cap_rgb[1], cap_rgb[2], 0xff]
            } else {
                brighten(rgb, 32, 0xff)
            };
            set_pixel(&mut buf, x, y, color);
        }

        // Anti-aliased lower feather 1px below the stroke.
        if stroke_bottom + 1 < PLOT_BOTTOM {
            blend_pixel(&mut buf, x, stroke_bottom + 1, [rgb[0], rgb[1], rgb[2], 210]);
        }
    }

    // Highlight live status dot at the rightmost sample (x = 58..60).
    if let Some(&last_y) = col_y.last() {
        let cx = (PLOT_RIGHT - 2) as isize;
        let cy = last_y as isize;
        for dy in -2..=2isize {
            for dx in -2..=2isize {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= 5 {
                    let px = (cx + dx) as usize;
                    let py = (cy + dy) as usize;
                    if (PLOT_LEFT..PLOT_RIGHT).contains(&px)
                        && (PLOT_TOP..PLOT_BOTTOM).contains(&py)
                    {
                        let dot_color = if dist_sq <= 2 {
                            [0xff, 0xff, 0xff, 0xff]
                        } else {
                            [cap_rgb[0], cap_rgb[1], cap_rgb[2], 0xff]
                        };
                        set_pixel(&mut buf, px, py, dot_color);
                    }
                }
            }
        }
    }

    buf
}

fn severity_palette(pct: f32, base: [u8; 3], base_cap: [u8; 3]) -> ([u8; 3], [u8; 3]) {
    if pct >= 92.0 {
        (COLOR_CRIT, CAP_CRIT)
    } else if pct >= 80.0 {
        (COLOR_WARN, CAP_WARN)
    } else {
        (base, base_cap)
    }
}

fn brighten(rgb: [u8; 3], boost: u8, alpha: u8) -> [u8; 4] {
    [
        rgb[0].saturating_add(boost),
        rgb[1].saturating_add(boost),
        rgb[2].saturating_add(boost),
        alpha,
    ]
}

fn new_badge_canvas(with_grid: bool) -> Vec<u8> {
    let size = ICON_SIZE as usize;
    let mut buf = vec![0u8; size * size * 4];
    let radius = 10.0_f32;
    let min_c = radius;
    let max_c = (size as f32) - 1.0 - radius;

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let dx = if fx < min_c {
                min_c - fx
            } else if fx > max_c {
                fx - max_c
            } else {
                0.0
            };
            let dy = if fy < min_c {
                min_c - fy
            } else if fy > max_c {
                fy - max_c
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            let edge_dist = x.min(y).min(size - 1 - x).min(size - 1 - y);

            if dist <= radius - 2.0 {
                if edge_dist == 0 {
                    set_pixel(&mut buf, x, y, BORDER_OUTER_COLOR);
                } else if edge_dist == 1 {
                    set_pixel(&mut buf, x, y, BORDER_INNER_COLOR);
                } else {
                    set_pixel(&mut buf, x, y, BG_COLOR);
                }
            } else if dist <= radius - 1.0 {
                set_pixel(&mut buf, x, y, BORDER_INNER_COLOR);
            } else if dist <= radius {
                set_pixel(&mut buf, x, y, BORDER_OUTER_COLOR);
            } else if dist < radius + 0.9 {
                let aa = ((radius + 0.9 - dist) / 0.9 * (BORDER_OUTER_COLOR[3] as f32)) as u8;
                set_pixel(
                    &mut buf,
                    x,
                    y,
                    [
                        BORDER_OUTER_COLOR[0],
                        BORDER_OUTER_COLOR[1],
                        BORDER_OUTER_COLOR[2],
                        aa,
                    ],
                );
            }
        }
    }

    if with_grid {
        // Subtle 25%, 50%, 75% dashed horizontal guide lines.
        for &gy in &[19usize, 32, 45] {
            for gx in PLOT_LEFT..PLOT_RIGHT {
                if (gx - PLOT_LEFT) % 3 != 2 {
                    set_pixel(&mut buf, gx, gy, GRID_COLOR);
                }
            }
        }
    }

    buf
}

fn set_pixel(buf: &mut [u8], x: usize, y: usize, rgba: [u8; 4]) {
    let size = ICON_SIZE as usize;
    if x < size && y < size {
        let idx = (y * size + x) * 4;
        buf[idx..idx + 4].copy_from_slice(&rgba);
    }
}

fn blend_pixel(buf: &mut [u8], x: usize, y: usize, src: [u8; 4]) {
    let size = ICON_SIZE as usize;
    if x >= size || y >= size {
        return;
    }
    let idx = (y * size + x) * 4;
    let sa = src[3] as u16;
    let inv_sa = 255 - sa;
    buf[idx] = ((src[0] as u16 * sa + buf[idx] as u16 * inv_sa) / 255) as u8;
    buf[idx + 1] = ((src[1] as u16 * sa + buf[idx + 1] as u16 * inv_sa) / 255) as u8;
    buf[idx + 2] = ((src[2] as u16 * sa + buf[idx + 2] as u16 * inv_sa) / 255) as u8;
    buf[idx + 3] = 0xff;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_produces_valid_rgba_buffer() {
        let samples: Vec<f32> = (0..56).map(|i| (i as f32) * 1.75).collect();
        let rgba = render_sparkline(&samples, COLOR_CPU, true);
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        let opaque_pixels = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(
            opaque_pixels > 3_500,
            "expected most 64x64 badge pixels to be non-transparent, got {opaque_pixels}"
        );
    }

    #[test]
    fn combined_bars_produces_valid_rgba_buffer() {
        let rgba = render_combined_bars(42.0, 68.5, 85.0, 1_500_000.0);
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        let opaque_pixels = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(opaque_pixels > 3_500);
    }
}
