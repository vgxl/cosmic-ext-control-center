# cosmic-ext-control-center

> [!NOTE]
> **This is a community continuation.** The original author
> ([Pyxyll](https://github.com/vgxl/cosmic-ext-control-center)) has moved off
> COSMIC and stopped maintaining the project after 0.1.5. This fork picks the
> project up: it adds a "prevent sleep" toggle, real-time volume/microphone
> sync, COSMIC over-amplification support, and several audio drag fixes (see
> [Changes over upstream](#changes-over-upstream)). Issues and PRs are welcome.
> 中文说明见 [README.zh-CN.md](README.zh-CN.md)。

A modular control center for the [COSMIC](https://system76.com/cosmic) desktop.

You arrange the module tiles you want (Wi-Fi, volume, media, power profile,
gauges, and so on) in an editor window. A panel applet then shows that same
layout in a popup, where the tiles are live and interactive. If a tile you want
isn't built in, you can add one with a small RON plugin file that runs shell
commands, without writing Rust or recompiling.

Status: early preview (0.1.5). It works and is usable day to day, but it still
has rough edges. See [Known issues](#known-issues).

<p align="center">
  <img src="docs/applet.png" alt="The panel applet popup showing live tiles" width="360">
  <img src="docs/editor.png" alt="The editor window with the module palette" width="480">
</p>

On the left, the applet popup with live, interactive tiles. On the right, the
editor window where you add, remove, reorder, and resize tiles from the palette.

## Two pieces

| Binary | What it is |
|---|---|
| `cosmic-ext-control-center` | The editor window. Add, remove, reorder, and resize tiles. Saves the layout to `cosmic-config`. |
| `cosmic-ext-control-center-applet` | The panel applet. A popup that shows your configured tiles, live and interactive. Its gear button opens the editor. The panel button can optionally show a cluster of live status icons (Wi-Fi/VPN, audio, Bluetooth, power profile) instead of a single icon. |

## Modules

Volume (follows the system's amplification switch: 100% normally, up to 150%
when it's on), Microphone, Media (MPRIS), Wi-Fi (SSID and link rate), VPN,
Bluetooth,
Airplane mode, Power profile, Appearance (light/dark), CPU/GPU/RAM/Disk gauges,
a combined system monitor, battery readout, a notification center, Do Not
Disturb, section divider, and session/power actions (lock, sleep, log out,
restart, shut down).

## Install

### Arch (AUR)

```sh
paru -S cosmic-ext-control-center        # or your preferred AUR helper
```

### Debian and Fedora (prebuilt)

Download the `.deb` or `.rpm` from the
[latest release](https://github.com/vgxl/cosmic-ext-control-center/releases/latest)
and install it with your package manager. A static-binary `.tar.gz` is also
attached for other distributions.

### Local build over an installed package

For trying changes on a machine that already has the package installed:

```sh
sudo bash install-local.sh    # builds release, installs to /usr/local, restarts the applet
```

### From source

Needs the Rust toolchain and [`just`](https://github.com/casey/just).

```sh
git clone https://github.com/vgxl/cosmic-ext-control-center
cd cosmic-ext-control-center
sudo just install                    # builds release and installs to /usr
```

Uninstall with `sudo just uninstall`.

### Add the applet to your panel

In a COSMIC session, go to Settings > Desktop > Panel (or Dock) > Add applet >
Control Center. Click the panel icon to open the popup. The gear opens the
editor, which also shows up in your app launcher as "Control Center".

## Changes over upstream

This fork continues from 0.1.5. What it adds or fixes:

- **Prevent sleep toggle** — a coffee-cup button next to the battery readout
  holds both a logind `idle` inhibitor (system suspend) and a Wayland
  `zwp_idle_inhibit_v1` inhibitor (cosmic-comp ignores logind inhibitors when it
  blanks the screen, so blocking screen-off needs the protocol directly). The
  toggle is persisted and re-acquired on startup.
- **Real-time volume/microphone sync** — `pactl subscribe` events refresh the
  audio tiles within milliseconds of a volume-key press or a change made in
  another app, instead of waiting for the next poll.
- **COSMIC over-amplification** — the volume slider's ceiling follows the
  system's amplification switch (100% normally, up to 150% when it's on),
  draws the 100% breakpoint on the slider, and tints the percentage readout
  with the accent color past 100%.
- **Fixed audio drag races** — a slider drag used to leave several `wpctl`
  writes racing, and the last one to *finish* (not the last asked for) won, so
  the sink could land on a level the slider didn't show. Only the newest write
  stays in flight now, and polls that read the device before a write are
  ignored for a short grace window.
- **Popup opens in sync** — the popup refreshes its state on the first frame
  instead of showing whatever was true when it last closed.

## Plugins

A plugin is a single `.ron` file in `~/.config/cosmic-ext-control-center/plugins/`.
It describes a tile and a list of controls, each bound to shell commands:

```ron
Manifest(
  id: "demo.echo", name: "Echo Demo", icon: "utilities-terminal-symbolic", size: Large,
  controls: [
    Label(id:"clock", label:"Clock", get: Cmd("date +%H:%M:%S"), poll: 1.0),
    Slider(id:"level", label:"Level", min:0.0, max:100.0,
           set: Cmd("echo level={value} >> /tmp/ccc-demo.log")),
    Toggle(id:"flag", label:"Flag", set: Cmd("notify-send 'Echo Demo' 'flag = {value}'")),
    Button(id:"ping", label:"Ping", run: Cmd("notify-send 'Echo Demo' 'ping'")),
  ],
)
```

The control types are `Slider`, `Toggle`, `Label`, and `Button`. `get` reads
state, `set` and `run` act (with `{value}` substituted), and `poll` (in seconds)
re-reads `get`. The manifest above is [`examples/echo-demo.ron`](examples/echo-demo.ron):
drop it into the plugins directory and add it from the editor to see all four
control types working.

Security note: plugin commands run with your privileges via `sh -c`. Only
install manifests you trust, the same as you would a shell script or an AUR
helper.

## Known issues

The inline Wi-Fi/Bluetooth/VPN selection drawer opens and closes instantly,
without a slide animation. COSMIC renders applet popups in software (tiny_skia),
so animating a reflow of the whole popup every frame is too expensive to stay
smooth; the drawer is snappy without it. A non-reflowing transition is a
possible future improvement. The app prefers the GPU (Vulkan) backend where one
is available, which keeps the rest of the popup responsive.

The plugin system is functional but experimental. For now it only supports
`Cmd` actions and basic styling. D-Bus and HTTP actions, plus native-looking
plugin tiles, are planned.

Tile layout varies by column width: toggles collapse to an icon at 1col, the
CPU/GPU/RAM gauges become history graphs at 3col and up, and the disk gauge
becomes a capacity bar. A few tiles (such as the combined system monitor) still
reuse one layout across sizes, so per-module polish across the range continues.

The media player has two styles, chosen per tile from the editor's config
sidebar (the tile's gear): "COSMIC" follows the design system (a plain card) and
is the default, while "Album art" uses the blurred album-art backdrop.

## Built with

[libcosmic](https://github.com/pop-os/libcosmic), which is MPL-2.0. This project
is [MIT](LICENSE). Bundled and linked dependencies keep their own licenses:
libcosmic and its iced fork are MPL-2.0; `zbus`, `image`, `serde`, and `ron` are
MIT or Apache-2.0.
