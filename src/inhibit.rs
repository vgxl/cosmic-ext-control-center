//! "Prevent sleep": hold *both* locks for as long as the toggle is on.
//!
//! Two different things can put the machine to sleep, and neither sees the
//! other's inhibitor:
//!
//! - **Screen blanking** (DPMS off) on COSMIC is done by cosmic-idle on its
//!   own idle timers — it fades the screen out and powers the output down,
//!   and it neither consults logind inhibitors nor relies on cosmic-comp's
//!   idle state alone. It honors the `org.freedesktop.ScreenSaver` D-Bus
//!   inhibit (the channel mpv and most players use) and Wayland idle
//!   inhibitors surfaced through cosmic-comp
//!   (`zwp_idle_inhibit_unstable_v1` on a live Wayland connection with a
//!   mapped surface). We take both, plus the logind lock for other setups.
//!
//! - **System suspend** on COSMIC is also driven by cosmic-idle (it runs
//!   `systemctl suspend` from its own timer), so the screensaver cookie
//!   blocks it too; the logind inhibitor covers setups where logind's idle
//!   action is in charge instead.
//!
//! Holding both values (and dropping them, or the whole process exiting) is
//! the entire lifetime management — a crashed applet can't leave the machine
//! pinned awake.

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::idle_inhibit::zv1::client::{zwp_idle_inhibit_manager_v1, zwp_idle_inhibitor_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use zbus::blocking::Connection as DbusConnection;
use zbus::zvariant::OwnedFd;

const DEST: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const IFACE: &str = "org.freedesktop.login1.Manager";

const SS_DEST: &str = "org.freedesktop.ScreenSaver";
const SS_PATH: &str = "/org/freedesktop/ScreenSaver";

/// An held pair of inhibitors. Keep the value alive: logind releases its lock
/// when the client disconnects, and the compositor releases the idle inhibit
/// when our Wayland connection closes.
pub struct PreventSleep {
    _logind: Option<LogindInhibitor>,
    _wayland: Option<WaylandInhibitor>,
    _screensaver: Option<ScreensaverInhibitor>,
}

/// Ask logind for an `idle` inhibitor in `block` mode — the machine will not
/// suspend while the returned value lives. `None` when logind is unreachable.
fn logind_acquire() -> Option<LogindInhibitor> {
    let conn = DbusConnection::system().ok()?;
    let reply = conn
        .call_method(
            Some(DEST),
            PATH,
            Some(IFACE),
            "Inhibit",
            &("idle", "Control Center", "Prevent sleep is on", "block"),
        )
        .ok()?;
    let fd = reply.body().deserialize::<OwnedFd>().ok()?;
    Some(LogindInhibitor { _conn: conn, _fd: fd })
}

struct LogindInhibitor {
    _conn: DbusConnection,
    _fd: OwnedFd,
}

/// An `org.freedesktop.ScreenSaver` inhibitor cookie. This is the channel
/// cosmic-idle actually listens on: while any cookie is held it disables BOTH
/// of its timers — screen-off (fade + DPMS off) and auto-suspend. A logind
/// inhibitor is *not* consulted by cosmic-idle, so without this cookie (or a
/// Wayland idle-inhibitor) the screen still blanks on COSMIC even with logind
/// held. mpv and most players inhibit through this same interface.
struct ScreensaverInhibitor {
    _conn: DbusConnection,
    cookie: u32,
}

impl Drop for ScreensaverInhibitor {
    fn drop(&mut self) {
        // Best effort: cosmic-idle also drops the cookie when this connection
        // goes away, but un-inhibit explicitly so the lock is gone immediately.
        let _ = self._conn.call_method(
            Some(SS_DEST),
            SS_PATH,
            Some(SS_DEST),
            "UnInhibit",
            &(self.cookie,),
        );
    }
}

fn screensaver_acquire() -> Option<ScreensaverInhibitor> {
    let conn = DbusConnection::session().ok()?;
    let reply = conn
        .call_method(
            Some(SS_DEST),
            SS_PATH,
            Some(SS_DEST),
            "Inhibit",
            &("Cosmic Ext Control Center", "Prevent sleep is on"),
        )
        .ok()?;
    let cookie = reply.body().deserialize::<u32>().ok()?;
    Some(ScreensaverInhibitor { _conn: conn, cookie })
}

/// A mapped 1×1 surface with an idle inhibitor attached: while this
/// connection lives, the compositor will not blank the display.
fn wayland_acquire() -> Option<WaylandInhibitor> {
    use std::os::fd::AsFd;
    let conn = Connection::connect_to_env().ok()?;
    let mut queue = conn.new_event_queue::<Globals>();
    let qh = queue.handle();

    let mut globals = Globals::default();
    conn.display().get_registry(&qh, ());
    // Two roundtrips: first collects globals, second settles the binds.
    queue.roundtrip(&mut globals).ok()?;
    queue.roundtrip(&mut globals).ok()?;

    let compositor = globals.compositor.take()?;
    let manager = globals.manager.take()?;
    let shm = globals.shm.take()?;
    let layer_shell = globals.layer_shell.take()?;

    let surface = compositor.create_surface(&qh, ());
    // Give the surface a role: an idle inhibitor is only honored while its
    // surface is *mapped*, and a bare wl_surface never is. A background
    // layer surface maps invisibly — unlike an xdg_toplevel it never shows
    // up in the window list / alt-tab.
    let layer = layer_shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Background,
        c"control-center-idle-inhibit".to_string_lossy().into_owned(),
        &qh,
        (),
    );
    layer.set_size(1, 1);
    layer.set_margin(0, 0, 0, 0);
    // Never take input, even if the compositor grows the surface.
    let region = compositor.create_region(&qh, ());
    region.add(0, 0, 0, 0);
    surface.set_input_region(Some(&region));
    region.destroy();
    // First commit establishes the role; the compositor answers with a
    // configure whose serial we must ack before mapping.
    surface.commit();
    queue.roundtrip(&mut globals).ok()?;
    let Some(serial) = globals.configured else {
        return None; // compositor never sent a configure — surface can't map
    };
    layer.ack_configure(serial);

    // Map the surface with a 1×1 fully transparent buffer — mapped content is
    // what makes the compositor honor the inhibitor; it must paint nothing.
    let memfd = memfd_with_pixel().ok()?;
    let pool = shm.create_pool(memfd.as_fd(), 4, &qh, ());
    // create_buffer args: (offset, width, height, stride, format).
    let buffer = pool.create_buffer(0, 1, 1, 4, wl_shm::Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.damage(0, 0, 1, 1);
    surface.commit();
    pool.destroy();
    buffer.destroy();

    let inhibitor = manager.create_inhibitor(&surface, &qh, ());
    queue.roundtrip(&mut globals).ok()?;
    Some(WaylandInhibitor {
        _conn: conn,
        _surface: surface,
        _layer: layer,
        _inhibitor: inhibitor,
    })
}

/// One fully transparent 1×1 ARGB pixel in a sealed memfd — the smallest
/// buffer that maps a surface. The fd only needs to outlive the `wl_buffer`
/// creation; the compositor holds its own reference to the pool contents.
fn memfd_with_pixel() -> std::io::Result<std::os::fd::OwnedFd> {
    use std::io::Write;
    use std::os::fd::FromRawFd;
    let fd = unsafe {
        libc::memfd_create(c"control-center-idle-inhibit".as_ptr(), libc::MFD_CLOEXEC)
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    // 0x 00 00 00 00 = fully transparent in ARGB8888.
    file.write_all(&[0x00, 0x00, 0x00, 0x00])?;
    file.flush()?;
    // Hand the fd over through `File` itself: keeping the `File` alive here
    // while also wrapping the raw fd in an `OwnedFd` would close it twice.
    Ok(file.into())
}

struct WaylandInhibitor {
    _conn: Connection,
    _surface: wl_surface::WlSurface,
    _layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    _inhibitor: zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
}

// --- wayland dispatch plumbing -------------------------------------------------
// The registry handler binds the four globals we need (compositor for the
// surface + input region, shm for the buffer that maps it, the layer shell for
// the role, and the idle-inhibit manager itself).

#[derive(Default)]
struct Globals {
    compositor: Option<wl_compositor::WlCompositor>,
    manager: Option<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    /// Serial from the layer surface's configure event — the ack MUST echo it.
    configured: Option<u32>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(5), qh, ()))
                }
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(1), qh, ())),
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(1), qh, ()))
                }
                "zwp_idle_inhibit_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(1), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Globals {
    fn event(
        state: &mut Self,
        _: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = event {
            state.configured = Some(serial);
        }
    }
}

