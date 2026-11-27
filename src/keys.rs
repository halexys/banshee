//! Global shortcuts and the MSI scenario key (F7).
//!
//! Shortcuts are stored as GNOME custom keybindings (the same ones Settings → Keyboard shows),
//! so they work with Banshee closed and both places stay in sync. Each one runs
//! `banshee --<action>`, which hands the action to the running instance.
//!
//! The scenario key never becomes a key event on Linux: msi_wmi receives its WMI event, does
//! not know the code and only logs "Unknown event received". Banshee follows the kernel log
//! and treats such a burst as a press (experimental).

use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const BASE: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/";

/// (id, label shown in GNOME Settings, CLI flag, default accelerator)
pub const ACTIONS: [(&str, &str, &str, &str); 9] = [
    ("cycle", "Banshee: siguiente perfil", "--cycle", "<Control><Alt>p"),
    ("silent", "Banshee: Silencioso", "--profile silent", "<Control><Alt>1"),
    ("balanced", "Banshee: Equilibrado", "--profile balanced", "<Control><Alt>2"),
    ("extreme", "Banshee: Extremo", "--profile extreme", "<Control><Alt>3"),
    ("superbatt", "Banshee: Super Batería", "--profile superbatt", "<Control><Alt>4"),
    ("boost", "Banshee: Fan Boost", "--boost", "<Control><Alt>b"),
    ("curve", "Banshee: curva propia", "--curve", "<Control><Alt>c"),
    ("panel", "Banshee: panel rápido", "--panel", "<Super>g"),
    ("open", "Banshee: abrir", "", ""),
];

fn path(id: &str) -> String {
    format!("{BASE}banshee-{id}/")
}

fn gs_get(schema: &str, key: &str) -> Option<String> {
    let o = Command::new("gsettings").args(["get", schema, key]).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn gs_set(schema: &str, key: &str, val: &str) -> Result<(), String> {
    let ok = Command::new("gsettings").args(["set", schema, key, val]).status().map(|s| s.success()).unwrap_or(false);
    if ok { Ok(()) } else { Err(format!("gsettings set {key}")) }
}

/// GVariant string literal ('...' with escapes).
fn gv(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn list() -> Vec<String> {
    gs_get(SCHEMA, "custom-keybindings")
        .map(|s| s.split('\'').skip(1).step_by(2).map(String::from).collect())
        .unwrap_or_default()
}

fn set_list(v: &[String]) -> Result<(), String> {
    let s = format!("[{}]", v.iter().map(|x| gv(x)).collect::<Vec<_>>().join(", "));
    gs_set(SCHEMA, "custom-keybindings", &s)
}

fn reloc(id: &str) -> String {
    format!("{SCHEMA}.custom-keybinding:{}", path(id))
}

#[derive(Serialize, Clone)]
pub struct Shortcut {
    pub id: &'static str,
    pub accel: String,
}

pub fn get_all() -> Vec<Shortcut> {
    let l = list();
    ACTIONS
        .iter()
        .map(|(id, ..)| {
            let accel = if l.contains(&path(id)) {
                gs_get(&reloc(id), "binding").map(|s| s.trim_matches('\'').to_string()).unwrap_or_default()
            } else {
                String::new()
            };
            Shortcut { id, accel }
        })
        .collect()
}

/// Sets (or with an empty `accel`, removes) the shortcut for action `id`.
pub fn set(id: &str, accel: &str) -> Result<(), String> {
    let (_, name, flag, _) = ACTIONS.iter().find(|a| a.0 == id).ok_or("unknown action")?;
    let p = path(id);
    let mut l = list();
    if accel.is_empty() {
        if l.contains(&p) {
            l.retain(|x| x != &p);
            set_list(&l)?;
        }
        return Ok(());
    }
    // the same keys on another Banshee action would fire both: take them over
    for (other, ..) in ACTIONS.iter().filter(|a| a.0 != id) {
        if l.contains(&path(other))
            && gs_get(&reloc(other), "binding").map(|s| s.trim_matches('\'') == accel).unwrap_or(false)
        {
            l.retain(|x| x != &path(other));
        }
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let cmd = format!("{} {flag}", exe.display()).trim().to_string();
    gs_set(&reloc(id), "name", &gv(name))?;
    gs_set(&reloc(id), "command", &gv(&cmd))?;
    gs_set(&reloc(id), "binding", &gv(accel))?;
    if !l.contains(&p) {
        l.push(p);
    }
    set_list(&l)
}

/// First run on GNOME: put the default shortcuts in place.
pub fn install_defaults() {
    if gs_get(SCHEMA, "custom-keybindings").is_none() {
        return; // not GNOME
    }
    for (id, _, _, accel) in ACTIONS {
        if !accel.is_empty() {
            let _ = set(id, accel);
        }
    }
}

/// Re-points existing shortcuts at this executable (after it moved or was renamed).
pub fn refresh_commands() {
    let l = list();
    let Ok(exe) = std::env::current_exe() else { return };
    for (id, _, flag, _) in ACTIONS {
        if l.contains(&path(id)) {
            let cmd = format!("{} {flag}", exe.display()).trim().to_string();
            if gs_get(&reloc(id), "command").map(|c| c.trim_matches('\'') != cmd).unwrap_or(false) {
                let _ = gs_set(&reloc(id), "command", &gv(&cmd));
            }
        }
    }
}

pub fn open_gnome_settings() {
    let _ = Command::new("gnome-control-center").arg("keyboard").stdin(Stdio::null()).spawn();
}

/// Follows the kernel log and calls `on_press` for each scenario-key press. A press shows up
/// as a short burst of "msi_wmi: Unknown event received"; bursts that come with an unknown
/// keyboard scancode belong to some other key and are ignored. Runs until journalctl exits.
pub fn watch_scenario_key(enabled: impl Fn() -> bool + Send + 'static, on_press: impl Fn() + Send + 'static) {
    std::thread::spawn(move || loop {
        let child = Command::new("journalctl")
            .args(["-k", "-f", "-n", "0", "-o", "cat", "--no-pager"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else { return };
        let Some(out) = child.stdout.take() else { return };
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if line.contains("msi_wmi: Unknown event received") {
                    let _ = tx.send(true);
                } else if line.contains("atkbd") && line.contains("Unknown key") {
                    let _ = tx.send(false);
                }
            }
        });
        let mut burst: Option<(Instant, bool)> = None; // (last event, other key seen)
        let mut last_fire = Instant::now() - Duration::from_secs(10);
        loop {
            match rx.recv_timeout(Duration::from_millis(150)) {
                Ok(wmi) => {
                    let now = Instant::now();
                    let other = burst.map(|b| b.1).unwrap_or(false) || !wmi;
                    burst = Some((now, other));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if let Some((t, other)) = burst {
                        if t.elapsed() > Duration::from_millis(350) {
                            burst = None;
                            if !other && enabled() && last_fire.elapsed() > Duration::from_secs(1) {
                                last_fire = Instant::now();
                                on_press();
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = child.kill();
        std::thread::sleep(Duration::from_secs(5));
    });
}
