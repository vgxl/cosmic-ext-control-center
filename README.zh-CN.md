# cosmic-ext-control-center

> [!NOTE]
> **这是社区的接续维护版本。** 原作者
> （[Pyxyll](https://github.com/vgxl/cosmic-ext-control-center)）已迁移离开
> COSMIC 桌面，在 0.1.5 之后停止维护。本分支接手了该项目：新增"防睡眠"开关、
> 音量/麦克风实时同步、COSMIC 过增益支持，并修复了多处音频拖动问题（详见
> [相对上游的变动](#相对上游的变动)）。欢迎提交 issue 和 PR。
> English README: [README.md](README.md)。

[COSMIC](https://system76.com/cosmic) 桌面的模块化控制中心。

你可以在编辑器窗口里随意排列想要的模块磁贴（Wi-Fi、音量、媒体、电源模式、
各种仪表等）。面板小程序会以弹出的形式显示同一套布局，磁贴在其中是实时、
可交互的。如果想要的磁贴没有内置，可以通过一个小小的 RON 插件文件添加——
运行 shell 命令即可，无需写 Rust 或重新编译。

状态：早期预览版（0.1.5 基础上的社区延续）。日常可用，但仍有一些粗糙之处，
参见[已知问题](#已知问题)。

<p align="center">
  <img src="docs/applet.png" alt="面板小程序弹出层，显示实时磁贴" width="360">
  <img src="docs/editor.png" alt="带模块面板的编辑器窗口" width="480">
</p>

左边是小程序弹出层，磁贴实时可交互；右边是编辑器窗口，可以从模块面板中
添加、删除、排序和调整磁贴大小。

## 两个组成部分

| 可执行文件 | 说明 |
|---|---|
| `cosmic-ext-control-center` | 编辑器窗口。添加、删除、排序、调整磁贴大小，布局保存到 `cosmic-config`。 |
| `cosmic-ext-control-center-applet` | 面板小程序。弹出层显示你配置的磁贴，实时可交互。齿轮按钮打开编辑器。面板按钮可选显示一组实时状态图标（Wi-Fi/VPN、音频、蓝牙、电源模式），而不是单个图标。 |

## 模块

音量（跟随系统的过增益开关：正常 100%，开启后最高 150%）、麦克风、媒体
（MPRIS）、Wi-Fi（SSID 和链路速率）、VPN、蓝牙、飞行模式、电源模式、外观
（浅色/深色）、CPU/GPU/内存/磁盘仪表、组合系统监视器、电量读数、通知中心、
勿扰模式、分区分隔线，以及会话/电源操作（锁定、睡眠、注销、重启、关机）。

## 相对上游的变动

本分支从 0.1.5 继续开发，新增或修复了：

- **防睡眠开关** —— 电量读数旁的咖啡杯按钮，同时持有 logind `idle`
  inhibitor（阻止系统挂起）和 Wayland `zwp_idle_inhibit_v1` inhibitor
  （cosmic-comp 熄屏时不理会 logind inhibitor，所以必须直接用该协议阻止
  熄屏）。开关状态会持久化，重启后自动恢复。
- **音量/麦克风实时同步** —— 通过 `pactl subscribe` 事件，按下音量键或
  其他应用改变音量后，音频磁贴在毫秒级内刷新，而不是等下一次轮询。
- **COSMIC 过增益** —— 音量滑杆的上限跟随系统的过增益开关（正常 100%，
  开启后最高 150%），滑杆上标出 100% 刻度，超过 100% 时百分比读数使用
  强调色。
- **修复音频拖动竞争** —— 以前拖动滑杆会留下多个 `wpctl` 写进程互相竞争，
  最后*完成*的那个（而不是最后请求的那个）生效，导致设备实际电平和滑杆
  显示不一致。现在只保留最新的一个写进程，并且在写入后的短暂宽限期内忽略
  先于写入读到的轮询结果。
- **弹出层打开即同步** —— 弹出层在第一帧就刷新状态，而不是显示上次关闭
  时的旧状态。

## 安装

### Arch（AUR）

```sh
paru -S cosmic-ext-control-center        # 或你喜欢的 AUR 助手
```

### Debian 和 Fedora（预编译包）

从[最新 release](https://github.com/vgxl/cosmic-ext-control-center/releases/latest)
下载 `.deb` 或 `.rpm` 用包管理器安装。也附带了静态二进制的 `.tar.gz`，
可用于其他发行版。

### 本地构建覆盖已安装的包

在已经装了包的机器上试用改动：

```sh
sudo bash install-local.sh    # 构建 release，安装到 /usr/local，并重启小程序
```

### 从源码

需要 Rust 工具链和 [`just`](https://github.com/casey/just)。

```sh
git clone https://github.com/vgxl/cosmic-ext-control-center
cd cosmic-ext-control-center
sudo just install                    # 构建 release 并安装到 /usr
```

卸载用 `sudo just uninstall`。

### 把小程序添加到面板

在 COSMIC 会话中，进入 设置 > Desktop > Panel（或 Dock）> Add applet >
Control Center。点击面板图标打开弹出层。齿轮打开编辑器，编辑器也会以
"Control Center"的名字出现在应用启动器里。

## 插件

插件是放在 `~/.config/cosmic-ext-control-center/plugins/` 目录下的一个
`.ron` 文件。它描述一个磁贴和一组控件，每个控件绑定 shell 命令：

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

控件类型有 `Slider`、`Toggle`、`Label`、`Button` 四种。`get` 读取状态，
`set` 和 `run` 执行操作（`{value}` 会被替换），`poll`（单位为秒）定时重新
读取 `get`。上面的示例就是
[`examples/echo-demo.ron`](examples/echo-demo.ron)：把它放进插件目录，在
编辑器里添加，就能看到全部四种控件的效果。

安全提示：插件命令通过 `sh -c` 以你的用户权限运行。只安装你信任的清单，
就像对待 shell 脚本或 AUR 助手一样。

## 已知问题

内联的 Wi-Fi/蓝牙/VPN 选择抽屉打开和关闭是瞬时的，没有滑动动画。COSMIC
用软件渲染（tiny_skia）绘制小程序弹出层，每帧对整个弹出层重排并做动画的
开销太大，无法保持流畅，所以抽屉在没有动画的情况下保持了灵敏。不引起
重排的过渡动画是一个可能的改进方向。应用在有可用 GPU（Vulkan）后端时优先
使用它，这能保证弹出层其余部分的响应性。

插件系统可用但仍是实验性的。目前只支持 `Cmd` 操作和基础样式。D-Bus 和
HTTP 操作，以及接近原生的插件磁贴外观，都在计划中。

磁贴布局随列宽变化：开关磁贴在 1 列宽时收缩为图标，CPU/GPU/内存仪表在
3 列宽及以上变为历史曲线图，磁盘仪表变为容量条。少数磁贴（如组合系统
监视器）在所有尺寸下仍共用一套布局，针对各尺寸的逐模块打磨会继续进行。

媒体播放器有两种样式，可在编辑器的配置侧栏（磁贴的齿轮）中按磁贴选择：
"COSMIC"遵循设计系统（普通卡片），是默认值；"Album art"使用模糊专辑封面
背景。

## 使用的技术

[libcosmic](https://github.com/pop-os/libcosmic)，MPL-2.0。本项目采用
[MIT](LICENSE) 许可证。捆绑和链接的依赖保留各自的许可证：libcosmic 及其
iced 分支为 MPL-2.0；`zbus`、`image`、`serde`、`ron` 为 MIT 或 Apache-2.0。
