//! Hardware access for the MSI Prestige 16 AI Studio B1VFG (EC 15A2EMS1.114).
//!
//! Everything goes through sysfs: the out-of-tree `msi-ec` module (loaded with
//! `debug=1`) for EC registers, power_supply for the battery, powercap (RAPL)
//! for CPU watts and efivarfs for the graphics-mode UEFI variable. Secure Boot
//! lockdown forbids raw port I/O, so there is no other path to the EC.

use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MSI: &str = "/sys/devices/platform/msi-ec";
const RAPL: &str = "/sys/class/powercap/intel-rapl:0/energy_uj";
const DGPU: &str = "/sys/bus/pci/devices/0000:01:00.0";
pub const EFIVAR: &str =
    "/sys/firmware/efi/efivars/MsiDCVarData-dd96baaf-145e-4f56-b1cf-193256298e99";

// EC registers used outside msi-ec's own attributes (verified from Windows, see memory notes).
const EC_KBD: usize = 0xD3; // 0x80 | level 0..3
const EC_RPM_CPU: usize = 0xC9; // rpm = RPM_CONST / raw
const EC_RPM_GPU: usize = 0xCB;
const EC_GPU_ARM: u8 = 0xD1; // low two bits = 01 arms the graphics switch
const RPM_CONST: u32 = 478_000;
const EFI_FLAGS: usize = 4 + 5; // 4 attribute bytes, then data byte 5

pub const NOT_SETUP: &str = "SETUP";

// ---------- small sysfs helpers ----------

fn rd(p: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn rd_num<T: std::str::FromStr>(p: impl AsRef<Path>) -> Option<T> {
    rd(p)?.parse().ok()
}

fn wr(p: impl AsRef<Path>, v: &str) -> Result<(), String> {
    fs::write(p.as_ref(), v).map_err(|e| match e.kind() {
        ErrorKind::PermissionDenied => NOT_SETUP.to_string(),
        _ => format!("{}: {}", p.as_ref().display(), e),
    })
}

fn msi(attr: &str) -> PathBuf {
    Path::new(MSI).join(attr)
}

pub fn battery_dir() -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| rd(p.join("type")).as_deref() == Some("Battery"))
        .collect();
    v.sort();
    v.into_iter().next()
}

/// True when the current user may write the controls (setup done).
pub fn writable() -> bool {
    OpenOptions::new().write(true).open(msi("shift_mode")).is_ok()
        && OpenOptions::new().write(true).open(msi("debug/ec_set")).is_ok()
}

pub fn msi_loaded() -> bool {
    msi("fw_version").exists()
}

pub fn debug_loaded() -> bool {
    msi("debug/ec_dump").exists()
}

// ---------- EC ----------

/// Parses msi-ec's `debug/ec_dump` (world-readable) into 256 bytes.
pub fn ec_dump() -> Option<[u8; 256]> {
    let text = fs::read_to_string(msi("debug/ec_dump")).ok()?;
    let mut ec = [0u8; 256];
    let mut rows = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 3 {
            continue;
        }
        let head = f[1].trim();
        if !head.starts_with("0x") || head.len() < 4 {
            continue;
        }
        let row = match u8::from_str_radix(&head[2..3], 16) {
            Ok(r) => r as usize,
            Err(_) => continue,
        };
        for (i, b) in f[2].split_whitespace().take(16).enumerate() {
            ec[row * 16 + i] = u8::from_str_radix(b, 16).ok()?;
        }
        rows += 1;
    }
    (rows == 16).then_some(ec)
}

pub fn ec_set(addr: u8, val: u8) -> Result<(), String> {
    wr(msi("debug/ec_set"), &format!("{addr:02x}={val:02x}"))
}

fn rpm(raw: u8) -> u32 {
    if raw == 0 {
        0
    } else {
        let r = RPM_CONST / raw as u32;
        if r > 9000 { 0 } else { r }
    }
}

// ---------- profiles ----------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Silent,
    Balanced,
    Extreme,
    Superbatt,
}

impl Profile {
    pub const ALL: [Profile; 4] = [Profile::Silent, Profile::Balanced, Profile::Extreme, Profile::Superbatt];

