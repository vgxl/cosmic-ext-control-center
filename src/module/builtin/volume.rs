//! A REAL built-in: system volume via `wpctl` (PipeWire/WirePlumber, always
//! present on COSMIC). Proves a built-in can drive live system state, not just
//! hold demo values. Reads happen off the UI thread (async refresh); writes are
//! spawned, so neither blocks the popup. A drag issues a write per step, and
//! only the newest is kept in flight (`super::write_superseding`) — left to
//! race, they finish out of order and the sink lands on a level the slider
//! doesn't show.
//!
//! The tile follows COSMIC's own sound settings: the slider's ceiling is 100%
//! until the system's "amplification" switch is on, then 150%. State is polled,
//! so a change made in the system settings — or by any other app — shows up here.

use crate::app::Message;
use crate::module::{ControlValue, InstanceId, Module, ModuleDescriptor, TileSize};
use cosmic::app::Task;
use cosmic::cosmic_config::{Config, ConfigGet};
use cosmic::prelude::*;
use std::process::{Child, Command};
use std::time::Instant;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";

/// COSMIC's sound settings, where the "allow above 100%" switch lives.
const COSMIC_AUDIO_ID: &str = "com.system76.CosmicAudio";
const COSMIC_AUDIO_VERSION: u64 = 1;
const AMPLIFY_KEY: &str = "amplification_sink";

/// Slider ceiling with amplification off / on. COSMIC's sound settings stop at
/// 150% when amplification is enabled, so the tile mirrors that.
const MAX_NORMAL: f32 = 1.0;
const MAX_AMPLIFIED: f32 = 1.5;

/// Speaker icon by mute state and level (Cosmic theme has the full set). Shared
/// by the volume tile and the panel status cluster.
pub fn volume_icon(muted: bool, level: f32) -> &'static str {
    if muted || level <= 0.0 {
        "audio-volume-muted-symbolic"
    } else if level < 0.34 {
        "audio-volume-low-symbolic"
    } else if level < 0.67 {
        "audio-volume-medium-symbolic"
    } else {
        "audio-volume-high-symbolic"
    }
}

pub struct VolumeModule {
    desc: ModuleDescriptor,
    value: f32,
    muted: bool,
    available: bool,
    /// COSMIC's amplification switch, so the slider's ceiling (100% vs 150%)
    /// matches what the system actually allows.
    amplify: bool,
    /// The in-flight `wpctl set-volume`. A faster drag would otherwise leave
    /// several writes racing for the sink, and the last one to *finish* — not
    /// the last one asked for — would win, so the sink ended up on a level the
    /// slider didn't show. Superseding the previous write keeps one writer.
    volume_writer: Option<Child>,
    /// Same for `wpctl set-mute` (kept apart so a mute click and a drag don't
    /// cancel each other).
    mute_writer: Option<Child>,
    /// When the sink was last written, to ignore polls that read it earlier.
    last_write: Option<Instant>,
}

#[derive(Default, Clone, Copy)]
struct VolData {
    value: f32,
    muted: bool,
    available: bool,
    amplify: bool,
}

/// Read COSMIC's amplification switch. Absent key → `false`, the same default
/// the sound settings start from.
fn amplification_enabled() -> bool {
    Config::new(COSMIC_AUDIO_ID, COSMIC_AUDIO_VERSION)
        .ok()
        .and_then(|ctx| ctx.get::<bool>(AMPLIFY_KEY).ok())
        .unwrap_or(false)
}

/// Parse `wpctl get-volume @DEFAULT_AUDIO_SINK@` → "Volume: 0.45 [MUTED]".
fn fetch() -> VolData {
    let mut d = VolData { amplify: amplification_enabled(), ..Default::default() };
    if let Ok(out) = Command::new("wpctl").args(["get-volume", SINK]).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(num) = s.split_whitespace().nth(1) {
                if let Ok(v) = num.parse::<f32>() {
                    d.value = v.clamp(0.0, MAX_AMPLIFIED);
                    d.muted = s.contains("[MUTED]");
                    d.available = true;
                }
            }
        }
    }
    d
}

impl VolumeModule {
    pub fn new() -> Self {
        let mut m = Self {
            desc: ModuleDescriptor {
                id: "builtin.volume".into(),
                name: "Volume".into(),
                icon: "audio-volume-high-symbolic".into(),
                size: TileSize::Full,
                resizable: true,
            },
            value: 0.5,
            muted: false,
            available: false,
            amplify: false,
            volume_writer: None,
            mute_writer: None,
            last_write: None,
        };
        m.set(fetch());
        m
    }

    fn set(&mut self, d: VolData) {
        self.amplify = d.amplify;
        // Keep the default until wpctl actually answers.
        if d.available {
            self.value = d.value;
            self.muted = d.muted;
            self.available = true;
        }
    }

    /// The slider's ceiling: 150% only while COSMIC's amplification is on.
    fn max(&self) -> f32 {
        if self.amplify { MAX_AMPLIFIED } else { MAX_NORMAL }
    }

    fn write(&mut self, v: f32) {
        // `wpctl set-volume` caps at 1.0 (100%) unless `-l/--limit` raises the
        // ceiling, so amplification above 100% needs it on every write.
        let mut cmd = Command::new("wpctl");
        cmd.args([
            "set-volume",
            SINK,
            &format!("{v:.2}"),
            "-l",
            &format!("{:.2}", self.max()),
        ]);
        super::write_superseding(&mut self.volume_writer, cmd);
        self.last_write = Some(Instant::now());
    }

    fn write_mute(&mut self, muted: bool) {
        let mut cmd = Command::new("wpctl");
        cmd.args(["set-mute", SINK, if muted { "1" } else { "0" }]);
        super::write_superseding(&mut self.mute_writer, cmd);
        self.last_write = Some(Instant::now());
    }
}

impl Module for VolumeModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }

    fn view(&self, id: InstanceId, edit: bool, width: f32) -> Element<'_, Message> {
        let max = self.max();
        // Never show a level the current ceiling can't express (an amplified
        // level read back after the switch was turned off).
        let value = self.value.min(max);
        let pct = if !self.available {
            "n/a".to_string()
        } else if self.muted {
            "Muted".to_string()
        } else {
            format!("{:.0}%", value * 100.0)
        };
        let icon = volume_icon(self.muted, value);
        super::slider_tile(
            id,
            width,
            value,
            max,
            icon,
            pct,
            "value",
            Some(("mute", self.muted)),
            edit,
        )
    }

    fn on_control(&mut self, control: &str, value: ControlValue) -> Task<Message> {
        match (control, value) {
            ("value", ControlValue::Float(v)) => {
                self.value = v as f32;
                if self.available {
                    self.write(self.value);
                }
            }
            ("mute", ControlValue::Bool(b)) => {
                self.muted = b;
                if self.available {
                    self.write_mute(b);
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn fetch_job(&self) -> Option<Box<dyn FnOnce() -> crate::module::Payload + Send>> {
        Some(Box::new(|| crate::module::Payload::new(fetch())))
    }

    fn follows_mixer(&self) -> bool {
        true
    }

    fn apply_data(&mut self, data: &dyn std::any::Any) {
        if let Some(d) = data.downcast_ref::<VolData>() {
            if super::poll_is_stale(self.last_write) {
                // A poll that began before our write can still report the old
                // sink state. Our value/mute are the newer intent, so keep them
                // and take only what the write can't affect.
                self.amplify = d.amplify;
                self.available |= d.available;
            } else {
                self.set(*d);
            }
        }
    }
}