macro_rules! noop_dispatch {
    ($($t:ty),+) => {
        $(impl Dispatch<$t, ()> for Globals {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        })+
    };
}

noop_dispatch!(
    wl_compositor::WlCompositor,
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    wl_region::WlRegion,
    wl_surface::WlSurface,
    zwlr_layer_shell_v1::ZwlrLayerShellV1,
    zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
    zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1
);

impl PreventSleep {
    /// Whether every lock source is currently held. On COSMIC the screensaver
    /// cookie and the Wayland inhibitor are the ones that actually block
    /// screen-off, so a partially-held set can leave the screen blanking even
    /// though the toggle reads "on".
    pub fn complete(&self) -> bool {
        self._logind.is_some() && self._wayland.is_some() && self._screensaver.is_some()
    }

    /// (Re-)acquire whichever locks are missing, logging the failures. Called
    /// right after startup — at boot the applet can come up before cosmic-idle
    /// (the ScreenSaver D-Bus service) or the Wayland session are ready, so a
    /// one-shot acquire would leave the toggle on with locks missing forever.
    pub fn acquire_missing(&mut self) {
        if self._logind.is_none() {
            self._logind = match logind_acquire() {
                Some(l) => Some(l),
                None => {
                    eprintln!("prevent-sleep: logind inhibitor unavailable");
                    None
                }
            };
        }
        if self._wayland.is_none() {
            self._wayland = match wayland_acquire() {
                Some(w) => Some(w),
                None => {
                    eprintln!("prevent-sleep: Wayland idle-inhibitor unavailable");
                    None
                }
            };
        }
        if self._screensaver.is_none() {
            self._screensaver = match screensaver_acquire() {
                Some(s) => Some(s),
                None => {
                    eprintln!("prevent-sleep: org.freedesktop.ScreenSaver inhibitor unavailable");
                    None
                }
            };
        }
    }
}

/// Acquire every lock source that answers. Whichever locks are available are
/// held; the toggle reads as "on" as long as the returned value lives. `None`
/// only when nothing at all could be reached — each failing leg is logged so a
/// silent partial failure can't hide behind a lit toggle.
pub fn acquire() -> Option<PreventSleep> {
    let mut held = PreventSleep { _logind: None, _wayland: None, _screensaver: None };
    held.acquire_missing();
    if held._logind.is_none() && held._wayland.is_none() && held._screensaver.is_none() {
        eprintln!("prevent-sleep: no inhibitor could be acquired at all");
        None
    } else {
        Some(held)
    }
}
