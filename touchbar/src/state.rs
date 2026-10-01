use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::theme::Theme;

const CURRENT_STATE: &str = ".local/state/omarchy/current";
const TOUCH_ID_STATE: &str = "/run/t1bridge/touch-id-state.json";
const BACKLIGHTS: &str = "/sys/class/backlight";
const LEDS: &str = "/sys/class/leds";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub theme: Theme,
    pub workspace: String,
    pub playback: String,
    pub media_available: bool,
    pub volume: u8,
    pub audio_muted: bool,
    pub brightness: u8,
    pub keyboard_backlight: u8,
}

pub struct VisualState {
    pub snapshot: Snapshot,
    width: u32,
    height: u32,
    home: PathBuf,
}

impl VisualState {
    pub fn load(width: u32, height: u32) -> Result<Self> {
        let home = home_dir()?;
        let snapshot = Snapshot::read(&home);
        Ok(Self {
            snapshot,
            width,
            height,
            home,
        })
    }

    pub fn refresh(&mut self) -> bool {
        let next_snapshot = Snapshot::read(&self.home);
        let changed = next_snapshot != self.snapshot;
        self.snapshot = next_snapshot;
        changed
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl Snapshot {
    fn read(home: &Path) -> Self {
        let current = home.join(CURRENT_STATE);
        let (volume, audio_muted) = read_volume();
        let (playback, media_available) = read_playback();
        Self {
            theme: Theme::load(&current),
            workspace: read_workspace().unwrap_or_else(|| "Desktop".into()),
            playback,
            media_available,
            volume,
            audio_muted,
            brightness: read_brightness(),
            keyboard_backlight: read_keyboard_backlight(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TouchIdState {
    pub version: u8,
    pub pid: u32,
    pub state: String,
    pub progress: Option<u8>,
}

impl TouchIdState {
    pub fn label(&self) -> &'static str {
        match self.state.as_str() {
            "enrollment" => "TOUCH ID SETUP · TOUCH, THEN LIFT YOUR FINGER",
            "authenticate" => "TOUCH ID · PLACE YOUR FINGER",
            "approve" => "TOUCH ID · TOUCH TO APPROVE",
            "retry" => "NOT RECOGNIZED · TRY AGAIN",
            "success" => "APPROVED",
            _ => "TOUCH ID",
        }
    }

    pub fn cancellable(&self) -> bool {
        self.state != "success"
    }
}

pub fn read_touch_id_state() -> Option<TouchIdState> {
    let data = fs::read(TOUCH_ID_STATE).ok()?;
    if data.len() > 1024 {
        return None;
    }
    let state: TouchIdState = serde_json::from_slice(&data).ok()?;
    let valid_state = matches!(
        state.state.as_str(),
        "enrollment" | "authenticate" | "approve" | "retry" | "success"
    );
    let valid_version = match state.version {
        1 => state.progress.is_none(),
        2 => state.state == "enrollment" && state.progress.is_some_and(|progress| progress <= 100),
        _ => false,
    };
    if state.pid == 0
        || !valid_state
        || !valid_version
        || !Path::new("/proc").join(state.pid.to_string()).exists()
    {
        return None;
    }
    Some(state)
}

pub fn load_font() -> Result<fontdue::Font> {
    let preferred = Command::new("fc-match")
        .args(["-f", "%{file}\\n", "JetBrainsMono Nerd Font"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|output| output.lines().next().map(PathBuf::from));
    let path = preferred
        .filter(|path| path.is_file())
        .or_else(|| {
            let fallback = PathBuf::from("/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf");
            fallback.is_file().then_some(fallback)
        })
        .context("could not find JetBrainsMono Nerd Font")?;
    let bytes = fs::read(&path).with_context(|| format!("read font {}", path.display()))?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
        .map_err(|error| anyhow::anyhow!("load font {}: {error}", path.display()))
}

pub fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .context("HOME is not an absolute path")
}

fn read_workspace() -> Option<String> {
    // The renderer starts with the user's systemd manager at boot, before
    // Hyprland exists, so it never inherits HYPRLAND_INSTANCE_SIGNATURE.
    // Instance 0 is hyprctl's first running compositor, looked up afresh on
    // every call, which also follows a compositor restart.
    let output = Command::new("hyprctl")
        .args(["--instance", "0", "-j", "activeworkspace"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    value.get("name").and_then(Value::as_str).map(str::to_owned)
}

fn read_volume() -> (u8, bool) {
    let Some(output) = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()
        .filter(|output| output.status.success())
    else {
        return (0, false);
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let volume = text
        .split_whitespace()
        .find_map(|part| part.parse::<f32>().ok())
        .map(|value| (value * 100.0).round().clamp(0.0, 100.0) as u8)
        .unwrap_or(0);
    (volume, text.contains("[MUTED]"))
}

fn read_playback() -> (String, bool) {
    let available = Command::new("playerctl")
        .arg("-l")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| !output.stdout.is_empty());
    if !available {
        return ("PLAY".into(), false);
    }
    let playing = Command::new("playerctl")
        .arg("status")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "Playing");
    (if playing { "PAUSE" } else { "PLAY" }.into(), true)
}

pub fn keyboard_backlight_device() -> Option<String> {
    fs::read_dir(LEDS)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .find(|name| name.contains("kbd_backlight"))
}

fn read_brightness() -> u8 {
    let Ok(entries) = fs::read_dir(BACKLIGHTS) else {
        return 0;
    };
    entries
        .flatten()
        .find_map(|entry| brightness_percentage(&entry.path()))
        .unwrap_or(0)
}

fn read_keyboard_backlight() -> u8 {
    keyboard_backlight_device()
        .and_then(|device| brightness_percentage(&Path::new(LEDS).join(device)))
        .unwrap_or(0)
}

fn brightness_percentage(device: &Path) -> Option<u8> {
    let current = read_number(&device.join("brightness"))?;
    let maximum = read_number(&device.join("max_brightness")).filter(|maximum| *maximum > 0)?;
    Some(((current * 100 + maximum / 2) / maximum).min(100) as u8)
}

fn read_number(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_impossible_touch_id_records() {
        let state: TouchIdState =
            serde_json::from_str(r#"{"version":2,"pid":1,"state":"enrollment","progress":101}"#)
                .unwrap();
        assert!(state.progress.is_some_and(|value| value > 100));
    }

    #[test]
    fn brightness_percentage_rounds_to_the_nearest_step() {
        let device = env::temp_dir().join(format!("leapfrog-touchbar-{}", std::process::id()));
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("brightness"), "26\n").unwrap();
        fs::write(device.join("max_brightness"), "255\n").unwrap();
        assert_eq!(brightness_percentage(&device), Some(10));
        fs::write(device.join("max_brightness"), "0\n").unwrap();
        assert_eq!(brightness_percentage(&device), None);
        fs::remove_dir_all(&device).unwrap();
    }
}
