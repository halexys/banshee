//! Tray icon: a ghost tinted with the active profile colour, plus a profile menu.

use crate::hw::{Profile, Status};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "main";
const S: u32 = 64;

/// Draws the ghost (same outline as the app logo) at 64x64.
pub fn ghost(rgb: [u8; 3]) -> Image<'static> {
    let mut px = vec![0u8; (S * S * 4) as usize];
    let (cx, top_cy, r) = (32.0f32, 28.0f32, 21.0f32);
    for y in 0..S {
        for x in 0..S {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let in_head = fy <= top_cy && (fx - cx).powi(2) + (fy - top_cy).powi(2) <= r * r;
            let in_body = fy > top_cy && fx >= cx - r && fx <= cx + r && fy <= 52.0;
            // three scallops along the bottom edge
            let in_tail = fy > 52.0 && fy <= 60.0 && fx >= cx - r && fx <= cx + r && {
                let seg = (fx - (cx - r)) / (2.0 * r / 3.0);
                let t = seg.fract();
                let depth = 52.0 + 8.0 * (1.0 - (2.0 * t - 1.0).abs());
                fy <= depth
            };
            let eye = |ex: f32| (fx - ex).powi(2) + (fy - 27.0).powi(2) <= 16.0;
            if (in_head || in_body || in_tail) && !eye(24.0) && !eye(40.0) {
                let i = ((y * S + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
    }
    Image::new_owned(px, S, S)
}

fn menu<R: Runtime>(app: &AppHandle<R>, st: &Status) -> tauri::Result<Menu<R>> {
    let m = Menu::new(app)?;
    m.append(&MenuItem::with_id(app, "panel", "Panel rápido", true, None::<&str>)?)?;
    m.append(&PredefinedMenuItem::separator(app)?)?;
    for p in Profile::ALL {
        let item = CheckMenuItem::with_id(app, format!("p:{}", p.id()), p.name(), st.writable, st.profile == Some(p), None::<&str>)?;
        m.append(&item)?;
    }
    m.append(&PredefinedMenuItem::separator(app)?)?;
    m.append(&CheckMenuItem::with_id(app, "boost", "Fan Boost", st.writable, st.cooler_boost, None::<&str>)?)?;
    m.append(&PredefinedMenuItem::separator(app)?)?;
    m.append(&MenuItem::with_id(app, "open", "Abrir Banshee", true, None::<&str>)?)?;
    m.append(&MenuItem::with_id(app, "panic", "Reset de pánico", st.writable, None::<&str>)?)?;
    m.append(&MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?)?;
    Ok(m)
}

fn tooltip(st: &Status) -> String {
    let p = st.profile.map(|p| p.name()).unwrap_or("Personalizado");
    match st.cpu_temp {
        Some(t) => format!("Banshee · {p} · CPU {t} °C"),
        None => format!("Banshee · {p}"),
    }
}

fn color(st: &Status) -> [u8; 3] {
    st.profile.map(|p| p.color()).unwrap_or([0x8B, 0x97, 0xA3])
}

pub fn create<R: Runtime>(app: &AppHandle<R>, st: &Status) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(ghost(color(st)))
        .tooltip(tooltip(st))
        .menu(&menu(app, st)?)
        .on_menu_event(|app, ev| crate::on_tray_menu(app, ev.id.as_ref()))
        .build(app)?;
    Ok(())
}

/// Rebuilds icon and menu (only called when profile or Fan Boost changed).
pub fn refresh<R: Runtime>(app: &AppHandle<R>, st: &Status) {
    if let Some(t) = app.tray_by_id(TRAY_ID) {
        let _ = t.set_icon(Some(ghost(color(st))));
        let _ = t.set_tooltip(Some(tooltip(st)));
        if let Ok(m) = menu(app, st) {
            let _ = t.set_menu(Some(m));
        }
    }
}

/// Shows the quick panel in the top-right corner, under GNOME's top bar.
pub fn show_panel<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window("panel") else { return };
    if let Ok(Some(m)) = w.current_monitor().or_else(|_| w.primary_monitor()) {
        let sc = m.scale_factor();
        let (mx, my) = (m.position().x, m.position().y);
        let mw = m.size().width as i32;
        let pw = (300.0 * sc) as i32;
        let x = mx + mw - pw - (12.0 * sc) as i32;
        let y = my + (40.0 * sc) as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = w.show();
    let _ = w.set_focus();
}

static PANEL_HIDDEN_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Remember when the panel went away: a click on the top-bar ghost first takes focus from the
/// panel (which hides it) and only then asks to toggle, which must not reopen it.
pub fn panel_hidden_now() {
    *PANEL_HIDDEN_AT.lock().unwrap() = Some(std::time::Instant::now());
}

pub fn hide_panel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
        panel_hidden_now();
    }
}

pub fn toggle_panel<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window("panel") else { return };
    if w.is_visible().unwrap_or(false) {
        hide_panel(app);
        return;
    }
    let just_hidden = PANEL_HIDDEN_AT.lock().unwrap().map(|t| t.elapsed().as_millis() < 400).unwrap_or(false);
    if !just_hidden {
        show_panel(app);
    }
}

/// The extension draws its own icon: hide (or bring back) the AppIndicator one.
pub fn set_visible<R: Runtime>(app: &AppHandle<R>, on: bool) {
    if let Some(t) = app.tray_by_id(TRAY_ID) {
        let _ = t.set_visible(on);
    }
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
    hide_panel(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