    /// (shift_mode, fan_mode, super_battery) as msi-ec names them.
    fn recipe(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Profile::Silent => ("comfort", "silent", "off"),
            Profile::Balanced => ("comfort", "auto", "off"),
            Profile::Extreme => ("turbo", "auto", "off"),
            Profile::Superbatt => ("eco", "auto", "on"),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Profile::Silent => "silent",
            Profile::Balanced => "balanced",
            Profile::Extreme => "extreme",
            Profile::Superbatt => "superbatt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Profile::Silent => "Silencioso",
            Profile::Balanced => "Equilibrado",
            Profile::Extreme => "Extremo",
            Profile::Superbatt => "Super Batería",
        }
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            Profile::Silent => [0x3C, 0xC8, 0xB4],
            Profile::Balanced => [0x5A, 0xA9, 0xFF],
            Profile::Extreme => [0xFF, 0x8A, 0x3D],
            Profile::Superbatt => [0x8B, 0xD4, 0x5A],
        }
    }

    pub fn from_id(s: &str) -> Option<Profile> {
        Profile::ALL.into_iter().find(|p| p.id() == s)
    }

    pub fn next(self) -> Profile {
        let i = Profile::ALL.iter().position(|p| *p == self).unwrap_or(0);
        Profile::ALL[(i + 1) % 4]
    }
}

pub fn current_profile() -> Option<Profile> {
    let s = rd(msi("shift_mode"))?;
    // With our own curve running the EC fan byte is 0x8D ("advanced"), which msi-ec does not
    // name: that is still the profile's normal (auto) fan behaviour underneath.
    let f = rd(msi("fan_mode")).filter(|f| f == "auto" || f == "silent").unwrap_or_else(|| "auto".into());
    let b = rd(msi("super_battery"))?;
    Profile::ALL.into_iter().find(|p| {
        let (rs, rf, rb) = p.recipe();
        rs == s && rf == f && rb == b
    })
}

pub fn apply_profile(p: Profile) -> Result<(), String> {
    let (s, f, b) = p.recipe();
    // Super Battery last when turning it on, first when turning it off.
    if b == "off" {
        wr(msi("super_battery"), b)?;
    }
    wr(msi("shift_mode"), s)?;
    wr(msi("fan_mode"), f)?;
    if b == "on" {
        wr(msi("super_battery"), b)?;
    }
    Ok(())
}

// ---------- fan curve ----------
//
// Two step tables in the EC (15A2EMS1, same layout as the rest of MSI's G2 family):
// speed[0] runs below temp[0], speed[i+1] from temp[i] up. Temperatures stay MSI's; only the
// speeds are edited. Speeds are EC duty units: 150 is the real maximum (measured: same rpm as
// Cooler Boost, ~5800/6100 rpm; 200 or 255 spin no faster). The EC only follows the table
// while the fan byte holds 0x8D (advanced); the tables do not change with the shift mode.

const EC_FAN_MODE: u8 = 0xD4;
const FAN_ADVANCED: u8 = 0x8D;
const CPU_T: usize = 0x6A;
const CPU_S: usize = 0x72;
const GPU_T: usize = 0x82;
const GPU_S: usize = 0x8A;
const EC_DUTY_CPU: usize = 0x71;
const EC_DUTY_GPU: usize = 0x89;
pub const FAN_MAX: u8 = 150;
pub const FACTORY: Curve = Curve { cpu: [32, 32, 45, 55, 65, 80, 90], gpu: [0, 32, 40, 45, 60, 70, 80] };

#[derive(Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Curve {
    pub cpu: [u8; 7],
    pub gpu: [u8; 7],
}

