//! Built-in (Rust) modules + their registry. Each implements `Module`
//! directly, so they get full libcosmic power. The registry exposes their
//! descriptors (for the "add" palette) and a factory to instantiate by id.

mod airplane;
mod appearance;
pub(crate) mod bluetooth;
mod divider;
mod dnd;
mod media;
mod microphone;
mod mpris;
mod notifications;
pub(crate) mod power_profile;
mod sysmon;
pub(crate) mod volume;
pub(crate) mod vpn;
pub(crate) mod wifi;

use crate::app::Message;
use crate::module::{ControlValue, InstanceId, Module, ModuleDescriptor, cols_for_width};
use crate::theme;
use cosmic::iced::{Alignment, Length};
use cosmic::prelude::*;
use cosmic::widget;
use std::process::{Child, Command};
use std::time::{Duration, Instant};
pub use airplane::AirplaneModule;
pub use appearance::AppearanceModule;
pub use bluetooth::BluetoothModule;
pub use divider::DividerModule;
pub use dnd::DndModule;
pub use media::MediaModule;
pub use microphone::MicrophoneModule;
pub use notifications::NotificationsModule;
pub use power_profile::PowerProfileModule;
pub use sysmon::{CpuModule, DiskModule, GpuModule, RamModule, SysMonModule};
pub use volume::VolumeModule;
pub use vpn::VpnModule;
pub use wifi::WifiModule;

/// Descriptors for every built-in, for the add-module palette.
pub fn descriptors() -> Vec<ModuleDescriptor> {
    vec![
        VolumeModule::new().descriptor().clone(),
        MicrophoneModule::new().descriptor().clone(),
        MediaModule::new().descriptor().clone(),
        PowerProfileModule::new().descriptor().clone(),
        WifiModule::new().descriptor().clone(),
        VpnModule::new().descriptor().clone(),
        BluetoothModule::new().descriptor().clone(),
        AirplaneModule::new().descriptor().clone(),
        AppearanceModule::new().descriptor().clone(),
        DndModule::new().descriptor().clone(),
        NotificationsModule::new().descriptor().clone(),
        DividerModule::new().descriptor().clone(),
        CpuModule::new().descriptor().clone(),
        GpuModule::new().descriptor().clone(),
        RamModule::new().descriptor().clone(),
        DiskModule::new().descriptor().clone(),
        SysMonModule::new().descriptor().clone(),
    ]
}

/// Whether more than one of this module type may be placed. Most are
/// single-instance (one Wi-Fi toggle is plenty); disk gauges (per mount) and
/// dividers (per section) are the exceptions. Unknown ids (plugins) are single.
pub fn allows_multiple(id: &str) -> bool {
    matches!(id, "builtin.disk" | "builtin.divider")
}

/// Palette category for grouping the "Add a module" list. Unknown ids (plugins)
/// fall under "Plugins". `CATEGORIES` is the display order.
pub const CATEGORIES: [&str; 6] =
    ["Audio", "Network", "Monitors", "System", "Layout", "Plugins"];

pub fn category(id: &str) -> &'static str {
    match id {
        "builtin.volume" | "builtin.microphone" | "builtin.media" | "builtin.media_art" => "Audio",
        "builtin.wifi" | "builtin.vpn" | "builtin.bluetooth" | "builtin.airplane" => "Network",
        "builtin.cpu" | "builtin.gpu" | "builtin.ram" | "builtin.disk" | "builtin.sysmon" => {
            "Monitors"
        }
        "builtin.power_profile"
        | "builtin.appearance"
        | "builtin.notifications"
        | "builtin.do_not_disturb" => "System",
        "builtin.divider" => "Layout",
        _ => "Plugins",
    }
}

