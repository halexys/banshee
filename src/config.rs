//! User settings, stored in ~/.config/banshee/config.json.

use crate::hw::{self, Curve, Profile};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub profile: Profile,
    pub kbd: Option<u8>,
    pub charge_limit: u8,
    /// Travel mode: charge to 100 % until this unix time, then go back to `charge_limit`.
    pub travel_until: Option<u64>,
    pub restore_on_start: bool,
    /// The user's fan curve (EC duty units, 0..=150).
    pub curve: Curve,
    /// Profiles that run `curve` instead of MSI's table. Silent never does: its fan byte (0x1D)
    /// is what makes it silent, and advanced mode would replace it.
    pub curve_profiles: Vec<Profile>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            profile: Profile::Balanced,
            kbd: None,
            charge_limit: 80,
            travel_until: None,
            restore_on_start: true,
            curve: hw::FACTORY,
            curve_profiles: Vec::new(),
        }
    }
}

pub const TRAVEL_DAYS: u64 = 7;

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    let dir = base.join("banshee");
    // settings from before the rename (when the app was called ghostdeck)
    let old = base.join("ghostdeck");
    if !dir.exists() && old.join("config.json").exists() {
        let _ = std::fs::rename(&old, &dir);
    }
    dir.join("config.json")
}

impl Config {
    pub fn load() -> Config {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }

    pub fn travel_active(&self) -> bool {
        self.travel_until.map(|t| t > now()).unwrap_or(false)
    }

    /// The limit the EC should hold right now (100 while travelling).
    pub fn effective_limit(&self) -> u8 {
        if self.travel_active() { 100 } else { self.charge_limit }
    }

    pub fn curve_for(&self, p: Profile) -> bool {
        p != Profile::Silent && self.curve_profiles.contains(&p)
    }

    /// Profile `p` was just applied: run our curve or MSI's table, whichever it is assigned.
    pub fn sync_fans(&self, p: Profile) -> Result<(), String> {
        if self.curve_for(p) { hw::engage_curve(&self.curve) } else { hw::release_curve(p) }
    }

    /// Re-asserts profile, charge limit and keyboard level (after boot or resume).
    pub fn apply(&mut self) -> Result<(), String> {
        if self.travel_until.is_some() && !self.travel_active() {
            self.travel_until = None;
            self.save();
        }
        hw::apply_profile(self.profile)?;
        self.sync_fans(self.profile)?;
        hw::set_charge_limit(self.effective_limit())?;
        if let Some(k) = self.kbd {
            hw::set_kbd(k)?;
        }
        Ok(())
    }
}
