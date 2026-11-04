//! Desktop integration: system tray with live sparkline/bar graphs, rich
//! telemetry menu, alert notifications and main-window visibility.
//!
//! Everything here runs from the engine thread through [`DesktopSink`] and
//! must never block it: tray updates are *posted* to the main thread and
//! notifications are shown on a blocking-pool thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;
use tm_core::models::{HistoryPoint, MetricsTick};
use tm_core::monitoring::TickSink;

use crate::state::AppState;
use crate::tray_graph::{ICON_SIZE, TrayGraphMode, render_combined_bars, render_tray_icon};

pub const MAIN_WINDOW: &str = "main";
const TRAY_UPDATE_MS: u64 = 1_000;
const EXTRA_TRAY_CPU_ID: &str = "tray-cpu";
const EXTRA_TRAY_GPU_ID: &str = "tray-gpu";

/// Whether the main window is currently shown. Ticks are only serialized for
/// the webview while this is true.
#[derive(Default)]
pub struct WindowVisibility(AtomicBool);

impl WindowVisibility {
    pub fn set(&self, visible: bool) {
        self.0.store(visible, Ordering::Relaxed);
    }
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.set_skip_taskbar(false);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_skip_taskbar(false);
        let _ = window.set_focus();
        if let Some(state) = app.try_state::<AppState>() {
            state.visibility.set(true);
            // Give the reappearing UI fresh data immediately.
            let _ = state.engine.refresh_now();
        }
    }
}

pub fn hide_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.set_skip_taskbar(true);
        let _ = window.hide();
        let _ = window.set_skip_taskbar(true);
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.visibility.set(false);
    }
}

/// Optional secondary taskbar graph icons (CPU sparkline + GPU sparkline).
pub struct ExtraTrayIcons<R: Runtime> {
    pub cpu_tray: TrayIcon<R>,
    pub gpu_tray: TrayIcon<R>,
}

/// Tray menu items and state whose text/icons change at runtime.
pub struct TrayItems<R: Runtime> {
    pub tray: TrayIcon<R>,
    pub pause: MenuItem<R>,
    pub cpu_item: MenuItem<R>,
    pub mem_item: MenuItem<R>,
    pub gpu_item: MenuItem<R>,
    pub net_item: MenuItem<R>,
    pub mode_combined: CheckMenuItem<R>,
    pub mode_cpu: CheckMenuItem<R>,
    pub mode_mem: CheckMenuItem<R>,
    pub mode_gpu: CheckMenuItem<R>,
    pub mode_net: CheckMenuItem<R>,
    pub mode_static: CheckMenuItem<R>,
    pub toggle_extra_trays: CheckMenuItem<R>,
    pub graph_mode: Mutex<TrayGraphMode>,
    pub extra_trays: Mutex<Option<ExtraTrayIcons<R>>>,
}