/// Instantiate a built-in by its module id, seeding any saved per-instance
/// params (e.g. a disk gauge's mount). Returns None for unknown ids.
pub fn make(id: &str, params: &std::collections::BTreeMap<String, String>) -> Option<Box<dyn Module>> {
    match id {
        "builtin.volume" => Some(Box::new(VolumeModule::new())),
        "builtin.microphone" => Some(Box::new(MicrophoneModule::new())),
        "builtin.media" => Some(Box::new(MediaModule::from_params(params))),
        // Legacy id from when the album-art look was a separate module; maps to
        // a media tile with style=framed and re-saves as builtin.media.
        "builtin.media_art" => Some(Box::new(MediaModule::framed_style())),
        "builtin.power_profile" => Some(Box::new(PowerProfileModule::new())),
        "builtin.wifi" => Some(Box::new(WifiModule::new())),
        "builtin.vpn" => Some(Box::new(VpnModule::new())),
        "builtin.bluetooth" => Some(Box::new(BluetoothModule::new())),
        "builtin.airplane" => Some(Box::new(AirplaneModule::new())),
        "builtin.appearance" => Some(Box::new(AppearanceModule::new())),
        "builtin.do_not_disturb" => Some(Box::new(DndModule::new())),
        "builtin.notifications" => Some(Box::new(NotificationsModule::new())),
        "builtin.divider" => Some(Box::new(DividerModule::new())),
        "builtin.cpu" => Some(Box::new(CpuModule::new())),
        "builtin.gpu" => Some(Box::new(GpuModule::new())),
        "builtin.ram" => Some(Box::new(RamModule::new())),
        "builtin.disk" => Some(Box::new(DiskModule::with_mount(
            params.get("mount").cloned().unwrap_or_else(|| "/".to_string()),
        ))),
        "builtin.sysmon" => Some(Box::new(SysMonModule::new())),
        _ => None,
    }
}

/// Shared tile chrome: a sized card, tinted with the accent when `active`.
pub(crate) fn tile<'a>(
    width: f32,
    active: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let accent = theme::accent(); // cerise
    widget::container(content)
        .padding(14)
        .width(Length::Fixed(width))
        .class(theme::card(active, accent))
        .into()
}

/// What the split pill's right-hand chevron does.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Chevron {
    /// No chevron (toggle only).
    None,
    /// Opens the module's external settings (sends the "settings" control).
    Settings,
    /// Expands an inline selection list (Wi-Fi networks, devices, profiles).
    Expand,
}

/// A full-region interactive overlay for a tile segment: transparent until
/// hovered (so the card shows through), then a subtle highlight, with the outer
/// corners (`round` = [top-left, top-right, bottom-right, bottom-left]) matched
/// to the card radius so the highlight sits flush. As a button it also gives the
/// pointer cursor — fixing the old `mouse_area` (no feedback) and the circular
/// icon-button hover (#28).
fn segment_class(round: [bool; 4]) -> cosmic::theme::Button {
    let style = move |bg: f32| {
        move |_focused: bool, t: &cosmic::Theme| {
            let c = t.cosmic();
            let fg: cosmic::iced::Color = c.background.on.into();
            let r = c.corner_radii.radius_m[0];
            let mut s = cosmic::widget::button::Style::new();
            if bg > 0.0 {
                s.background = Some(cosmic::iced::Background::Color(theme::alpha(fg, bg)));
            }
            s.border_radius = [
                if round[0] { r } else { 0.0 },
                if round[1] { r } else { 0.0 },
                if round[2] { r } else { 0.0 },
                if round[3] { r } else { 0.0 },
            ]
            .into();
            s.icon_color = Some(fg);
            s.text_color = Some(fg);
            s
        }
    };
    cosmic::theme::Button::Custom {
        active: Box::new(style(0.0)),
        disabled: Box::new(move |t| style(0.0)(false, t)),
        hovered: Box::new(style(0.08)),
        pressed: Box::new(style(0.14)),
    }
}

