//! The GNOME Shell extension (top-bar ghost + temperatures) lives inside this binary and is
//! written to the user's extensions folder on start, so the app stays a single executable.
//! GNOME on Wayland only loads a new extension at the next login.

use std::path::PathBuf;
use std::process::Command;

pub const UUID: &str = "banshee@halexys.dev";
/// The same extension under the app's previous name; removed on start.
const OLD_UUID: &str = "ghostdeck@halexys.dev";
const FILES: [(&str, &str); 3] = [
    ("metadata.json", include_str!("../extension/metadata.json")),
    ("extension.js", include_str!("../extension/extension.js")),
    ("banshee-symbolic.svg", include_str!("../extension/banshee-symbolic.svg")),
];

fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").map(|d| d.to_uppercase().contains("GNOME")).unwrap_or(false)
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("gnome-shell/extensions").join(UUID))
}

fn gsettings_list(key: &str) -> Option<Vec<String>> {
    let out = Command::new("gsettings").args(["get", "org.gnome.shell", key]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    Some(s.split('\'').skip(1).step_by(2).map(String::from).collect())
}

fn gsettings_set(key: &str, v: &[String]) {
    let list = format!("[{}]", v.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", "));
    let _ = Command::new("gsettings").args(["set", "org.gnome.shell", key, &list]).status();
}

/// Writes (or refreshes) the extension and marks it enabled. Returns true when files changed.
pub fn install() -> bool {
    if !is_gnome() {
        return false;
    }
    let Some(d) = dir() else { return false };
    if let Some(old) = d.parent().map(|p| p.join(OLD_UUID)) {
        let _ = std::fs::remove_dir_all(old);
    }
    let _ = std::fs::create_dir_all(&d);
    let mut changed = false;
    for (name, body) in FILES {
        let p = d.join(name);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(body) {
            changed |= std::fs::write(&p, body).is_ok();
        }
    }
    if let Some(mut en) = gsettings_list("enabled-extensions") {
        let before = en.clone();
        en.retain(|x| x != OLD_UUID);
        if !en.iter().any(|x| x == UUID) {
            en.push(UUID.into());
        }
        if en != before {
            gsettings_set("enabled-extensions", &en);
        }
    }
    if let Some(dis) = gsettings_list("disabled-extensions") {
        if dis.iter().any(|x| x == UUID) {
            let keep: Vec<String> = dis.into_iter().filter(|x| x != UUID).collect();
            gsettings_set("disabled-extensions", &keep);
        }
    }
    changed
}