pub fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayItems<R>> {
    let open = MenuItem::with_id(app, "open", "Open Task Manager", true, None::<&str>)?;
    let hide_to_tray = MenuItem::with_id(
        app,
        "hide_to_tray",
        "Hide to Tray (Dock Hidden)",
        true,
        None::<&str>,
    )?;

    let cpu_item = MenuItem::with_id(app, "stat_cpu", "CPU: —", true, None::<&str>)?;
    let mem_item = MenuItem::with_id(app, "stat_mem", "Memory: —", true, None::<&str>)?;
    let gpu_item = MenuItem::with_id(app, "stat_gpu", "GPU: —", true, None::<&str>)?;
    let net_item = MenuItem::with_id(app, "stat_net", "Net: —", true, None::<&str>)?;

    let mode_combined = CheckMenuItem::with_id(
        app,
        "mode_combined",
        "Combined Bars (CPU · RAM · GPU · Net)",
        true,
        true,
        None::<&str>,
    )?;
    let mode_cpu = CheckMenuItem::with_id(
        app,
        "mode_cpu",
        "CPU Usage Graph",
        true,
        false,
        None::<&str>,
    )?;
    let mode_mem = CheckMenuItem::with_id(
        app,
        "mode_mem",
        "Memory Usage Graph",
        true,
        false,
        None::<&str>,
    )?;
    let mode_gpu = CheckMenuItem::with_id(
        app,
        "mode_gpu",
        "GPU Usage Graph",
        true,
        false,
        None::<&str>,
    )?;
    let mode_net = CheckMenuItem::with_id(
        app,
        "mode_net",
        "Network Throughput Graph",
        true,
        false,
        None::<&str>,
    )?;
    let mode_static = CheckMenuItem::with_id(
        app,
        "mode_static",
        "Static App Icon",
        true,
        false,
        None::<&str>,
    )?;

    let mode_submenu = Submenu::with_items(
        app,
        "Tray Graph Mode",
        true,
        &[
            &mode_combined,
            &mode_cpu,
            &mode_mem,
            &mode_gpu,
            &mode_net,
            &mode_static,
        ],
    )?;

    let toggle_extra_trays = CheckMenuItem::with_id(
        app,
        "toggle_extra_trays",
        "Show Extra Graph Icons in Taskbar",
        true,
        false,
        None::<&str>,
    )?;

    let pause = MenuItem::with_id(app, "pause", "Pause monitoring", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &hide_to_tray,
            &PredefinedMenuItem::separator(app)?,
            &cpu_item,
            &mem_item,
            &gpu_item,
            &net_item,
            &PredefinedMenuItem::separator(app)?,
            &mode_submenu,
            &toggle_extra_trays,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let initial_rgba = render_combined_bars(0.0, 0.0, 0.0, 0.0);
    let initial_icon = Image::new_owned(initial_rgba, ICON_SIZE, ICON_SIZE);

    let tray = TrayIconBuilder::with_id("main")
        .tooltip("Task Manager")
        .icon(initial_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" | "stat_cpu" | "stat_mem" | "stat_gpu" | "stat_net" => {
                show_main_window(app);
            }
            "hide_to_tray" => {
                hide_main_window(app);
            }
            "mode_combined" => set_tray_mode(app, TrayGraphMode::CombinedBars),
            "mode_cpu" => set_tray_mode(app, TrayGraphMode::CpuSparkline),
            "mode_mem" => set_tray_mode(app, TrayGraphMode::MemorySparkline),
            "mode_gpu" => set_tray_mode(app, TrayGraphMode::GpuSparkline),
            "mode_net" => set_tray_mode(app, TrayGraphMode::NetworkSparkline),
            "mode_static" => set_tray_mode(app, TrayGraphMode::StaticIcon),
            "toggle_extra_trays" => toggle_extra_taskbar_trays(app),
            "pause" => toggle_pause(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window(MAIN_WINDOW)
                    && window.is_visible().unwrap_or(false)
                    && !window.is_minimized().unwrap_or(false)
                    && window.is_focused().unwrap_or(false)
                {
                    hide_main_window(app);
                } else {
                    show_main_window(app);
                }
            }
        })
        .build(app)?;

    Ok(TrayItems {
        tray,
        pause,
        cpu_item,
        mem_item,
        gpu_item,
        net_item,
        mode_combined,
        mode_cpu,
        mode_mem,
        mode_gpu,
        mode_net,
        mode_static,
        toggle_extra_trays,
        graph_mode: Mutex::new(TrayGraphMode::CombinedBars),
        extra_trays: Mutex::new(None),
    })
}

fn set_tray_mode<R: Runtime>(app: &AppHandle<R>, mode: TrayGraphMode) {
    let Some(items) = app.try_state::<TrayItems<R>>() else {
        return;
    };
    if let Ok(mut current) = items.graph_mode.lock() {
        *current = mode;
    }
    let _ = items
        .mode_combined
        .set_checked(mode == TrayGraphMode::CombinedBars);
    let _ = items
        .mode_cpu
        .set_checked(mode == TrayGraphMode::CpuSparkline);
    let _ = items
        .mode_mem
        .set_checked(mode == TrayGraphMode::MemorySparkline);
    let _ = items
        .mode_gpu
        .set_checked(mode == TrayGraphMode::GpuSparkline);
    let _ = items
        .mode_net
        .set_checked(mode == TrayGraphMode::NetworkSparkline);
    let _ = items
        .mode_static
        .set_checked(mode == TrayGraphMode::StaticIcon);

    if let Some(state) = app.try_state::<AppState>()
        && let Some(latest) = state.engine.latest()
    {
        let history = state.engine.history(60);
        refresh_tray_visuals(app, &items, &latest, &history);
    }
}