/// GNOME-style quick-toggle pill: the whole left area toggles the module; an
/// optional smaller right segment opens its settings or an inline list. The tile
/// is tinted with the accent when on. Controls go inert in edit mode (so the
/// reorder drag isn't swallowed). Modules handle the "on" / "settings" controls.
pub(crate) fn toggle_tile<'a>(
    id: InstanceId,
    width: f32,
    on: bool,
    edit: bool,
    icon: &str,
    label: &str,
    status: &str,
    chevron: Chevron,
) -> Element<'a, Message> {
    let accent = theme::accent();
    // A definite tile height. The full-height divider uses `Length::Fill`, and
    // an *unbounded* Fill collapses the tile's measured height inside flex_row
    // (which made the next row draw on top of it). A fixed card height bounds
    // the Fill so measurement — and the layout — stay correct.
    const H: f32 = 64.0;

    // 1col: there's no room for the label/status/chevron pill, so collapse to a
    // centered icon that toggles on tap (still tinted when on). Settings aren't
    // reachable here — that's the 2col+ layout.
    if cols_for_width(width) <= 1 {
        let mut btn = widget::button::custom(
            widget::container(widget::icon::from_name(icon).size(26))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .class(segment_class([true, true, true, true]))
        .padding(0)
        .width(Length::Fill)
        .height(Length::Fill);
        if !edit {
            btn = btn.on_press(Message::Control(id, "on".into(), ControlValue::Bool(!on)));
        }
        return widget::container(btn)
            .width(Length::Fixed(width))
            .height(Length::Fixed(H))
            .class(theme::card(on, accent))
            .into();
    }

    let info = widget::Row::new()
        .spacing(10)
        .align_y(Alignment::Center)
        .push(widget::icon::from_name(icon).size(22))
        .push(
            widget::Column::new()
                .spacing(1)
                .push(widget::text::body(label.to_string()))
                .push(widget::text::caption(status.to_string())),
        );
    // Round the left corners to match the card; keep the inner edge (against the
    // chevron) square. With no chevron the toggle area is the whole tile.
    let left_round = if chevron != Chevron::None {
        [true, false, false, true]
    } else {
        [true, true, true, true]
    };
    let mut left = widget::button::custom(
        widget::container(info).center_y(Length::Fill).padding([0, 14]),
    )
    .class(segment_class(left_round))
    .padding(0)
    .width(Length::Fill)
    .height(Length::Fill);
    if !edit {
        left = left.on_press(Message::Control(id, "on".into(), ControlValue::Bool(!on)));
    }

    let mut row = widget::Row::new()
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .push(left);
    if chevron != Chevron::None {
        // A full-height divider in the window background colour separates the
        // toggle area from the chevron — reads as a gap between the two halves of
        // the split pill.
        row = row.push(
            widget::container(widget::Space::new())
                .width(Length::Fixed(2.0))
                .height(Length::Fill)
                .class(theme::divider_gap()),
        );
        // Expand uses a down chevron (reveals the list); Settings keeps the
        // forward chevron (leaves to an external page).
        let glyph = if chevron == Chevron::Expand {
            "pan-down-symbolic"
        } else {
            "go-next-symbolic"
        };
        // A full-height segment button (rounded right corners) so the hover
        // covers the whole chevron area, not a circle.
        let mut chev = widget::button::custom(
            widget::container(widget::icon::from_name(glyph).size(16))
                .center_y(Length::Fill)
                .padding([0, 16]),
        )
        .class(segment_class([false, true, true, false]))
        .padding(0)
        .height(Length::Fill);
        if !edit {
            let msg = match chevron {
                Chevron::Expand => Message::Expand(id),
                _ => Message::Control(id, "settings".into(), ControlValue::Trigger),
            };
            chev = chev.on_press(msg);
        }
        row = row.push(chev);
    }

    widget::container(row)
        .width(Length::Fixed(width))
        .height(Length::Fixed(H))
        .class(theme::card(on, accent))
        .into()
}

/// The leading icon for a level control. When `mute = Some((control, muted))`
/// the icon is a button that toggles mute (the caller passes a glyph already
/// reflecting the muted state); otherwise it's a static icon.
fn level_icon<'a>(
    id: InstanceId,
    icon: &str,
    mute: Option<(&'static str, bool)>,
    edit: bool,
    size: u16,
) -> Element<'a, Message> {
    match (mute, edit) {
        (Some((ctrl, muted)), false) => {
            // `button::icon` ignores the handle size (renders its fixed 16px), so
            // use `button::custom` with the Icon style to honor `size`.
            widget::button::custom(widget::icon::from_name(icon).size(size))
                .class(cosmic::theme::Button::Icon)
                .padding(0)
                .on_press(Message::Control(id, ctrl.into(), ControlValue::Bool(!muted)))
                .into()
        }
        _ => widget::container(widget::icon::from_name(icon).size(size)).into(),
    }
}

