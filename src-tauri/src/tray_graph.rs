//! Dynamic RGBA 32×32 tray icon renderer for live taskbar sparklines and bar meters.

use tm_core::models::HistoryPoint;

pub const ICON_SIZE: u32 = 32;
const PLOT_LEFT: usize = 2;
const PLOT_RIGHT: usize = 30; // exclusive, width = 28
const PLOT_TOP: usize = 4;
const PLOT_BOTTOM: usize = 28; // exclusive, height = 24

const BG_COLOR: [u8; 4] = [0x14, 0x19, 0x20, 0xff];
const BORDER_COLOR: [u8; 4] = [0x2d, 0x36, 0x42, 0xff];
const GRID_COLOR: [u8; 4] = [0x1e, 0x26, 0x31, 0xff];
const TRACK_COLOR: [u8; 4] = [0x1c, 0x23, 0x2d, 0xff];

pub const COLOR_CPU: [u8; 3] = [0x3c, 0xb8, 0xc2];
pub const COLOR_MEM: [u8; 3] = [0x9d, 0x8c, 0xff];
pub const COLOR_GPU: [u8; 3] = [0x4c, 0xc3, 0x8a];
pub const COLOR_NET: [u8; 3] = [0xf0, 0xb4, 0x4c];
const COLOR_WARN: [u8; 3] = [0xf0, 0xb4, 0x4c];
const COLOR_CRIT: [u8; 3] = [0xff, 0x7a, 0x6b];

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

/// Renders the requested [`TrayGraphMode`] into a 32×32 RGBA byte buffer, or
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
            let peak = raw
                .iter()
                .copied()
                .fold(10_240.0_f32, f32::max); // minimum 10 KB/s ceiling
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
    let mut out: Vec<f32> = history
        .iter()
        .rev()
        .take(width)
        .map(&extract)
        .collect();
    out.reverse();
    if out.is_empty() {
        out.push(extract(latest));
    }
    out
}

/// Renders a 4-bar meter badge (CPU, Memory, GPU, Network) into a 32×32 RGBA buffer.
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
        ((net_kb.ln_1p() / 50_000.0_f64.ln_1p()) * 100.0).clamp(4.0, 100.0) as f32
    };

    let bars: [(usize, usize, f32, [u8; 3], bool); 4] = [
        (4, 9, cpu_pct, COLOR_CPU, true),
        (10, 15, mem_pct, COLOR_MEM, true),
        (17, 22, gpu_pct, COLOR_GPU, true),
        (23, 28, net_pct, COLOR_NET, false),
    ];

    let plot_h = (PLOT_BOTTOM - PLOT_TOP) as f32;
    for (x0, x1, pct, base_rgb, shift_high) in bars {
        let clamped = pct.clamp(0.0, 100.0);
        let rgb = if shift_high {
            severity_color(clamped, base_rgb)
        } else {
            base_rgb
        };
        let fill_px = if clamped <= 0.5 {
            1usize
        } else {
            ((clamped / 100.0) * plot_h).round().clamp(2.0, plot_h) as usize
        };
        let fill_top = PLOT_BOTTOM - fill_px;

        for y in PLOT_TOP..PLOT_BOTTOM {
            for x in x0..x1 {
                if y < fill_top {
                    set_pixel(&mut buf, x, y, TRACK_COLOR);
                } else if y == fill_top {
                    // Bright cap for crisp visibility at small tray icon sizes.
                    set_pixel(&mut buf, x, y, brighten(rgb, 36, 0xff));
                } else {
                    let frac = (y - fill_top) as f32 / (fill_px.max(1) as f32);
                    let alpha = (235.0 - frac * 55.0).round() as u8;
                    blend_pixel(&mut buf, x, y, [rgb[0], rgb[1], rgb[2], alpha]);
                }
            }
        }
    }

    buf
}

/// Renders an area sparkline for `samples` (`0.0..=100.0`) into a 32×32 RGBA buffer.
pub fn render_sparkline(samples: &[f32], base_rgb: [u8; 3], shift_on_high: bool) -> Vec<u8> {
    let mut buf = new_badge_canvas(true);
    let width = PLOT_RIGHT - PLOT_LEFT;
    let height = PLOT_BOTTOM - PLOT_TOP;

    let latest_val = samples.last().copied().unwrap_or(0.0).clamp(0.0, 100.0);
    let rgb = if shift_on_high {
        severity_color(latest_val, base_rgb)
    } else {
        base_rgb
    };

    // Build per-column Y coordinates (PLOT_TOP..PLOT_BOTTOM).
    let mut col_y = vec![PLOT_BOTTOM - 1; width];
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
            let offset = (norm * (height - 1) as f32).round() as usize;
            *y_out = (PLOT_BOTTOM - 1).saturating_sub(offset);
        }
    }

    for (col_idx, &y_val) in col_y.iter().enumerate() {
        let x = PLOT_LEFT + col_idx;
        let prev_y = if col_idx > 0 { col_y[col_idx - 1] } else { y_val };
        let stroke_top = y_val.min(prev_y);
        let stroke_bottom = y_val.max(prev_y);

        // Fill gradient beneath the sparkline down to PLOT_BOTTOM - 1.
        for y in (stroke_bottom + 1)..PLOT_BOTTOM {
            let depth = (y - y_val) as f32 / (height as f32);
            let alpha = (145.0 * (1.0 - depth * 0.72)).clamp(28.0, 145.0) as u8;
            blend_pixel(&mut buf, x, y, [rgb[0], rgb[1], rgb[2], alpha]);
        }

        // Draw continuous anti-aliased top stroke.
        for y in stroke_top..=stroke_bottom {
            set_pixel(&mut buf, x, y, brighten(rgb, 24, 0xff));
        }
        if stroke_bottom + 1 < PLOT_BOTTOM {
            blend_pixel(&mut buf, x, stroke_bottom + 1, [rgb[0], rgb[1], rgb[2], 190]);
        }
    }

    buf
}

fn severity_color(pct: f32, base: [u8; 3]) -> [u8; 3] {
    if pct >= 92.0 {
        COLOR_CRIT
    } else if pct >= 80.0 {
        COLOR_WARN
    } else {
        base
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
    let radius = 5.5_f32;
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
            if dist <= radius - 1.0 {
                let is_border = x == 0 || y == 0 || x == size - 1 || y == size - 1;
                set_pixel(
                    &mut buf,
                    x,
                    y,
                    if is_border { BORDER_COLOR } else { BG_COLOR },
                );
            } else if dist <= radius {
                set_pixel(&mut buf, x, y, BORDER_COLOR);
            } else if dist < radius + 0.8 {
                let aa = ((radius + 0.8 - dist) / 0.8 * 180.0) as u8;
                set_pixel(
                    &mut buf,
                    x,
                    y,
                    [BORDER_COLOR[0], BORDER_COLOR[1], BORDER_COLOR[2], aa],
                );
            }
        }
    }

    if with_grid {
        for &gy in &[10usize, 16, 22] {
            for gx in PLOT_LEFT..PLOT_RIGHT {
                set_pixel(&mut buf, gx, gy, GRID_COLOR);
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
        let samples: Vec<f32> = (0..28).map(|i| (i as f32) * 3.5).collect();
        let rgba = render_sparkline(&samples, COLOR_CPU, true);
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        let opaque_pixels = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(opaque_pixels > 800, "expected most badge pixels to be non-transparent");
    }

    #[test]
    fn combined_bars_produces_valid_rgba_buffer() {
        let rgba = render_combined_bars(42.0, 68.5, 85.0, 1_500_000.0);
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        let opaque_pixels = rgba.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(opaque_pixels > 800);
    }
}
