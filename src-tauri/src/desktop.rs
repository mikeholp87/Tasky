//! Desktop integration: system tray, alert notifications and main-window
//! visibility.
//!
//! Everything here runs from the engine thread through [`DesktopSink`] and
//! must never block it: tray updates are *posted* to the main thread and
//! notifications are shown on a blocking-pool thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;
use tm_core::models::MetricsTick;
use tm_core::monitoring::TickSink;

use crate::state::AppState;

pub const MAIN_WINDOW: &str = "main";
const TRAY_UPDATE_MS: u64 = 2_000;

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
        let _ = window.unminimize();
        let _ = window.show();
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
        let _ = window.hide();
    }
    if let Some(state) = app.try_state::<AppState>() {
        state.visibility.set(false);
    }
}

/// Tray menu items whose text changes at runtime.
pub struct TrayItems<R: Runtime> {
    pub tray: TrayIcon<R>,
    pub pause: MenuItem<R>,
    pub summary: MenuItem<R>,
}

pub fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayItems<R>> {
    let open = MenuItem::with_id(app, "open", "Open Tasky", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause monitoring", true, None::<&str>)?;
    let summary = MenuItem::with_id(app, "summary", "CPU — | RAM —", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &summary,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Tasky")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
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
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    Ok(TrayItems {
        tray,
        pause,
        summary,
    })
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
    if let Some(state) = app.try_state::<AppState>() {
        let _ = app.emit("monitoring-state-changed", state.engine.state());
    }
    if let Some(items) = app.try_state::<TrayItems<R>>() {
        let _ = items.pause.set_text(if paused {
            "Resume monitoring"
        } else {
            "Pause monitoring"
        });
    }
}

/// Receives every tick (no JSON): throttled tray summary and alert
/// notifications.
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
            let text = format!(
                "CPU {:.0}% | RAM {:.0}%",
                tick.cpu.usage_percent, tick.memory.used_percent
            );
            let app = self.app.clone();
            // Post, never wait: the main thread may itself be waiting on us.
            let _ = self.app.run_on_main_thread(move || {
                if let Some(items) = app.try_state::<TrayItems<R>>() {
                    let _ = items.tray.set_tooltip(Some(&text));
                    let _ = items.summary.set_text(&text);
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
