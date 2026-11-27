//! One-time system setup, run as root through pkexec (`banshee --setup`).
//!
//! - loads msi-ec with debug=1 on every boot (EC dump / ec_set attributes)
//! - a udev rule hands the control attributes to the invoking user, so the app
//!   never runs as root and needs no password for everyday changes
//! - applies the same ownership right now, so no re-login is needed

use crate::hw;
use std::fs;
use std::path::Path;
use std::process::Command;

const MODPROBE: &str = "/etc/modprobe.d/banshee.conf";
const RULES: &str = "/etc/udev/rules.d/90-banshee.rules";
const MSI_ATTRS: &str = "shift_mode fan_mode super_battery cooler_boost fn_key win_key debug/ec_set debug/ec_get";

fn rules(uid: u32) -> String {
    format!(
        "# Written by Banshee (banshee --setup). Gives uid {uid} write access to the\n\
         # msi-ec controls, the battery charge limit and the RAPL energy counter.\n\
         ACTION==\"bind\", SUBSYSTEM==\"platform\", DRIVER==\"msi-ec\", RUN+=\"/bin/sh -c 'cd /sys%p && chown {uid} {MSI_ATTRS}; chmod u+w debug/ec_set; true'\"\n\
         ACTION==\"add|change\", SUBSYSTEM==\"power_supply\", KERNEL==\"BAT*\", RUN+=\"/bin/sh -c 'chown {uid} /sys%p/charge_control_end_threshold /sys%p/charge_control_start_threshold 2>/dev/null; true'\"\n\
         ACTION==\"add\", SUBSYSTEM==\"powercap\", KERNEL==\"intel-rapl:0\", RUN+=\"/bin/sh -c 'chown {uid} /sys%p/energy_uj; true'\"\n"
    )
}

fn chown_now(uid: u32) {
    let mut paths: Vec<String> = MSI_ATTRS.split(' ').map(|a| format!("{}/{}", hw::MSI, a)).collect();
    if let Some(b) = hw::battery_dir() {
        for a in ["charge_control_end_threshold", "charge_control_start_threshold"] {
            paths.push(b.join(a).display().to_string());
        }
    }
    paths.push("/sys/class/powercap/intel-rapl:0/energy_uj".into());
    let _ = Command::new("chown").arg(uid.to_string()).args(&paths).status();
    let _ = Command::new("chmod").args(["u+w", &format!("{}/debug/ec_set", hw::MSI)]).status();
}

pub fn run() -> Result<(), String> {
    let uid: u32 = std::env::var("PKEXEC_UID")
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or("run through pkexec (PKEXEC_UID missing)")?;
    if uid == 0 {
        return Err("refusing to set up for root".into());
    }

    // files from before the rename (when the app was called ghostdeck)
    for old in ["/etc/modprobe.d/ghostdeck.conf", "/etc/udev/rules.d/90-ghostdeck.rules"] {
        let _ = fs::remove_file(old);
    }
    fs::write(MODPROBE, "# Written by Banshee: EC dump / ec_set attributes\noptions msi_ec debug=1\n")
        .map_err(|e| format!("{MODPROBE}: {e}"))?;
    fs::write(RULES, rules(uid)).map_err(|e| format!("{RULES}: {e}"))?;
    for f in [MODPROBE, RULES] {
        let _ = fs::set_permissions(f, std::os::unix::fs::PermissionsExt::from_mode(0o644));
    }
    let _ = Command::new("udevadm").args(["control", "--reload"]).status();

    // Reload msi-ec so debug=1 takes effect now (the bind event also runs the rule).
    if !hw::debug_loaded() {
        let _ = Command::new("modprobe").args(["-r", "msi_ec"]).status();
        let ok = Command::new("modprobe").arg("msi_ec").status().map(|s| s.success()).unwrap_or(false);
        if !ok {
            return Err("modprobe msi_ec failed (is the DKMS module installed and the MOK enrolled?)".into());
        }
        let _ = Command::new("udevadm").arg("settle").status();
    }
    chown_now(uid);

    if !Path::new(hw::MSI).join("debug/ec_dump").exists() {
        return Err("msi-ec loaded without debug attributes".into());
    }
    Ok(())
}
