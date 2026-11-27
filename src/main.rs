// Banshee - MSI Prestige 16 AI Studio B1VFG control panel.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod extension;
mod hw;
mod setup;
mod tray;

use config::Config;
use hw::{Curve, Profile, Sampler, Status};
use serde::Serialize;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Manager, Runtime, State, WindowEvent};

struct App {
    sampler: Sampler,
    status: Mutex<Status>,
    cfg: Mutex<Config>,
    /// last time the GNOME extension polled us (it then owns the top-bar icon)
    ext_seen: Mutex<Option<std::time::Instant>>,
    tray_hidden: std::sync::atomic::AtomicBool,
}

type Shared = Arc<App>;

#[derive(Serialize)]
struct View {
    status: Status,
    charge_limit: u8,
    travel_active: bool,
    travel_until: Option<u64>,
    fans: FansView,
}

#[derive(Serialize)]
struct FansView {
    curve: Curve,
    factory: Curve,
    profiles: Vec<Profile>,
    max: u8,
}

// ---------- shared logic (UI commands, tray and CLI all go through here) ----------

impl App {
    fn resample(&self) -> Status {
        let s = self.sampler.sample();
        *self.status.lock().unwrap() = s.clone();
        s
    }

    fn set_profile(&self, p: Profile) -> Result<(), String> {
        hw::apply_profile(p)?;
        let mut c = self.cfg.lock().unwrap();
        c.profile = p;
        c.save();
        c.sync_fans(p)
    }

    /// Known-good state: Balanced, MSI's own fan table, no boost (the curve stays saved).
    fn panic_reset(&self) -> Result<(), String> {
        let _ = hw::set_cooler_boost(false);
        hw::apply_profile(Profile::Balanced)?;
        let mut c = self.cfg.lock().unwrap();
        c.profile = Profile::Balanced;
        c.save();
        hw::release_curve(Profile::Balanced)
    }
}

fn refresh_ui<R: Runtime>(app: &AppHandle<R>, st: &Shared) {
    let s = st.resample();
    tray::refresh(app, &s);
}

pub fn on_tray_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let st = app.state::<Shared>().inner().clone();
    let res = match id {
        "open" => {
            tray::show_main(app);
            Ok(())
        }
        "panel" => {
            tray::show_panel(app);
            Ok(())
        }
        "quit" => {
            app.exit(0);
            Ok(())
        }
        "panic" => st.panic_reset(),
        "boost" => {
            let on = st.status.lock().unwrap().cooler_boost;
            hw::set_cooler_boost(!on)
        }
        p if p.starts_with("p:") => match Profile::from_id(&p[2..]) {
            Some(p) => st.set_profile(p),
            None => Ok(()),
        },
        _ => Ok(()),
    };
    if let Err(e) = res {
        eprintln!("tray action {id}: {e}");
    }
    refresh_ui(app, &st);
}

// ---------- commands for the web UI ----------

fn view(st: &App) -> View {
    let c = st.cfg.lock().unwrap();
    View {
        status: st.status.lock().unwrap().clone(),
        charge_limit: c.charge_limit,
        travel_active: c.travel_active(),
        travel_until: c.travel_until.filter(|_| c.travel_active()),
        fans: FansView { curve: c.curve, factory: hw::FACTORY, profiles: c.curve_profiles.clone(), max: hw::FAN_MAX },
    }
}

#[tauri::command]
fn get_status(st: State<Shared>) -> View {
    view(&st)
}