/// The 100% tick drawn on a slider that can amplify past it.
const BOOST_MARK: [f32; 1] = [1.0];

/// The slider's percentage readout. Tinted with the accent once the level is
/// boosted past 100%, so amplification reads as deliberate, not as a cap.
fn pct_text<'a>(pct: String, boosted: bool) -> Element<'a, Message> {
    let t = widget::text::caption(pct);
    // `class` (not `color`) because this libcosmic's Text needs a style class
    // for the cosmic theme. Both arms stay a `Text`.
    let t = if boosted {
        t.class(cosmic::style::Text::Custom(theme::accent_text))
    } else {
        t
    };
    t.into()
}

/// A level control (volume / mic / brightness): 1col = compact icon + %, 2col+
/// = a single inline row of icon + slider + %. The icon doubles as a mute
/// toggle when `mute` is set. Inert in edit mode. Intentionally has **no card
/// background** — it sits directly on the hub surface.
///
/// `max` is the slider's full scale: 1.0 for an ordinary level, 1.5 for the
/// volume tile, which adapts COSMIC's over-amplification (up to 150%) and marks
/// the 100% breakpoint. `value` is expected to lie within `0.0..=max`.
pub(crate) fn slider_tile<'a>(
    id: InstanceId,
    width: f32,
    value: f32,
    max: f32,
    icon: &str,
    pct: String,
    value_control: &'static str,
    mute: Option<(&'static str, bool)>,
    edit: bool,
) -> Element<'a, Message> {
    let cols = cols_for_width(width);
    // Past 100% the reading is an amplifier warning, so it wears the accent.
    let boosted = max > 1.0 && value > 1.0;

    // Bare wrapper: keeps the grid width + horizontal padding, but no card and
    // only a little vertical padding (these are bgless rows — full card padding
    // made stacked sliders feel far apart).
    let bare = |content: Element<'a, Message>| -> Element<'a, Message> {
        widget::container(content)
            .padding([4, 14])
            .width(Length::Fixed(width))
            .into()
    };

    // Compact (1col): just the icon (mute toggle) and the value.
    if cols <= 1 {
        return bare(
            widget::Column::new()
                .spacing(4)
                .align_x(Alignment::Center)
                .push(level_icon(id, icon, mute, edit, 22))
                .push(pct_text(pct, boosted))
                .into(),
        );
    }

    let on_change = move |v: f32| {
        Message::Control(id, value_control.into(), ControlValue::Float(v as f64))
    };

    // 2col+: a single inline row — icon, slider (fills), then the percentage.
    let s: Element<'a, Message> = if edit {
        // Width/girth must go on the Linear itself; it defaults to a fixed 100px
        // width + thin girth, and a container wrapper does NOT stretch it.
        // The edit-mode bar shares the slider's full scale, so a boosted level
        // fills it the same way the applet's handle sits on it.
        widget::progress_bar::determinate_linear(value / max)
            .width(Length::Fill)
            .girth(Length::Fixed(6.0))
            .into()
    } else {
        let slider = widget::slider(0.0..=max, value, on_change)
            .step(0.01_f32)
            .width(Length::Fill);
        if max > 1.0 {
            slider.breakpoints(&BOOST_MARK).into()
        } else {
            slider.into()
        }
    };
    bare(
        widget::Row::new()
            .spacing(12)
            .align_y(Alignment::Center)
            .push(level_icon(id, icon, mute, edit, 24))
            .push(s)
            .push(pct_text(pct, boosted))
            .into(),
    )
}

// --- preview mode ---------------------------------------------------------
// The editor is a layout surface — it doesn't need live data, just the form of
// each tile. In preview mode every external query/action is a no-op, so modules
// render their default state with zero subprocess/D-Bus/image work (the editor
// was painfully slow doing the real fetches on build + every poll).

use std::sync::atomic::{AtomicBool, Ordering};

static PREVIEW: AtomicBool = AtomicBool::new(false);