fn toggle_extra_taskbar_trays<R: Runtime>(app: &AppHandle<R>) {
    let Some(items) = app.try_state::<TrayItems<R>>() else {
        return;
    };
    let mut guard = match items.extra_trays.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    if guard.is_some() {
        *guard = None;
        let _ = app.remove_tray_by_id(EXTRA_TRAY_CPU_ID);
        let _ = app.remove_tray_by_id(EXTRA_TRAY_GPU_ID);
        let _ = items.toggle_extra_trays.set_checked(false);
    } else {
        let initial_rgba = render_combined_bars(0.0, 0.0, 0.0, 0.0);
        let cpu_icon = Image::new_owned(initial_rgba.clone(), ICON_SIZE, ICON_SIZE);
        let gpu_icon = Image::new_owned(initial_rgba, ICON_SIZE, ICON_SIZE);

        let cpu_tray = TrayIconBuilder::with_id(EXTRA_TRAY_CPU_ID)
            .tooltip("Task Manager — CPU Graph")
            .icon(cpu_icon)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            })
            .build(app);

        let gpu_tray = TrayIconBuilder::with_id(EXTRA_TRAY_GPU_ID)
            .tooltip("Task Manager — GPU Graph")
            .icon(gpu_icon)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_main_window(tray.app_handle());
                }
            })
            .build(app);

        if let (Ok(cpu_tray), Ok(gpu_tray)) = (cpu_tray, gpu_tray) {
            *guard = Some(ExtraTrayIcons { cpu_tray, gpu_tray });
            let _ = items.toggle_extra_trays.set_checked(true);
            drop(guard);
            if let Some(state) = app.try_state::<AppState>()
                && let Some(latest) = state.engine.latest()
            {
                let history = state.engine.history(60);
                refresh_tray_visuals(app, &items, &latest, &history);
            }
        } else {
            let _ = items.toggle_extra_trays.set_checked(false);
        }
    }
}

fn toggle_pause<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut s = state.engine.state();
    s.paused = !s.paused;
    if state.engine.set_state(s).is_ok() {
        update_pause_label(app, s.paused);
    }
}

pub fn update_pause_label<R: Runtime>(app: &AppHandle<R>, paused: bool) {
    if let Some(items) = app.try_state::<TrayItems<R>>() {
        let _ = items.pause.set_text(if paused {
            "Resume monitoring"
        } else {
            "Pause monitoring"
        });
    }
}

fn refresh_tray_visuals<R: Runtime>(
    app: &AppHandle<R>,
    items: &TrayItems<R>,
    tick: &MetricsTick,
    history: &[HistoryPoint],
) {
    let cpu_line = format_cpu_line(tick);
    let mem_line = format_mem_line(tick);
    let gpu_line = format_gpu_line(tick);
    let net_line = format_net_line(tick);

    let _ = items.cpu_item.set_text(&cpu_line);
    let _ = items.mem_item.set_text(&mem_line);
    let _ = items.gpu_item.set_text(&gpu_line);
    let _ = items.net_item.set_text(&net_line);

    let tooltip = format!("{cpu_line}\n{mem_line}\n{gpu_line}\n{net_line}");
    let _ = items.tray.set_tooltip(Some(&tooltip));

    let mode = items
        .graph_mode
        .lock()
        .map(|g| *g)
        .unwrap_or(TrayGraphMode::CombinedBars);

    match render_tray_icon(mode, history, &tick.point) {
        Some(rgba) => {
            let _ = items
                .tray
                .set_icon(Some(Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)));
        }
        None => {
            if let Some(default_icon) = app.default_window_icon() {
                let _ = items.tray.set_icon(Some(default_icon.clone()));
            }
        }
    }

    if let Ok(extra_guard) = items.extra_trays.lock()
        && let Some(extra) = extra_guard.as_ref()
    {
        if let Some(cpu_rgba) = render_tray_icon(TrayGraphMode::CpuSparkline, history, &tick.point)
        {
            let _ = extra
                .cpu_tray
                .set_icon(Some(Image::new_owned(cpu_rgba, ICON_SIZE, ICON_SIZE)));
            let _ = extra.cpu_tray.set_tooltip(Some(&cpu_line));
        }
        if let Some(gpu_rgba) = render_tray_icon(TrayGraphMode::GpuSparkline, history, &tick.point)
        {
            let _ = extra
                .gpu_tray
                .set_icon(Some(Image::new_owned(gpu_rgba, ICON_SIZE, ICON_SIZE)));
            let _ = extra.gpu_tray.set_tooltip(Some(&gpu_line));
        }
    }
}