#[tauri::command]
fn set_profile(app: AppHandle, st: State<Shared>, id: String) -> Result<View, String> {
    let p = Profile::from_id(&id).ok_or("unknown profile")?;
    st.set_profile(p)?;
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn open_main(app: AppHandle) {
    tray::show_main(&app);
}

#[tauri::command]
fn panic_reset(app: AppHandle, st: State<Shared>) -> Result<View, String> {
    st.panic_reset()?;
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_boost(app: AppHandle, st: State<Shared>, on: bool) -> Result<View, String> {
    hw::set_cooler_boost(on)?;
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_kbd(app: AppHandle, st: State<Shared>, level: u8) -> Result<View, String> {
    hw::set_kbd(level)?;
    {
        let mut c = st.cfg.lock().unwrap();
        c.kbd = Some(level.min(3));
        c.save();
    }
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_charge(app: AppHandle, st: State<Shared>, pct: u8) -> Result<View, String> {
    hw::set_charge_limit(pct)?;
    {
        let mut c = st.cfg.lock().unwrap();
        c.charge_limit = pct.clamp(20, 100);
        c.travel_until = None;
        c.save();
    }
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_travel(app: AppHandle, st: State<Shared>, on: bool) -> Result<View, String> {
    {
        let mut c = st.cfg.lock().unwrap();
        c.travel_until = on.then(|| config::now() + config::TRAVEL_DAYS * 86_400);
        hw::set_charge_limit(c.effective_limit())?;
        c.save();
    }
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_fn(app: AppHandle, st: State<Shared>, right: bool) -> Result<View, String> {
    hw::set_fn_side(right)?;
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_curve(app: AppHandle, st: State<Shared>, curve: Curve) -> Result<View, String> {
    {
        let mut c = st.cfg.lock().unwrap();
        c.curve = curve.sanitized();
        c.save();
        let p = hw::current_profile().unwrap_or(c.profile);
        if c.curve_for(p) {
            hw::engage_curve(&c.curve)?;
        }
    }
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
fn set_curve_profile(app: AppHandle, st: State<Shared>, id: String, on: bool) -> Result<View, String> {
    let p = Profile::from_id(&id).ok_or("unknown profile")?;
    if p == Profile::Silent {
        return Err("Silencioso usa siempre la curva de fábrica".into());
    }
    {
        let mut c = st.cfg.lock().unwrap();
        c.curve_profiles.retain(|x| *x != p);
        if on {
            c.curve_profiles.push(p);
        }
        c.save();
        if hw::current_profile() == Some(p) {
            c.sync_fans(p)?;
        }
    }
    refresh_ui(&app, &st);
    Ok(view(&st))
}

fn pkexec_self(args: &[&str]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = Command::new("pkexec").arg(exe).args(args).output().map_err(|e| format!("pkexec: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => Err("CANCELLED".into()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

#[tauri::command]
async fn set_gpu_mode(app: AppHandle, igpu: bool) -> Result<View, String> {
    pkexec_self(&["--gpu-mode", if igpu { "igpu" } else { "hybrid" }])?;
    let st = app.state::<Shared>().inner().clone();
    refresh_ui(&app, &st);
    Ok(view(&st))
}

#[tauri::command]
async fn run_setup(app: AppHandle) -> Result<View, String> {
    pkexec_self(&["--setup"])?;
    let st = app.state::<Shared>().inner().clone();
    std::thread::sleep(Duration::from_millis(500));
    {
        let mut c = st.cfg.lock().unwrap();
        if let Err(e) = c.apply() {
            eprintln!("apply after setup: {e}");
        }
    }
    refresh_ui(&app, &st);
    Ok(view(&st))
}

// ---------- background sampler ----------

fn spawn_poller<R: Runtime>(app: AppHandle<R>, st: Shared) {
    std::thread::spawn(move || {
        let mut last_wall = SystemTime::now();
        let mut last_key: Option<(Option<Profile>, bool, bool)> = None;
        let mut last_engage = std::time::Instant::now() - Duration::from_secs(60);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = SystemTime::now();
            let gap = now.duration_since(last_wall).unwrap_or_default();
            last_wall = now;

            // Woke from sleep (the EC may come back in other modes): re-assert settings.
            // Also catches travel mode expiring.
            let need_apply = gap > Duration::from_secs(15) || {
                let c = st.cfg.lock().unwrap();
                c.travel_until.is_some() && !c.travel_active()
            };
            if need_apply && hw::writable() {
                let mut c = st.cfg.lock().unwrap();
                if c.restore_on_start {
                    if let Err(e) = c.apply() {
                        eprintln!("re-apply: {e}");
                    }
                }
            }

            let mut s = st.resample();

            // Keep fans matched to the profile when it changes behind our back (Fn+F7, CLI) and
            // when the EC drops advanced mode on its own (rate-limited, in case it refuses it).
            if s.writable {
                if let Some(p) = s.profile {
                    let mut c = st.cfg.lock().unwrap();
                    let changed = c.profile != p;
                    let dropped = c.curve_for(p) && !s.fan_curve && last_engage.elapsed() > Duration::from_secs(10);
                    if changed || dropped {
                        if changed {
                            c.profile = p;
                            c.save();
                        }
                        if c.curve_for(p) {
                            last_engage = std::time::Instant::now();
                        }
                        if let Err(e) = c.sync_fans(p) {
                            eprintln!("fan sync: {e}");
                        }
                        drop(c);
                        s = st.resample();
                    }
                }
            }
            // extension gone (disabled, shell restarted without it): bring our tray icon back
            let ext_alive = st.ext_seen.lock().unwrap().map(|t| t.elapsed() < Duration::from_secs(8)).unwrap_or(false);
            if !ext_alive && st.tray_hidden.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let h = app.clone();
                let _ = app.run_on_main_thread(move || tray::set_visible(&h, true));
            }

            let key = (s.profile, s.cooler_boost, s.writable);
            if last_key != Some(key) {
                tray::refresh(&app, &s);
                last_key = Some(key);
            }
        }
    });
}

// ---------- CLI ----------

fn cli(args: &[String]) -> Option<i32> {
    let a = args.get(1)?.as_str();
    let res: Result<(), String> = match a {
        "--setup" => setup::run(),
        "--gpu-mode" => match args.get(2).map(|s| s.as_str()) {
            Some("igpu") => hw::write_gpu_mode(true),
            Some("hybrid") => hw::write_gpu_mode(false),
            _ => Err("usage: --gpu-mode igpu|hybrid".into()),
        },
        "--profile" | "--cycle" => {
            let mut c = Config::load();
            let p = if a == "--cycle" {
                Some(hw::current_profile().unwrap_or(c.profile).next())
            } else {
                args.get(2).and_then(|s| Profile::from_id(s))
            };
            match p {
                Some(p) => hw::apply_profile(p).and_then(|_| {
                    c.profile = p;
                    c.save();
                    println!("{}", p.name());
                    c.sync_fans(p)
                }),
                None => Err("usage: --profile silent|balanced|extreme|superbatt".into()),
            }
        }
        "--status" => {
            println!("{}", serde_json::to_string_pretty(&Sampler::new().sample()).unwrap_or_default());
            Ok(())
        }
        _ => return None,
    };
    Some(match res {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("banshee: {e}");
            1
        }
    })
}

// ---------- single instance ----------

fn sock_path() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    std::path::PathBuf::from(dir).join("banshee.sock")
}

/// Returns false (after asking it to show its window) when another instance is running.
fn claim_instance(show: bool) -> Option<std::os::unix::net::UnixListener> {
    use std::io::Write;
    use std::os::unix::net::{UnixListener, UnixStream};
    let p = sock_path();
    if let Ok(mut s) = UnixStream::connect(&p) {
        let _ = s.write_all(if show { b"show" } else { b"noop" });
        return None;
    }
    let _ = std::fs::remove_file(&p);
    UnixListener::bind(&p).ok()
}

/// What the top-bar extension shows: profile colour and name, CPU/GPU temperatures.
fn ext_state(st: &App) -> String {
    let s = st.status.lock().unwrap().clone();
    let [r, g, b] = s.profile.map(|p| p.color()).unwrap_or([0x8B, 0x97, 0xA3]);
    serde_json::json!({
        "color": format!("#{r:02X}{g:02X}{b:02X}"),
        "name": s.profile.map(|p| p.name()).unwrap_or("Personalizado"),
        "cpu": s.cpu_temp,
        "gpu": if s.gpu.awake { s.gpu.temp } else { None },
        "boost": s.cooler_boost,
    })
    .to_string()
}

/// One command per connection ("show", "toggle", "state", "noop"), newline optional.
fn serve_instance<R: Runtime>(app: AppHandle<R>, l: std::os::unix::net::UnixListener) {
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for mut s in l.incoming().flatten() {
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 32];
            let mut n = 0;
            while n < buf.len() {
                match s.read(&mut buf[n..]) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => {
                        n += k;
                        if buf[..n].contains(&b'\n') {
                            break;
                        }
                    }
                }
            }
            let cmd = String::from_utf8_lossy(&buf[..n]).trim().to_string();
            let h = app.clone();
            match cmd.as_str() {
                "show" => {
                    let _ = app.run_on_main_thread(move || tray::show_main(&h));
                }
                "toggle" => {
                    let _ = app.run_on_main_thread(move || tray::toggle_panel(&h));
                    let _ = s.write_all(b"ok\n");
                }
                "state" => {
                    let st = app.state::<Shared>().inner().clone();
                    *st.ext_seen.lock().unwrap() = Some(std::time::Instant::now());
                    if !st.tray_hidden.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let _ = app.run_on_main_thread(move || tray::set_visible(&h, false));
                    }
                    let _ = s.write_all(format!("{}\n", ext_state(&st)).as_bytes());
                }
                _ => {}
            }
        }
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = cli(&args) {
        std::process::exit(code);
    }
    let start_hidden = args.iter().any(|a| a == "--tray");
    // GNOME's Wayland session ignores client window positions; under XWayland the
    // quick panel can sit in the top-right corner next to the tray icon.
    if std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    let Some(listener) = claim_instance(!start_hidden) else {
        return;
    };

    let mut cfg = Config::load();
    if cfg.restore_on_start && hw::writable() {
        if let Err(e) = cfg.apply() {
            eprintln!("restore: {e}");
        }
    }
    let shared: Shared = Arc::new(App {
        sampler: Sampler::new(),
        status: Mutex::new(Status::default()),
        cfg: Mutex::new(cfg),
        ext_seen: Mutex::new(None),
        tray_hidden: std::sync::atomic::AtomicBool::new(false),
    });
    extension::install();
    let first = shared.resample();

    tauri::Builder::default()
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            get_status, open_main, panic_reset, set_profile, set_boost, set_kbd, set_charge, set_travel, set_fn, set_curve, set_curve_profile, set_gpu_mode, run_setup
        ])
        .setup(move |app| {
            let h = app.handle().clone();
            tray::create(&h, &first)?;
            spawn_poller(h.clone(), shared.clone());
            serve_instance(h.clone(), listener);
            if !start_hidden {
                tray::show_main(&h);
            }
            Ok(())
        })
        .on_window_event(|w, ev| match ev {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = w.hide();
            }
            // the quick panel behaves like a popup: gone as soon as it loses focus
            WindowEvent::Focused(false) if w.label() == "panel" => {
                let _ = w.hide();
                tray::panel_hidden_now();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running Banshee");
}