/// Enable preview (data-free) mode for this process. The editor sets this; the
/// applet leaves it off so it shows live data.
pub fn set_preview(on: bool) {
    PREVIEW.store(on, Ordering::Relaxed);
}

/// Whether this process is a data-free preview (the editor).
pub fn preview() -> bool {
    PREVIEW.load(Ordering::Relaxed)
}

// --- shared command helpers (synchronous; commands here are fast) ---

/// Run a command, return trimmed stdout on success. No-op in preview mode.
pub(crate) fn out(cmd: &str) -> Option<String> {
    if preview() {
        return None;
    }
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Fire a command, ignoring output (fire-and-forget). No-op in preview mode.
pub(crate) fn run(cmd: &str) {
    if preview() {
        return;
    }
    let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
}

// --- audio write coalescing ----------------------------------------------
// A slider issues a write per step while it is dragged. Left as plain
// fire-and-forget, those processes race and the *last one to finish* — not the
// last one asked for — decides the device, so the sink/source ends up on a
// level the slider doesn't show. The two helpers below keep exactly one write
// in flight and stop a poll from undoing it.

/// How long after a write a polled reading may still predate it. `wpctl` only
/// takes a few milliseconds, but a poll already in flight can have read the
/// device *before* the write landed; without this the stale numbers snap the
/// slider back under the user's finger.
pub(crate) const WRITE_GRACE: Duration = Duration::from_millis(500);

/// Whether a polled reading should be ignored because we wrote the device after
/// that poll's read began.
pub(crate) fn poll_is_stale(last_write: Option<Instant>) -> bool {
    last_write.is_some_and(|t| t.elapsed() < WRITE_GRACE)
}

/// Spawn a fire-and-forget write into `slot`, superseding the previous write for
/// the same control while it is still in flight. A writer that already finished
/// is left alone: it wrote first and the new one writes after, so the order
/// stays right. A live one is killed and reaped off-thread, so the UI thread
/// never waits on it. Callers keep separate slots per control, so a mute click
/// and a drag can't cancel each other.
pub(crate) fn write_superseding(slot: &mut Option<Child>, mut cmd: Command) {
    if preview() {
        return;
    }
    if let Some(mut prev) = slot.take() {
        if prev.try_wait().ok().flatten().is_none() {
            let _ = prev.kill();
            std::thread::spawn(move || {
                let _ = prev.wait();
            });
        }
    }
    if let Ok(child) = cmd.spawn() {
        *slot = Some(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recent_write_makes_the_poll_stale() {
        assert!(poll_is_stale(Some(Instant::now())));
        assert!(!poll_is_stale(None));
        assert!(!poll_is_stale(Some(
            Instant::now() - WRITE_GRACE - Duration::from_millis(1)
        )));
    }
}

use crate::module::Payload;

/// A boxed, `Send` closure producing a module's data payload off the UI thread.
pub(crate) type FetchJob = Box<dyn FnOnce() -> Payload + Send>;

/// Run one module's `job` on tokio's blocking pool and deliver it via
/// `Message::StateLoaded` (on-demand refresh: drawer open / manual rescan).
pub(crate) fn single_fetch(id: InstanceId, job: FetchJob) -> cosmic::app::Task<Message> {
    cosmic::task::future(async move {
        let payload = tokio::task::spawn_blocking(job)
            .await
            .unwrap_or_else(|_| Payload::new(()));
        cosmic::action::app(Message::StateLoaded(id, payload))
    })
}

/// Run every module's `job` on the blocking pool in parallel, then deliver all
/// results in ONE `Message::StateBatch` — so a poll repaints the popup once,
/// not once per module (the source of the per-poll input hitching).
pub(crate) fn poll_batch(jobs: Vec<(InstanceId, FetchJob)>) -> cosmic::app::Task<Message> {
    cosmic::task::future(async move {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|(id, job)| tokio::task::spawn_blocking(move || (id, job())))
            .collect();
        let mut results = Vec::with_capacity(handles.len());
        for h in handles {
            if let Ok(r) = h.await {
                results.push(r);
            }
        }
        cosmic::action::app(Message::StateBatch(results))
    })
}
