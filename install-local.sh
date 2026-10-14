#!/usr/bin/env bash
# Install the locally built Control Center into /usr/local, replacing the
# distribution package (which has been uninstalled).
#
# Usage:  sudo bash install-local.sh
#
# Installs both binaries plus the desktop/metainfo files: the panel finds the
# applet through the .desktop entry, so it has to live in /usr/local/share now
# that the package no longer provides one in /usr/share.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")"

BIN_DIR=/usr/local/bin
APP_DIR=/usr/local/share/applications
META_DIR=/usr/local/share/metainfo

# Build before installing. Installing whatever happens to sit in
# target/release is the easy mistake: the new code never lands in the binary
# that gets copied, and the old applet keeps running while the sources look
# updated (this exact trap made a mixer-event fix look like it did nothing).
cargo build --release

for b in cosmic-ext-control-center cosmic-ext-control-center-applet; do
    if [ ! -x "target/release/$b" ]; then
        echo "缺少 target/release/$b —— 先运行：cargo build --release" >&2
        exit 1
    fi
done

install -Dm0755 target/release/cosmic-ext-control-center-applet "$BIN_DIR/cosmic-ext-control-center-applet"
install -Dm0755 target/release/cosmic-ext-control-center        "$BIN_DIR/cosmic-ext-control-center"

install -Dm0644 resources/com.pyxyll.CosmicExtControlCenter.desktop       "$APP_DIR/com.pyxyll.CosmicExtControlCenter.desktop"
install -Dm0644 resources/com.pyxyll.CosmicExtControlCenterApplet.desktop "$APP_DIR/com.pyxyll.CosmicExtControlCenterApplet.desktop"
install -Dm0644 resources/com.pyxyll.CosmicExtControlCenter.metainfo.xml  "$META_DIR/com.pyxyll.CosmicExtControlCenter.metainfo.xml"

# Applet's own symbolic icons (referenced by name from the code).
install -Dm0644 resources/coffee-symbolic.svg \
    /usr/local/share/icons/hicolor/scalable/apps/coffee-symbolic.svg

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" || true
fi

echo
echo "已安装到 /usr/local："
ls -l "$BIN_DIR"/cosmic-ext-control-center* "$APP_DIR"/com.pyxyll.CosmicExtControlCenter*.desktop

# 替换正在运行的实例：panel 检测到 applet 退出后会用新二进制自动重新拉起。
pids=$(pgrep -f 'cosmic-ext-control-center(-applet)?$' || true)
if [ -n "$pids" ]; then
    echo
    echo "正在重启运行中的实例 (pid: $pids)……"
    kill $pids
    # 等旧进程真正退出，避免新旧实例并存。
    for _ in $(seq 1 20); do
        pgrep -f 'cosmic-ext-control-center(-applet)?$' >/dev/null || break
        sleep 0.25
    done
    if pgrep -f 'cosmic-ext-control-center(-applet)?$' >/dev/null; then
        echo "旧进程未退出，强制结束。"
        pkill -9 -f 'cosmic-ext-control-center(-applet)?$' || true
    fi
    echo "用重启 panel 的方式拉起新版 applet（panel 由 cosmic-session 自动重启，"
    echo "重启后会把所有 applet 用新二进制重新拉起——直接 kill applet 后 panel 常常不会自动重拉）。"
    pkill -x cosmic-panel || true
fi

echo
echo "若面板上的图标没回来，注销重登一次（或在 设置 > Desktop > Panel 里重新添加 Control Center）。"