fn format_cpu_line(tick: &MetricsTick) -> String {
    match tick.cpu.frequency_mhz {
        Some(mhz) if mhz > 0 => {
            format!(
                "CPU: {:.0}% · {:.2} GHz",
                tick.cpu.usage_percent,
                mhz as f64 / 1000.0
            )
        }
        _ => format!("CPU: {:.0}%", tick.cpu.usage_percent),
    }
}

fn format_mem_line(tick: &MetricsTick) -> String {
    format!(
        "Memory: {:.0}% ({} / {})",
        tick.memory.used_percent,
        format_bytes(tick.memory.used_bytes),
        format_bytes(tick.memory.total_bytes),
    )
}

fn format_gpu_line(tick: &MetricsTick) -> String {
    let Some(gpu) = tick.gpus.first() else {
        return "GPU: Not detected".to_owned();
    };
    let usage = gpu
        .usage_percent
        .map(|u| format!("{u:.0}%"))
        .unwrap_or_else(|| "—".to_owned());
    let temp = gpu
        .temperature_celsius
        .map(|t| format!(" · {t:.0}°C"))
        .unwrap_or_default();
    if gpu.total_vram_bytes > 0 {
        format!(
            "GPU: {usage}{temp} ({} / {})",
            format_bytes(gpu.used_vram_bytes),
            format_bytes(gpu.total_vram_bytes)
        )
    } else {
        format!("GPU: {usage}{temp}")
    }
}

fn format_net_line(tick: &MetricsTick) -> String {
    format!(
        "Net: ↓ {}  ↑ {}",
        format_rate(tick.point.net_rx),
        format_rate(tick.point.net_tx),
    )
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn format_rate(bytes_per_sec: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes_per_sec.max(0.0);
    if b >= GB {
        format!("{:.2} GB/s", b / GB)
    } else if b >= MB {
        format!("{:.1} MB/s", b / MB)
    } else if b >= KB {
        format!("{:.1} KB/s", b / KB)
    } else {
        format!("{:.0} B/s", b)
    }
}

/// Receives every tick (no JSON): throttled tray summary + dynamic graph icon
/// and alert notifications.
pub struct DesktopSink<R: Runtime> {
    app: AppHandle<R>,
    last_tray_update: AtomicU64,
    notifications_enabled: Arc<AtomicBool>,
}

impl<R: Runtime> DesktopSink<R> {
    pub fn new(app: AppHandle<R>, notifications_enabled: Arc<AtomicBool>) -> Self {
        Self {
            app,
            last_tray_update: AtomicU64::new(0),
            notifications_enabled,
        }
    }
}

impl<R: Runtime> TickSink for DesktopSink<R> {
    fn wants_json(&self) -> bool {
        false
    }

    fn deliver(&self, tick: &MetricsTick, _json: Option<&str>) -> bool {
        let last = self.last_tray_update.load(Ordering::Relaxed);
        if tick.timestamp_ms.saturating_sub(last) >= TRAY_UPDATE_MS {
            self.last_tray_update
                .store(tick.timestamp_ms, Ordering::Relaxed);
            let app = self.app.clone();
            let tick_clone = tick.clone();
            // Post, never wait: the main thread may itself be waiting on us.
            let _ = self.app.run_on_main_thread(move || {
                if let Some(items) = app.try_state::<TrayItems<R>>() {
                    let history = app
                        .try_state::<AppState>()
                        .map(|s| s.engine.history(60))
                        .unwrap_or_default();
                    refresh_tray_visuals(&app, &items, &tick_clone, &history);
                }
            });
        }

        if !tick.alerts.is_empty() && self.notifications_enabled.load(Ordering::Relaxed) {
            let app = self.app.clone();
            let alerts = tick.alerts.clone();
            tauri::async_runtime::spawn_blocking(move || {
                for alert in alerts {
                    if let Err(e) = app
                        .notification()
                        .builder()
                        .title(&alert.rule_name)
                        .body(&alert.message)
                        .show()
                    {
                        tracing::warn!("could not show notification: {e}");
                    }
                }
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_and_rates_cleanly() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16.0 GB");
        assert_eq!(format_rate(942.0), "942 B/s");
        assert_eq!(format_rate(6553.6), "6.4 KB/s");
    }
}
