//! Microphone: input level slider + mute toggle, via `wpctl` on the default
//! audio source. Like the volume tile, a drag issues a write per step, so the
//! writes are coalesced to the newest one (`super::write_superseding`) and a
//! poll that read the source before our write is ignored for a moment.

use crate::app::Message;
use crate::module::{ControlValue, InstanceId, Module, ModuleDescriptor, TileSize};
use cosmic::app::Task;
use cosmic::prelude::*;
use std::process::{Child, Command};
use std::time::Instant;

const SRC: &str = "@DEFAULT_AUDIO_SOURCE@";

pub struct MicrophoneModule {
    desc: ModuleDescriptor,
    level: f32,
    muted: bool,
    available: bool,
    /// In-flight writes, one slot per control: a newer write supersedes the
    /// older one so the source can't end up on a level the slider doesn't show.
    level_writer: Option<Child>,
    mute_writer: Option<Child>,
    /// When the source was last written, to ignore polls that read it earlier.
    last_write: Option<Instant>,
}

#[derive(Default)]
struct MicData {
    level: f32,
    muted: bool,
    available: bool,
}

/// Read the default source level off the UI thread. "Volume: 0.40 [MUTED]".
fn fetch() -> MicData {
    let mut d = MicData::default();
    if let Some(o) = super::out(&format!("wpctl get-volume {SRC}")) {
        if let Some(num) = o.split_whitespace().nth(1) {
            if let Ok(v) = num.parse::<f32>() {
                d.level = v.clamp(0.0, 1.5);
                d.muted = o.contains("[MUTED]");
                d.available = true;
            }
        }
    }
    d
}

impl MicrophoneModule {
    pub fn new() -> Self {
        let mut m = Self {
            desc: ModuleDescriptor {
                id: "builtin.microphone".into(),
                name: "Microphone".into(),
                icon: "audio-input-microphone-symbolic".into(),
                size: TileSize::Full,
                resizable: true,
            },
            level: 0.5,
            muted: false,
            available: false,
            level_writer: None,
            mute_writer: None,
            last_write: None,
        };
        m.set(fetch());
        m
    }

    fn set(&mut self, d: MicData) {
        if d.available {
            self.level = d.level;
            self.muted = d.muted;
            self.available = true;
        }
    }

    fn write_level(&mut self, level: f32) {
        let mut cmd = Command::new("wpctl");
        cmd.args(["set-volume", SRC, &format!("{level:.2}")]);
        super::write_superseding(&mut self.level_writer, cmd);
        self.last_write = Some(Instant::now());
    }

    fn write_mute(&mut self, muted: bool) {
        let mut cmd = Command::new("wpctl");
        cmd.args(["set-mute", SRC, if muted { "1" } else { "0" }]);
        super::write_superseding(&mut self.mute_writer, cmd);
        self.last_write = Some(Instant::now());
    }
}

impl Module for MicrophoneModule {
    fn descriptor(&self) -> &ModuleDescriptor {
        &self.desc
    }

    fn view(&self, id: InstanceId, edit: bool, width: f32) -> Element<'_, Message> {
        let pct = if self.available {
            format!("{:.0}%", self.level * 100.0)
        } else {
            "n/a".to_string()
        };
        let icon = if self.muted {
            "microphone-sensitivity-muted-symbolic"
        } else {
            "audio-input-microphone-symbolic"
        };
        super::slider_tile(
            id,
            width,
            self.level.min(1.0),
            1.0,
            icon,
            pct,
            "level",
            Some(("mute", self.muted)),
            edit,
        )
    }

    fn on_control(&mut self, control: &str, value: ControlValue) -> Task<Message> {
        match (control, value) {
            ("level", ControlValue::Float(v)) => {
                self.level = v as f32;
                if self.available {
                    self.write_level(self.level);
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
        if let Some(d) = data.downcast_ref::<MicData>() {
            if super::poll_is_stale(self.last_write) {
                // A poll that began before our write can still report the old
                // source state. Our level/mute are the newer intent, so keep
                // them and take only what the write can't affect.
                self.available |= d.available;
            } else {
                self.set(MicData {
                    level: d.level,
                    muted: d.muted,
                    available: d.available,
                });
            }
        }
    }
}
