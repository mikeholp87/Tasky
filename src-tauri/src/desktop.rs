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
use tauri::{AppHandle, Manager, Runtime};
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

/// Wayland lets an app neither read nor set its own window position, so on
/// GNOME (whose compositor does not remember positions either) a restored
/// window always opens wherever the shell puts it. XWayland windows can be
/// positioned, so at 100% scaling, where XWayland looks identical, Tasky runs
/// through it and remembers position as well as size. Fractional or HiDPI
/// scaling would make an X11 window blurry or tiny, so those stay on native
/// Wayland (size is still remembered). Setting `GDK_BACKEND` yourself wins.
#[cfg(target_os = "linux")]
fn gnome_wayland_with_xwayland(
    session: Option<&str>,
    desktop: Option<&str>,
    display: Option<&str>,
    backend: Option<&str>,
) -> bool {
    backend.is_none()
        && session == Some("wayland")
        && desktop.is_some_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("gnome")))
        && display.is_some_and(|d| !d.is_empty())
}

/// Whether every monitor is at exactly 100%. Unknown (empty) counts as no.
#[cfg(target_os = "linux")]
fn all_unscaled(scales: &[f64]) -> bool {
    !scales.is_empty() && scales.iter().all(|&s| (s - 1.0).abs() < f64::EPSILON)
}

#[cfg(target_os = "linux")]
fn parse_monitor_scales(state: &gio::glib::Variant) -> Option<Vec<f64>> {
    let logical = state.try_child_value(2)?;
    if !logical.is_container() {
        return None;
    }
    (0..logical.n_children())
        .map(|i| logical.try_child_value(i)?.try_child_value(2)?.get::<f64>())
        .collect()
}

/// Scale factor of every logical monitor, from GNOME's display service;
/// empty when it cannot be asked.
#[cfg(target_os = "linux")]
fn monitor_scales() -> Vec<f64> {
    let ask = || -> Result<Vec<f64>, gio::glib::Error> {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
        // GetCurrentState -> (serial, monitors, logical monitors, properties).
        let state = bus.call_sync(
            Some("org.gnome.Mutter.DisplayConfig"),
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
            None,
            None,
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        )?;
        Ok(parse_monitor_scales(&state).unwrap_or_default())
    };
    ask().unwrap_or_default()
}

/// Call first thing in `run`, before GTK opens a display.
#[cfg(target_os = "linux")]
pub fn prefer_x11_on_gnome_wayland() {
    let var = |k: &str| std::env::var(k).ok();
    let (session, desktop) = (var("XDG_SESSION_TYPE"), var("XDG_CURRENT_DESKTOP"));
    let (display, backend) = (var("DISPLAY"), var("GDK_BACKEND"));
    // The bus is only asked on a GNOME Wayland session that has XWayland.
    if gnome_wayland_with_xwayland(
        session.as_deref(),
        desktop.as_deref(),
        display.as_deref(),
        backend.as_deref(),
    ) && all_unscaled(&monitor_scales())
    {
        // GIO starts threads; select the backend without changing the environment.
        gdk::set_allowed_backends("x11,wayland");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn prefer_x11_on_gnome_wayland() {}

/// How long a window that starts hidden waits for a tray to appear. Right
/// after login the shell may not have started its tray yet.
const TRAY_WAIT_SECS: u64 = 15;

/// Whether something on this desktop shows tray icons: an owner of the
/// StatusNotifier watcher name, which KDE, Xfce and GNOME's AppIndicator
/// extension all provide. Stock GNOME does not, so a window hidden "to the
/// tray" there could never be brought back. If the bus cannot be asked, the
/// answer is no: the safe failure is a window that stays visible.
#[cfg(target_os = "linux")]
pub fn tray_host_available() -> bool {
    use gio::glib::{ToVariant, VariantTy};

    let ask = || -> Result<bool, gio::glib::Error> {
        let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
        let reply = bus.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&("org.kde.StatusNotifierWatcher",).to_variant()),
            Some(VariantTy::new("(b)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        )?;
        Ok(reply.get::<(bool,)>().is_some_and(|(owned,)| owned))
    };
    ask().unwrap_or(false)
}

/// Other platforms always have a tray or taskbar to come back from.
#[cfg(not(target_os = "linux"))]
pub fn tray_host_available() -> bool {
    true
}

/// For a window that starts hidden in the tray: if no tray ever appears,
/// show the window rather than leave the app running with no way to reach it.
pub fn show_window_if_no_tray<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        for _ in 0..TRAY_WAIT_SECS {
            if tray_host_available() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        tracing::warn!("no system tray is available; showing the window instead of hiding it");
        show_main_window(&app);
    });
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
    let open = MenuItem::with_id(app, "open", "Open Task Manager", true, None::<&str>)?;
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
        .tooltip("Task Manager")
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{all_unscaled, gnome_wayland_with_xwayland as gnome};

    const WAYLAND: Option<&str> = Some("wayland");
    const GNOME: Option<&str> = Some("ubuntu:GNOME");
    const X: Option<&str> = Some(":0");

    #[test]
    fn uses_xwayland_only_on_gnome_wayland_with_xwayland() {
        assert!(gnome(WAYLAND, GNOME, X, None));
        assert!(!gnome(WAYLAND, Some("KDE"), X, None), "KDE");
        assert!(!gnome(Some("x11"), GNOME, X, None), "already X11");
        assert!(!gnome(WAYLAND, GNOME, None, None), "no XWayland");
        assert!(!gnome(WAYLAND, GNOME, X, Some("wayland")), "user chose");
    }

    /// Needs a live GNOME session: `cargo test -- --ignored reads_real`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs a running GNOME session"]
    fn reads_real_monitor_scales() {
        let scales = super::monitor_scales();
        assert!(!scales.is_empty(), "no logical monitors read");
        assert!(scales.iter().all(|&s| s >= 1.0), "{scales:?}");
    }

    #[test]
    fn only_a_100_percent_setup_qualifies() {
        assert!(all_unscaled(&[1.0, 1.0]));
        // Fractional or HiDPI scaling would blur or shrink an X11 window.
        assert!(!all_unscaled(&[1.0, 1.5]) && !all_unscaled(&[2.0]));
        // The display service could not be asked: stay on native Wayland.
        assert!(!all_unscaled(&[]));
    }

    #[test]
    fn rejects_missing_or_unknown_monitor_scales() {
        use gio::glib::{ToVariant, Variant};

        let reply = |monitors: Vec<Variant>| {
            Variant::tuple_from_iter([
                0u32.to_variant(),
                ().to_variant(),
                Variant::tuple_from_iter(monitors),
            ])
        };
        let good = (0, 0, 1.0f64).to_variant();
        assert_eq!(
            super::parse_monitor_scales(&reply(vec![good.clone()])),
            Some(vec![1.0])
        );
        assert_eq!(super::parse_monitor_scales(&false.to_variant()), None);
        assert_eq!(
            super::parse_monitor_scales(&(0, 0, false).to_variant()),
            None
        );
        assert_eq!(
            super::parse_monitor_scales(&reply(vec![good, (0, 0, "unknown").to_variant()])),
            None
        );
    }
}
