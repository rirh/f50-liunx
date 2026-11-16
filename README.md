<div align="center">

<img src="f50-web/public/favicon.svg" width="112" height="112" alt="F50 Linux">

# F50 Linux

**让中兴 ZTE F50 随身 5G 路由器运行 OpenWrt**

一个 Rust 守护进程 `f50d` 接管基带、移动数据、短信、Wi-Fi、USB 和系统，再配一个网页控制台。<br>
原厂 Android 保留在另一个槽位，随时可以切回去。

![OpenWrt 25.12](https://img.shields.io/badge/OpenWrt-25.12-00B5E2?logo=openwrt&logoColor=white)
![Rust](https://img.shields.io/badge/f50d-Rust-B7410E?logo=rust&logoColor=white)
![React](https://img.shields.io/badge/web-React%20%2B%20Vite-61DAFB?logo=react&logoColor=black)
![Go](https://img.shields.io/badge/proxy-Go%20%2B%20Xray-00ADD8?logo=go&logoColor=white)
![License: MIT](https://img.shields.io/badge/license-MIT-2dd4bf)

[功能](#功能) · [首次安装](#首次安装) · [系统切换](#在-android-和-openwrt-之间切换) · [日常开发](#日常开发) · [维护](#维护)

</div>

---

## 亮点

- **原生 OpenWrt**：完整的 OpenWrt 25.12，`apk` 装包、LuCI 协议、WireGuard、SQM 都能用
- **一个守护进程管全部**：`f50d` 以 ubus 对象 `f50.*` 提供基带、数据、短信、网络和系统功能，网页和命令行用的是同一套接口
- **双系统**：Android 和 OpenWrt 各占一个槽位，命令或按钮切换；Linux 连续两次启动失败自动回到 Android
- **透明代理**：内置 Xray 引擎和分流规则，局域网设备和本机流量都能接管

## 功能

网页控制台在 `http://192.168.77.1/`，用 OpenWrt 的 root 账户登录。

| 分区         | 内容                                                                                                                                                                                 |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **仪表盘**   | 运营商、网络制式、实时网速、本期流量、信号，以及 CPU、内存、存储、温度；常用开关一页完成                                                                                             |
| **移动网络** | 信号与小区（服务小区、邻区、载波聚合）、移动数据与 APN、流量统计与限额、SIM 卡、网络锁定（锁频 / 锁小区）                                                                            |
| **短信**     | 收发与会话                                                                                                                                                                           |
| **连接**     | Wi-Fi 热点、Wi-Fi 上网（连接其他 Wi-Fi 作为上行）、局域网与 DHCP、USB 网络                                                                                                           |
| **代理**     | 订阅与节点（VLESS、Hysteria2、Trojan、VMess、Shadowsocks、SOCKS5、HTTP）、策略组与测速、分流（智能分流 / 全局代理 / 全部直连，自定义规则与应用分流）、连接与日志；可指定设备不走代理 |
| **系统**     | 系统信息、性能与温控、指示灯、系统设置、系统更新、启动                                                                                                                               |
| **工具**     | 命令行（浏览器里的 root shell，刷新页面不断开）、AT 终端、后台任务、系统日志                                                                                                         |

页面数据通过 `f50.live` 实时推送（标题栏显示「实时」）；推送不可用时自动改为定时刷新。

## 快速开始

```sh
make backup     # 先备份不可替代的分区
make kernel     # 构建内核（只需一次）
make install    # 构建 f50d 和网页，开启 ADB，刷入 OpenWrt
```

装好后用 USB 连接电脑，打开 `http://192.168.77.1/`。详细步骤见下文。

## 目录

| 目录                                | 内容                                                                                                                                       |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| [`f50d/`](f50d/README.md)           | 设备守护进程（Rust），以 ubus 对象 `f50.*` 提供全部功能，接口见 [`f50d/docs/API.md`](f50d/docs/API.md)                                     |
| `f50-web/`                          | 网页控制台（React + Vite），经 uhttpd 的 `/ubus` 调用 `f50d`                                                                               |
| [`f50-proxy/`](f50-proxy/README.md) | 代理引擎（Go：Xray-core + 分流规则，移植自 one-node 的 onecore），由 `f50d` 的 `f50.proxy` 管理，设计见 [`DESIGN.md`](f50-proxy/DESIGN.md) |
| `install.sh`、`installer/`、`boot/` | 从原厂 Android 首次安装：启动镜像、Linux 槽位、根文件系统                                                                                  |
| `uninstall.sh`                      | 卸载 Linux，恢复原厂 Android                                                                                                               |
| `android/magisk/`                   | Android 里切回 Linux 用的 Magisk 模块                                                                                                      |
| `systems/openwrt/`                  | OpenWrt 根文件系统（docker 构建，内含 f50d 和网页）                                                                                        |
| `kernel/`                           | 5.4 厂商内核的构建，产物在 `out/`                                                                                                          |
| `make/`                             | 全部 make 命令，`make` 查看说明                                                                                                            |

## 准备

在 macOS 或 Linux 上：

- `adb`（macOS：`brew install android-platform-tools`）、Python 3、`lz4`、docker
- Rust 稳定版（1.98+，`aarch64-unknown-linux-musl` 目标），构建 f50d
- Node.js + pnpm，构建网页
- Go 1.27+，构建代理引擎
- 一根能传数据的 USB 线

可选：复制 `make/local.mk.example` 为 `make/local.mk`，修改设备地址等设置。

## 首次安装

设备运行原厂固件时：

```sh
make backup     # 先备份不可替代的分区
make kernel     # 构建内核（只需一次）
make install    # 构建 f50d 和网页，开启 ADB，刷入 OpenWrt
```

`make install` 会自己开启 ADB 并准备 root。如果失败，先在原厂管理页 `http://192.168.0.1` 用**管理密码**打开
「ADB 调试」，或运行 `make adb`，再重试。只想检查空间、不写入：`make install-check`。

安装完成后设备重启进入 OpenWrt：电脑通过 USB 网络连到 `192.168.77.1`，打开 `http://192.168.77.1/` 登录。

> [!IMPORTANT]
> 刷机前务必先运行 `make backup`。备份里有设备独有的分区，丢了无法从别处找回。

## 在 Android 和 OpenWrt 之间切换

在这台电脑的项目目录中，通过 USB 数据线执行：

```sh
make android    # 返回 Android，并设为默认启动
make openwrt    # 自动准备 Android root，切回 OpenWrt，并恢复 OpenWrt 默认启动
```

Android 下运行 `make openwrt` 时，电脑还需连接 F50 的 Wi-Fi。命令复用本项目构建的 `work/busybox` 和切换脚本，
无需安装 Magisk；切换不会重新刷入镜像或同步 TrustOS。多台 USB 设备连接时用 `F50_SERIAL=...` 指定目标。

也可以不用电脑：

- **OpenWrt → Android**：侧边栏账户菜单里的「切换到 Android」，或在设备上执行
  `ubus call f50.boot android`（两秒后重启进入 Android）
- **Android → OpenWrt**：Magisk → 模块 → F50 Linux switch → 操作；或在电脑上
  `adb shell su -c f50-linux`（`su -c "f50-linux status"` 只查看两个槽位）

切换只改 misc 分区里的启动控制块，不动 Android 的系统和数据。Linux 连续两次启动失败会自动回到 Android。

看设备当前在哪个系统：`adb devices -l`，序列号以 `F50LINUX-` 开头是 OpenWrt，纯数字是 Android。

## 日常开发

```sh
make dev            # 网页开发服务器 127.0.0.1:5173，/ubus 转发到设备
make test           # f50d 和代理引擎的测试，网页 lint 和类型检查
make deploy         # 通过 SSH 部署 f50d、代理引擎和网页到运行中的设备
make deploy-f50d    # 只换 f50d 并重启守护进程
make deploy-web     # 只部署网页
make ssh            # 登录设备
make status         # 内核、版本、内存和 f50d 状态
make logs           # 跟踪系统日志
```

## 维护

```sh
make uninstall        # 卸载 Linux，恢复原厂 Android
make reset-password   # 在 Android 里重置 OpenWrt 的 root 密码
make collect-logs     # Linux 启动失败回到 Android 后，收集启动日志
make backup-full      # 全分区备份
```

## 许可

MIT，见 [LICENSE](LICENSE)。
