//! The menu-bar icon and what its two mouse buttons do.
//!
//! Left click drops the panel down under the icon; right click opens a three-line
//! system menu. That split is the convention every menu-bar app follows, and it
//! is what lets the panel hold the settings and the cheat sheet without a menu
//! item apiece — the icon itself is the way in.

use crate::debug_log;
use std::sync::Mutex;
use std::time::Instant;
use tauri::{
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

/// The label of the panel window in tauri.conf.json.
pub const PANEL: &str = "panel";

/// The size the panel is authored at. The interface scale grows the window from
/// here, so zoomed content is not clipped by a window that stayed at 1×.
pub const PANEL_SIZE: (f64, f64) = (420.0, 484.0);

/// Breathing room between the tray icon and the panel, in logical pixels.
const TRAY_GAP: f64 = 6.0;

/// How long after an auto-hide a tray click still counts as "the click that
/// hid it". Clicking the icon takes focus away from the panel, which hides it
/// before this handler runs — without the guard the handler would then see a
/// hidden panel and put it straight back up, and the icon would never close it.
const AUTO_HIDE_GRACE_MS: u128 = 400;

static LAST_AUTO_HIDE: Mutex<Option<Instant>> = Mutex::new(None);

pub fn note_auto_hide() {
    if let Ok(mut t) = LAST_AUTO_HIDE.lock() {
        *t = Some(Instant::now());
    }
}

fn just_auto_hid() -> bool {
    LAST_AUTO_HIDE
        .lock()
        .ok()
        .and_then(|t| *t)
        .map(|t| t.elapsed().as_millis() < AUTO_HIDE_GRACE_MS)
        .unwrap_or(false)
}

/// A rectangle in physical pixels with a top-left origin — the space both tray
/// icon rects and window positions are reported in.
#[derive(Clone, Copy)]
pub struct PixelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Centred under the icon, flipped above it when it would run off the bottom
/// (a Windows taskbar sits down there), and kept on the screen either way.
pub fn popover_position(
    icon: PixelRect,
    win_w: f64,
    win_h: f64,
    screen: Option<PixelRect>,
    gap: f64,
) -> (f64, f64) {
    let mut x = icon.x + icon.w / 2.0 - win_w / 2.0;
    let mut y = icon.y + icon.h + gap;
    if let Some(s) = screen {
        if y + win_h > s.y + s.h {
            y = (icon.y - gap - win_h).max(s.y + gap);
        }
        let leftmost = s.x + gap;
        let rightmost = (s.x + s.w - win_w - gap).max(leftmost);
        x = x.clamp(leftmost, rightmost);
    }
    (x, y)
}

fn anchor_to_tray(w: &tauri::WebviewWindow, rect: tauri::Rect) {
    let scale = w.scale_factor().unwrap_or(1.0);
    let pos = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    let icon = PixelRect { x: pos.x, y: pos.y, w: size.width, h: size.height };
    let Ok(win) = w.outer_size() else { return };
    let screen = w
        .monitor_from_point(icon.x, icon.y)
        .ok()
        .flatten()
        .or_else(|| w.current_monitor().ok().flatten())
        .map(|m| PixelRect {
            x: m.position().x as f64,
            y: m.position().y as f64,
            w: m.size().width as f64,
            h: m.size().height as f64,
        });
    // The gap is a logical measure; everything else here is physical.
    let (x, y) = popover_position(icon, win.width as f64, win.height as f64, screen, TRAY_GAP * scale);
    let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Grows the panel to the chosen interface scale before it is shown, so the
/// zoomed content still fits the frame.
fn size_to_scale(app: &AppHandle, w: &tauri::WebviewWindow) {
    let scale = app
        .try_state::<crate::SettingsState>()
        .and_then(|c| c.current.lock().ok().map(|s| s.ui_scale))
        .unwrap_or(crate::settings::DEFAULT_UI_SCALE) as f64;
    let (bw, bh) = PANEL_SIZE;
    let _ = w.set_size(tauri::LogicalSize::new(bw * scale, bh * scale));
}

pub fn toggle_panel(app: &AppHandle, rect: tauri::Rect) {
    let Some(w) = app.get_webview_window(PANEL) else {
        // A destroyed window silently does nothing, which reads to the user as a
        // dead icon. Say so in the log.
        debug_log::log("tray: the panel window is gone, cannot show it");
        return;
    };
    if crate::mac_window::popover_visible(app) || just_auto_hid() {
        crate::mac_window::hide_popover(app);
        return;
    }
    size_to_scale(app, &w);
    anchor_to_tray(&w, rect);
    crate::mac_window::show_popover(app);
}

/// Update first, then the version, then quit. Settings and the cheat sheet are
/// not here: the left click is their way in.
pub fn build(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let version = MenuItem::with_id(
        app,
        "version",
        format!("Iago v{}", env!("CARGO_PKG_VERSION")),
        false,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Iago", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&update)
        .separator()
        .item(&version)
        .item(&quit)
        .build()?;

    // announce_update() rewrites this item's text when a release lands.
    app.manage(update.clone());

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Iago — clipboard history (⌥V)")
        .menu(&menu)
        // The menu belongs to the right button alone; the left one is handled
        // below, or the panel and the menu would fight over the same click.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                // On the release, not the press: the press is also what takes
                // focus off an open panel, and acting on both toggles twice.
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), rect);
            }
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    crate::on_update_clicked(app).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: PixelRect = PixelRect { x: 0.0, y: 0.0, w: 1440.0, h: 900.0 };

    #[test]
    fn the_panel_hangs_centred_under_the_icon() {
        let icon = PixelRect { x: 700.0, y: 0.0, w: 24.0, h: 24.0 };
        let (x, y) = popover_position(icon, 420.0, 484.0, Some(SCREEN), 6.0);
        assert_eq!(x, 700.0 + 12.0 - 210.0);
        assert_eq!(y, 30.0);
    }

    #[test]
    fn an_icon_at_the_bottom_of_the_screen_puts_the_panel_above_it() {
        // Windows: the tray sits on a taskbar along the bottom edge.
        let icon = PixelRect { x: 700.0, y: 870.0, w: 24.0, h: 24.0 };
        let (_, y) = popover_position(icon, 420.0, 484.0, Some(SCREEN), 6.0);
        assert_eq!(y, 870.0 - 6.0 - 484.0);
    }

    #[test]
    fn an_icon_near_the_right_edge_does_not_push_the_panel_off_screen() {
        let icon = PixelRect { x: 1420.0, y: 0.0, w: 24.0, h: 24.0 };
        let (x, _) = popover_position(icon, 420.0, 484.0, Some(SCREEN), 6.0);
        assert_eq!(x, 1440.0 - 420.0 - 6.0);
    }

    #[test]
    fn a_screen_narrower_than_the_panel_still_leaves_it_on_screen() {
        let narrow = PixelRect { x: 0.0, y: 0.0, w: 320.0, h: 900.0 };
        let icon = PixelRect { x: 300.0, y: 0.0, w: 24.0, h: 24.0 };
        let (x, _) = popover_position(icon, 420.0, 484.0, Some(narrow), 6.0);
        assert_eq!(x, 6.0, "clamping must not invert into a position off the left edge");
    }

    #[test]
    fn with_no_screen_to_read_the_panel_still_lands_under_the_icon() {
        let icon = PixelRect { x: 700.0, y: 0.0, w: 24.0, h: 24.0 };
        let (x, y) = popover_position(icon, 420.0, 484.0, None, 6.0);
        assert_eq!((x, y), (502.0, 30.0));
    }
}