impl Curve {
    /// Clamps to 0..=FAN_MAX and makes each table non-decreasing (a hotter step never spins slower).
    pub fn sanitized(mut self) -> Curve {
        for t in [&mut self.cpu, &mut self.gpu] {
            let mut lo = 0;
            for v in t.iter_mut() {
                *v = (*v).clamp(lo, FAN_MAX);
                lo = *v;
            }
        }
        self
    }
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct FanTemps {
    pub cpu: [u8; 6],
    pub gpu: [u8; 6],
}

pub fn fan_temps(ec: &[u8; 256]) -> FanTemps {
    let mut f = FanTemps::default();
    f.cpu.copy_from_slice(&ec[CPU_T..CPU_T + 6]);
    f.gpu.copy_from_slice(&ec[GPU_T..GPU_T + 6]);
    f
}

fn write_tables(c: &Curve) -> Result<(), String> {
    for i in 0..7 {
        ec_set((CPU_S + i) as u8, c.cpu[i])?;
        ec_set((GPU_S + i) as u8, c.gpu[i])?;
    }
    Ok(())
}

/// Loads the curve and hands the fans to it.
pub fn engage_curve(c: &Curve) -> Result<(), String> {
    write_tables(&c.sanitized())?;
    ec_set(EC_FAN_MODE, FAN_ADVANCED)
}

/// Puts MSI's own table back; if our curve was running, the fan byte goes back to `p`'s mode.
pub fn release_curve(p: Profile) -> Result<(), String> {
    let ec = ec_dump().ok_or("EC dump unavailable")?;
    if ec[CPU_S..CPU_S + 7] != FACTORY.cpu || ec[GPU_S..GPU_S + 7] != FACTORY.gpu {
        write_tables(&FACTORY)?;
    }
    if ec[EC_FAN_MODE as usize] == FAN_ADVANCED {
        wr(msi("fan_mode"), p.recipe().1)?;
    }
    Ok(())
}

// ---------- simple controls ----------

pub fn set_cooler_boost(on: bool) -> Result<(), String> {
    wr(msi("cooler_boost"), if on { "on" } else { "off" })
}

pub fn set_fn_side(right: bool) -> Result<(), String> {
    wr(msi("fn_key"), if right { "right" } else { "left" })
}

pub fn set_kbd(level: u8) -> Result<(), String> {
    ec_set(EC_KBD as u8, 0x80 | level.min(3))
}

pub fn set_charge_limit(pct: u8) -> Result<(), String> {
    let b = battery_dir().ok_or("no battery")?;
    wr(b.join("charge_control_end_threshold"), &pct.clamp(20, 100).to_string())
}

// ---------- graphics mode ----------

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Gfx {
    Hybrid,
    Discrete,
    Igpu,
}

fn gfx_from(bits: u8) -> Gfx {
    match bits & 3 {
        1 => Gfx::Discrete,
        2 => Gfx::Igpu,
        _ => Gfx::Hybrid,
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct GfxState {
    pub booted: Gfx,
    pub next: Gfx,
}

pub fn gfx_state() -> Option<GfxState> {
    let d = fs::read(EFIVAR).ok()?;
    let f = *d.get(EFI_FLAGS)?;
    let supported = f & 0x10 != 0 && f & 0x20 != 0;
    supported.then(|| GfxState { booted: gfx_from(f >> 2), next: gfx_from(f) })
}

/// Root only (run through pkexec): stores `mode` for the next boot and arms the EC.
pub fn write_gpu_mode(igpu: bool) -> Result<(), String> {
    let mut d = fs::read(EFIVAR).map_err(|e| format!("read efivar: {e}"))?;
    if d.len() <= EFI_FLAGS {
        return Err("MsiDCVarData too short".into());
    }
    let f = d[EFI_FLAGS];
    if f & 0x10 == 0 || f & 0x20 == 0 {
        return Err("graphics switch not supported by firmware".into());
    }
    let want: u8 = if igpu { 2 } else { 0 };
    let booted = (f >> 2) & 3;
    d[EFI_FLAGS] = (f & !3) | want;

    let _ = Command::new("chattr").args(["-i", EFIVAR]).status();
    let res = fs::write(EFIVAR, &d).map_err(|e| format!("write efivar: {e}"));
    let _ = Command::new("chattr").args(["+i", EFIVAR]).status();
    res?;

    let ec = ec_dump().ok_or("ec_dump unavailable (msi-ec debug not loaded)")?;
    let cur = ec[EC_GPU_ARM as usize];
    let armed = if want == booted { cur & 0xFC } else { (cur & 0xFC) | 0x01 };
    ec_set(EC_GPU_ARM, armed)?;
    std::thread::sleep(Duration::from_millis(300));
    let back = ec_dump().map(|e| e[EC_GPU_ARM as usize]);
    if back != Some(armed) {
        return Err(format!("EC 0xD1 readback {back:02x?}, expected {armed:02x}"));
    }
    Ok(())
}

// ---------- telemetry ----------

#[derive(Serialize, Clone, Debug, Default)]
pub struct Gpu {
    pub awake: bool,
    pub temp: Option<u32>,
    pub watts: Option<f32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Battery {
    pub pct: Option<u32>,
    pub status: String,
    pub watts: Option<f32>,
    pub limit: Option<u32>,
    pub ac: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Status {
    pub msi: bool,
    pub debug: bool,
    pub writable: bool,
    pub fw: Option<String>,
    pub profile: Option<Profile>,
    pub cooler_boost: bool,
    pub fn_right: bool,
    pub kbd: Option<u8>,
    pub cpu_temp: Option<u32>,
    pub cpu_fan_pct: Option<u32>,
    pub rpm_cpu: Option<u32>,
    pub rpm_gpu: Option<u32>,
    pub cpu_watts: Option<f32>,
    pub gpu: Gpu,
    pub battery: Battery,
    pub gfx: Option<GfxState>,
    /// our curve is driving the fans (EC fan byte = advanced)
    pub fan_curve: bool,
    pub duty_cpu: Option<u8>,
    pub duty_gpu: Option<u8>,
    pub fan_temps: Option<FanTemps>,
}

pub struct Sampler {
    rapl_last: Mutex<Option<(u64, Instant)>>,
    gpu_cache: Mutex<Option<(Gpu, Instant)>>,
}

impl Sampler {
    pub fn new() -> Self {
        Sampler { rapl_last: Mutex::new(None), gpu_cache: Mutex::new(None) }
    }

    fn cpu_watts(&self) -> Option<f32> {
        let e: u64 = rd_num(RAPL)?;
        let now = Instant::now();
        let mut last = self.rapl_last.lock().unwrap();
        let w = last.and_then(|(pe, pt)| {
            let dt = now.duration_since(pt).as_secs_f64();
            (e >= pe && dt > 0.2).then(|| ((e - pe) as f64 / 1e6 / dt) as f32)
        });
        *last = Some((e, now));
        w
    }

    /// Queries nvidia-smi only when the RTX is already awake, so polling never wakes it.
    fn gpu(&self) -> Gpu {
        let awake = rd(Path::new(DGPU).join("power/runtime_status")).as_deref() == Some("active");
        if !awake {
            return Gpu::default();
        }
        let mut c = self.gpu_cache.lock().unwrap();
        if let Some((g, t)) = c.as_ref() {
            if t.elapsed() < Duration::from_secs(3) {
                return g.clone();
            }
        }
        let out = Command::new("nvidia-smi")
            .args(["--query-gpu=temperature.gpu,power.draw", "--format=csv,noheader,nounits"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let mut it = out.trim().split(',').map(|s| s.trim());
        let g = Gpu {
            awake: true,
            temp: it.next().and_then(|s| s.parse().ok()),
            watts: it.next().and_then(|s| s.parse().ok()),
        };
        *c = Some((g.clone(), Instant::now()));
        g
    }

    pub fn sample(&self) -> Status {
        let ec = ec_dump();
        let bat = battery_dir();
        let battery = bat
            .as_ref()
            .map(|b| {
                let uw: Option<f64> = rd_num(b.join("power_now")).or_else(|| {
                    let i: f64 = rd_num(b.join("current_now"))?;
                    let v: f64 = rd_num(b.join("voltage_now"))?;
                    Some(i * v / 1e6)
                });
                let ac = fs::read_dir("/sys/class/power_supply")
                    .ok()
                    .map(|d| {
                        d.flatten().any(|e| {
                            rd(e.path().join("type")).as_deref() == Some("Mains")
                                && rd(e.path().join("online")).as_deref() == Some("1")
                        })
                    })
                    .unwrap_or(false);
                Battery {
                    pct: rd_num(b.join("capacity")),
                    status: rd(b.join("status")).unwrap_or_default(),
                    watts: uw.map(|w| (w / 1e6) as f32).filter(|w| *w > 0.05),
                    limit: rd_num(b.join("charge_control_end_threshold")),
                    ac,
                }
            })
            .unwrap_or_default();

        Status {
            msi: msi_loaded(),
            debug: ec.is_some(),
            writable: writable(),
            fw: rd(msi("fw_version")),
            profile: current_profile(),
            cooler_boost: rd(msi("cooler_boost")).as_deref() == Some("on"),
            fn_right: rd(msi("fn_key")).as_deref() == Some("right"),
            kbd: ec.map(|e| (e[EC_KBD] & 0x0F).min(3)),
            cpu_temp: rd_num(msi("cpu/realtime_temperature")),
            cpu_fan_pct: rd_num(msi("cpu/realtime_fan_speed")),
            rpm_cpu: ec.map(|e| rpm(e[EC_RPM_CPU])),
            rpm_gpu: ec.map(|e| rpm(e[EC_RPM_GPU])),
            cpu_watts: self.cpu_watts(),
            gpu: self.gpu(),
            battery,
            gfx: gfx_state(),
            fan_curve: ec.map(|e| e[EC_FAN_MODE as usize] == FAN_ADVANCED).unwrap_or(false),
            duty_cpu: ec.map(|e| e[EC_DUTY_CPU]),
            duty_gpu: ec.map(|e| e[EC_DUTY_GPU]),
            fan_temps: ec.as_ref().map(fan_temps),
        }
    }
}
