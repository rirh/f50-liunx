# 项目目录说明

本文按 2026-11-12 的本地目录整理，覆盖所有顶层目录和主要功能子目录。内核驱动内部的大量目录按驱动模块归类。

这个项目包含四类内容：电脑上的安装与构建工具、设备上运行的程序、内核与硬件适配代码、本机产生的安装资料。目录里有源码或编译产物，不代表这些内容已经安装到设备。

## 1. 按你要做的事情找代码

| 要做什么 | 从哪里开始 |
|---|---|
| 本地预览 OpenWrt 界面 | 根目录 `make dev`，访问 `http://127.0.0.1:27547/` |
| 改首页布局、状态卡片 | `openwrt/luci-app-mu300/htdocs/luci-static/resources/view/mu300/home.js` |
| 改公共样式、显示辅助函数 | `openwrt/luci-app-mu300/htdocs/luci-static/resources/mu300/common.js`，CSS 也写在这个 JS 里 |
| 改短信、VPN、流量等页面 | `openwrt/luci-app-mu300/htdocs/luci-static/resources/view/mu300/` 下对应的 JS |
| 改网页菜单 | `openwrt/luci-app-mu300/root/usr/share/luci/menu.d/` |
| 改网页接口和接口权限 | `openwrt/luci-app-mu300/root/usr/libexec/rpcd/`、`root/usr/share/rpcd/acl.d/` |
| 改设备控制逻辑 | 先看 `openwrt/luci-app-mu300/root/usr/libexec/unisoc-modem/` 的适配器，再找它调用的设备命令 |
| 改共享的设备命令 | `rootfs/overlay/opt/mu300/bin/`、`rootfs/overlay/opt/mu300/lib/` |
| 改 OpenWrt 默认配置、开机服务 | `openwrt/overlay/`；仅面板版需要的内容看 `openwrt/luci-overlay/` |
| 改 Ubuntu 的网页面板 | `rootfs/panel/`，它使用 Cockpit，与 OpenWrt 的 LuCI 是两套界面 |
| 改安装流程或电脑端安装窗口 | `install.sh`、`tools/`、`gui/` |
| 改开机选系统、挂载根文件系统 | `boot/init` |
| 改 5.4 内核或驱动 | `kernel/` |
| 改主线内核或移植的驱动 | `upstream/` |
| 找本机安装记录、离线包、备份 | `work/`、`backup/`，不要把它们当作源码目录 |

## 2. 所有顶层目录

| 目录 | 用途 | 在哪里使用 |
|---|---|---|
| `openwrt/` | OpenWrt 系统组装、系统配置、LuCI 网页及其后端 | 构建在电脑上完成，生成内容安装到设备 |
| `rootfs/` | Ubuntu 等系统的根文件系统组装，以及多种系统共用的 MU300 命令 | 构建在电脑上完成，运行程序在设备上 |
| `boot/` | 生成 Linux 启动镜像、早期启动、选择和挂载系统、切换启动槽位 | 镜像生成在电脑上，`init` 在设备早期启动时运行 |
| `kernel/` | 厂商 5.4 内核的构建入口、配置片段和驱动补丁 | 电脑上编译，内核与模块在设备运行 |
| `upstream/` | Linux 主线内核的构建、移植、补丁、驱动和调试资料 | 电脑上构建，内核与模块在设备运行 |
| `android/` | 从 Android 切回 Linux 的 Magisk 模块及相关文件 | 模块在电脑上打包，在设备 Android 中使用 |
| `android-vendor/` | 从设备提取厂商程序、固件依赖和权限信息的脚本及文件清单 | 提取工具在电脑上运行，提取内容供设备 Linux 使用 |
| `arch/` | Arch Linux ARM 根文件系统的构建脚本 | 备用发行版构建入口，不是当前 OpenWrt 界面的组成部分 |
| `gui/` | Python 图形安装器，封装安装选项、设备检测和进度显示 | 电脑端桌面程序 |
| `i18n/` | 命令行和图形安装器的翻译文件 | 电脑端安装器使用 |
| `tools/` | 安装辅助、发布打包、本地开发代理、设备工具源码等 | 有的在电脑上运行，有的编译后安装到设备 |
| `make/` | 按开发、部署、设备管理和调试拆分的 Make 命令与本机配置示例 | 电脑端使用，详见 [命令说明](../make/README.md) |
| `tests/` | 自动化测试、模拟设备、固定输入和升级实验 | 测试环境使用；不作为正常系统服务部署 |
| `docs/` | 构建、发布、安装协议、硬件研究和使用说明 | 开发者阅读 |
| `stock/` | 原厂固件来源和恢复说明 | 参考资料；当前目录仅有说明，不是完整原厂系统源码或固件备份 |
| `out/` | 当前本机的内核与模块产物 | 构建或组装镜像时读取；可能来自编译，也可能来自预编译包解压 |
| `work/` | 下载包、提取的厂商文件、设备专用镜像、部署日志等本机工作资料 | 安装和部署时使用，Git 忽略 |
| `backup/` | 原设备备份、源码裁剪前的归档等 | 恢复和追溯使用，Git 忽略 |
| `.git/` | Git 的版本管理元数据 | 开发工具使用，不部署到设备 |
| `.claude/` | 本地 Claude 开发工具配置 | 开发工具使用，不属于设备运行程序 |

根目录的重要文件：`Makefile` 是统一命令入口，加载 `make/*.mk`；`install.sh`、`uninstall.sh` 负责安装和卸载；`README.md` 是总说明；`.gitignore` 定义产物与本机资料的忽略规则。

## 3. `openwrt/`：你修改界面最常用的目录

```text
openwrt/
├── build-rootfs.sh           组装 OpenWrt 系统包
├── luci-app-mu300/           MU300 的 LuCI 插件
│   ├── htdocs/              浏览器加载的文件，对应设备 /www/
│   │   └── luci-static/resources/
│   │       ├── view/mu300/  首页、短信、VPN、流量等各个页面
│   │       └── mu300/       公共样式、辅助函数、接口声明、VPN 公共代码
│   ├── root/               插件的设备端文件，对应设备文件系统根目录 /
│   └── po/                 插件界面的翻译源码
├── overlay/                普通 OpenWrt 和面板版共用的系统适配
├── luci-overlay/           面板版 openwrt-luci 的额外系统适配
├── patches/                对 OpenWrt 组件和构建过程的补丁
└── keys/usign/             验证下载的 OpenWrt 文件所用的公钥
```

### `luci-app-mu300/` 内部如何分工

| 位置 | 内容 | 安装到设备后的位置 |
|---|---|---|
| `htdocs/luci-static/resources/view/mu300/` | 各个页面的渲染和交互逻辑 | `/www/luci-static/resources/view/mu300/` |
| `htdocs/luci-static/resources/mu300/common.js` | 共享 CSS、数据格式化、RPC 声明等 | `/www/luci-static/resources/mu300/common.js` |
| `htdocs/luci-static/resources/mu300/vpn.js` | VPN 页面共用代码 | `/www/luci-static/resources/mu300/vpn.js` |
| `root/usr/share/luci/menu.d/` | 菜单名称、排序和页面路由 | `/usr/share/luci/menu.d/` |
| `root/usr/share/rpcd/acl.d/` | 页面可调用哪些接口、读写哪些资源 | `/usr/share/rpcd/acl.d/` |
| `root/usr/share/rpcd/ucode/` | 用 ucode 实现的 RPC 服务，例如网速采样 | `/usr/share/rpcd/ucode/` |
| `root/usr/libexec/rpcd/` | `mu300dash`、`mu300vpn`、`mu300term` 等接口实现 | `/usr/libexec/rpcd/` |
| `root/usr/libexec/unisoc-modem/` | 将面板操作转成设备命令的适配器，如 AT、锁频、流量、升级 | `/usr/libexec/unisoc-modem/` |
| `root/usr/share/unisoc-modem/` | 适配器共用的 Shell 函数 | `/usr/share/unisoc-modem/` |
| `root/etc/config/` | 插件的 UCI 默认配置 | `/etc/config/` |
| `root/etc/init.d/` | 插件后台服务的启动脚本 | `/etc/init.d/` |
| `root/etc/hotplug.d/` | 网络设备和接口变化时的处理脚本 | `/etc/hotplug.d/` |
| `po/<语言>/mu300.po` | 界面翻译源文件，简体中文在 `po/zh_Hans/` | 构建成 `.lmo` 后安装到 `/usr/lib/lua/luci/i18n/` |

插件的 `Makefile` 用来构建 OpenWrt 软件包；仓库根目录的 `Makefile` 用来启动本地开发代理，二者用途不同。插件 `lmo/` 是构建时生成的翻译目录，当前源码中没有预先保存它。

### 三处系统适配目录

`overlay` 表示按路径合并进系统镜像的文件集合。例如 `openwrt/overlay/etc/config/` 会提供设备 `/etc/config/` 中的默认配置。它是项目的源码目录，不是设备运行时的 OverlayFS 分区。

| 目录 | 负责什么 |
|---|---|
| `openwrt/overlay/etc/board.d/` | 设备识别后的默认网络初始化 |
| `openwrt/overlay/etc/config/` | 网络、防火墙、服务等 UCI 默认配置 |
| `openwrt/overlay/etc/init.d/` | OpenWrt 的 procd 服务启动脚本 |
| `openwrt/overlay/etc/hotplug.d/` | 热插拔事件处理 |
| `openwrt/overlay/etc/uci-defaults/` | 首次启动或配置迁移时运行的初始化脚本 |
| `openwrt/overlay/etc/docker/`、`etc/mu300/`、`etc/sysctl.d/`、`etc/profile.d/` | Docker、MU300、内核参数、登录环境的配置 |
| `openwrt/overlay/lib/netifd/` | 蜂窝网络等协议与网络管理器的对接 |
| `openwrt/overlay/lib/preinit/`、`lib/mu300/` | 早期初始化与 OpenWrt 平台辅助脚本 |
| `openwrt/overlay/opt/mu300/` | OpenWrt 专用的命令或兼容补充 |
| `openwrt/overlay/usr/` | 系统辅助命令和额外 RPC 权限等 |
| `openwrt/overlay/www/` | 蜂窝协议配置页、Wi-Fi 提示等额外 LuCI 资源 |
| `openwrt/luci-overlay/etc/` | 面板版额外配置、后台服务和初始化脚本，如网页终端、短信服务 |
| `openwrt/luci-overlay/lib/netifd/` | 面板版的额外网络协议实现 |
| `openwrt/luci-overlay/opt/mu300/` | 短信池、短信转发、IPv6 辅助等面板版所需命令 |

OpenWrt 组装还会复用 `rootfs/overlay/opt/mu300/` 中的通用命令，因此查设备功能时不能只看 `openwrt/`。

## 4. `rootfs/`：系统环境和共用设备命令

`rootfs/Dockerfile` 默认构建 Ubuntu 24.04 的基础系统环境；`rootfs/assemble.sh` 把基础文件系统、项目文件、模块和需要的工具组装成系统包。Docker 在这里是电脑上的构建工具，不意味着设备上的 Ubuntu 运行在 Docker 中。

| 子目录 | 用途 |
|---|---|
| `overlay/opt/mu300/bin/` | 设备命令主体，例如系统更新、系统切换、蜂窝数据、Wi-Fi、CPU、电源、流量、工具菜单 |
| `overlay/opt/mu300/lib/` | 上述命令共用的函数、数据、默认模板和发布校验公钥等 |
| `overlay/etc/mu300/` | MU300 的默认配置 |
| `overlay/etc/systemd/` | Ubuntu 等 systemd 系统的服务与定时任务 |
| `overlay/etc/udev/`、`etc/tmpfiles.d/` | 设备事件、运行目录及权限配置 |
| `overlay/etc/docker/`、`etc/ssh/`、`etc/sysctl.d/`、`etc/modprobe.d/` | Docker、SSH、内核参数、模块加载配置 |
| `overlay/usr/libexec/unisoc-modem/`、`overlay/usr/share/unisoc-modem/` | 设备操作的适配脚本和共用函数 |
| `panel/cockpit/mu300/` | Ubuntu Cockpit 面板的 HTML、JS、CSS 和插件描述 |
| `panel/bin/` | Cockpit 面板的安装、状态查询和设备操作命令 |
| `panel/po/` | Cockpit 面板翻译 |

`rootfs/panel/` 是可选面板组件，不是 OpenWrt LuCI 的前端。修改当前 OpenWrt 首页应去 `openwrt/luci-app-mu300/`。

## 5. `boot/`、`kernel/`、`upstream/`：启动与硬件支持

### `boot/`：已有内核怎样启动系统

| 文件 | 用途 |
|---|---|
| `init` | Linux 早期启动入口：加载模块、准备 USB、挂载 Linux 文件系统、读取系统选择并切换到所选系统 |
| `build-boot-image.py` | 把内核、模块、启动脚本和设备需要的内容组装成启动镜像 |
| `module-order.txt`、`module-order-u30air.txt` | 模块加载顺序 |
| `flash-trial.sh` | 写入试启动镜像及设置试启动信息 |
| `android-boot-linux.sh` | 从 Android 侧重新选择 Linux 启动 |

### `kernel/`：厂商 5.4 内核路线

这里是构建脚本、配置和补丁，不是完整 Linux 内核源码树。构建入口会准备内核源码和构建环境。

| 位置 | 用途 |
|---|---|
| `build-all.sh` | 厂商内核及配套模块的构建入口 |
| `build-linux.sh`、`build-u30air.sh` | 基础内核和机型相关构建 |
| `build-wlan.sh`、`build-mali.sh` | Wi-Fi、GPU 模块构建 |
| `*.config`、`*.fragment` | 原始配置、合并后的配置和项目配置片段 |
| `patches/` | 应用到厂商内核或驱动的补丁 |
| `mu300-dp-enable/`、`build-display.sh` | 显示输出相关的实验模块与构建入口；目录存在不代表本机 HDMI 已可用 |

### `upstream/`：主线内核路线

这里的 `upstream` 指 Linux 主线内核适配，不是本项目 Git 远程仓库的副本。

| 子目录 | 用途 |
|---|---|
| `patches/` | 主线内核需要应用的补丁 |
| `port/` | 移植进内核源码树的实现、头文件以及安装这些改动的脚本 |
| `port/drivers/` | 按子系统分类的移植代码：时钟、CPU/设备频率、电源、USB、PCI、温控、看门狗等 |
| `port/include/` | 移植需要的 Linux 头文件和设备树绑定定义 |
| `modules/` | 在主线内核上单独编译的驱动模块 |
| `dts/` | 设备树源文件与相关研究材料，不代表当前启动一定使用这里的实验设备树 |
| `keys/` | kernel.org 源码签名校验公钥和指纹 |
| `parity/` | 对照厂商 5.4 内核的功能清单、模块清单和硬件记录 |
| `debug/` | 定位早期启动问题的探针代码 |
| `stub/` | 小型启动入口、复位实验代码 |
| `tools/` | 主线移植、调制解调器适配等辅助工具 |

`upstream/modules/` 中的主要模块：

| 模块目录 | 对应功能 |
|---|---|
| `sprd_modem/` | 调制解调器、核间通信、数据通道和相关驱动 |
| `wcn_bsp/` | Wi-Fi / 蓝牙连接子系统的底层支持 |
| `sprd_wlan_combo/` | Wi-Fi 驱动 |
| `sprdbt_tty/` | 蓝牙串口通道 |
| `mali/` | Mali GPU 驱动 |
| `sprd_dvfs/` | 动态电压和频率调节支持 |
| `mu300_thermal/` | 温控相关支持 |
| `sprd_pmic_wdt/` | 电源管理芯片的看门狗 |
| `ums9620_lpintc/` | 低功耗中断控制器 |
| `zte_sar_stk/` | SAR 传感器相关支持 |

## 6. `android/` 与 `android-vendor/`：两个不同用途

`android/` 放的是 Android 侧的切换工具。当前保留的 `android/magisk/mu300-linux-switch/` 是 Magisk 切换模块：`system/bin/` 提供命令入口，其他脚本处理模块安装、按钮操作和启动槽位切换。`android/magisk/build.sh` 打包切换模块。

当前副本的 `android/magisk/installer/` 只有 `module.prop.in` 模板。旧说明中提到的完整 Magisk 安装器与发布打包链，并没有全部保留在这个裁剪副本中；不能仅凭旧文档判断它仍可完整构建。

`android-vendor/` 放的是提取工具和输入清单：

| 内容 | 用途 |
|---|---|
| `extract-subset.sh`、`extract_subset.py`、`pull_closure.py` | 提取 Linux 运行调制解调器等功能所需的厂商程序及依赖 |
| `extract-gpu-subset.sh`、`gpu-files.txt` | GPU 用户态依赖的提取和文件清单 |
| `subset-files.txt` | 基础厂商文件清单 |
| `ueventd/` | 生成设备节点权限所依据的原始规则 |
| `gen-ueventd-perms.py`、`ueventd-perms.sh` | 生成和应用 Linux 下需要的设备权限 |

这两个目录都不是 Android 完整系统源码。真正提取出来的厂商文件通常在安装工作目录的 `android-subset/`、`android-gpu-subset/`、`firmware/` 中。

## 7. `tools/`：电脑辅助脚本与设备小工具

顶层脚本主要处理安装问答、ADB/SSH 连接、存储布局、备份、版本选择、签名校验、发布包和翻译等。`luci-dev.py` 是最近加入的本地界面开发代理，根目录 `make dev` 调用它。

| 子目录 | 用途 |
|---|---|
| `dev/` | Make 命令使用的 SSH、前后端文件部署和设备调试工具 |
| `adbd/` | 构建设备 Linux 下的 ADB 服务程序 |
| `bt-init/` | 初始化蓝牙芯片的厂商参数 |
| `carry/` | 系统升级时扫描和迁移用户文件，保留权限、扩展属性等信息 |
| `display/` | 显示输出、桌面和 USB-PD 的实验脚本与服务 |
| `e2fsck/` | 构建早期启动时可用的 ext 文件系统检查工具 |
| `f2fs-online/` | Android userdata 的 F2FS 在线缩容相关工具 |
| `gpu/` | GPU/OpenCL 探测和压力测试工具 |
| `keys/` | 读取设备实体按键事件的程序；这里的 keys 指按键，不是密钥 |
| `logdw/` | 接收 Android 厂商程序日志的小程序，使其能在 Linux 环境记录日志 |
| `mu300-peek/` | 读取或修改物理内存、硬件寄存器的底层调试模块 |
| `pwcheck/` | 安装和更新流程中的密码哈希核对工具 |
| `vbc-profile/` | 从厂商 XML 参数生成音频 DSP 配置文件 |
| `__pycache__/` | Python 自动生成的缓存，不是源码 |

这些工具并非全部已经安装或启用。比如有 GPU 测试工具源码，不代表当前发布包一定包含其编译后的程序。

## 8. 安装界面、翻译、文档和测试

| 目录 | 用途 |
|---|---|
| `gui/mu300gui/` | 图形安装器的窗口、状态模型、控制器、安装进程、设备连接、主题、翻译及日志脱敏等模块 |
| `i18n/` | 安装器的 `<语言>.tsv` 翻译；与 LuCI 的 `po/`、Cockpit 的 `panel/po/` 分开 |
| `docs/images/gui/` | 图形安装器的说明图片 |
| `docs/images/installer/` | 命令行安装器的示例输出、图片和渲染脚本 |
| `docs/reference/` | 硬件研究参考资料，例如 Android 网络加速状态记录 |
| `tests/fixtures/` | 固定测试输入、模拟 VPN 命令、协议输出和上游组件样本 |
| `tests/data/` | 测试用的模块等数据快照 |
| `tests/update-experiment/` | 系统升级、文件迁移实验的容器、场景和执行脚本 |
| `tests/update-experiment/results/` | 已保存的升级实验结果；不是当前设备状态的实时证明 |
| `tests/.claude/` | 测试目录下的本地开发工具配置，不属于测试数据或设备程序 |

常用说明：安装和使用看根目录 `README.md`；自行构建看 [BUILD.md](BUILD.md)；发布流程看 [RELEASING.md](RELEASING.md)；其他发行版看 [DISTROS.md](DISTROS.md)；硬件与问题排查的历史记录看 [FINDINGS.md](FINDINGS.md)。裁剪副本中的旧文档可能仍引用已经移除的文件，应以实际存在的源码为准。

## 9. `out/`、`work/`、`backup/`：本机资料和产物

| 目录 | 当前用途 |
|---|---|
| `out/` | 内核 `Image`、模块索引和模块文件；有这些文件不等于已经重新编译或部署 |
| `out/modules/` | 基础机型使用的内核模块 |
| `out/modules-u30air/` | U30 Air 机型相关模块 |
| `work/offline-release/` | 下载的系统、内核发布包，以及来源、签名和校验记录 |
| `work/offline-vpn/` | 独立 VPN 项目的离线模块包与相关资料，不是主项目的正式 VPN 源码目录 |
| `work/install-f50/` | 这台 F50 的安装工作目录：厂商文件、分区读取结果、设备专用启动镜像、安装日志等 |
| `work/install-f50/android-subset/`、`android-gpu-subset/` | 从设备提取的厂商程序和 GPU 用户态依赖 |
| `work/install-f50/dumps/` | 这次安装读取的设备分区或启动信息 |
| `work/install-f50/firmware/` | 这次安装使用的设备固件 |
| `work/install-f50/release/` | 安装器实际读取的发布包和解包结果 |
| `work/deploy/` | 本机部署参数、日志、首次启动记录、登录材料和恢复材料；其中可能含敏感信息 |
| `work/deploy/boot-readiness/`、`root-recovery/` | 启动链准备和恢复相关资料 |
| `work/deploy/vpn/` | 本机 VPN 配置过程的记录和材料，可能含认证或订阅信息 |
| `work/dumps/`、`work/firmware/` | 工作区中的预留或提取目录；当前也可能为空，具体以对应安装记录为准 |
| `backup/F50-full-2026-11-11/` | 这台设备的原始备份资料 |
| `backup/` 下的归档文件 | 例如本地源码裁剪前的归档，用于追溯或恢复 |

`work/` 和 `backup/` 已被 Git 忽略，但被忽略不等于可以直接清理。设备校准信息、原始启动链、登录材料和恢复文件可能只保存在这里。源码修改应落在上面的源码目录，不要只改解压出来的安装副本。

构建或安装后还可能出现顶层 `release/`、`dumps/`、`firmware/`、`android-subset/`、`android-gpu-subset/`，以及 `upstream/out/`、`upstream/out-*/`。它们分别用于发布包、设备提取资料和主线内核产物；是否存在取决于所执行的流程，不能把预期输出目录当成已完成构建。

## 10. 修改与生效范围

| 修改位置 | `make dev` 刷新后是否直接生效 | 后续怎么进入设备 |
|---|---|---|
| `openwrt/luci-app-mu300/htdocs/` 中 `/luci-static/` 下的文件 | 是，本地代理优先读取仓库文件 | 部署改动文件或重新打包插件/系统 |
| `openwrt/overlay/www/` 中 `/luci-static/` 下的文件 | 是；如果与插件同路径，插件文件优先 | 部署改动文件或重新构建系统包 |
| 插件 `root/`、翻译 `po/` | 否，相关运行内容仍来自设备 | 部署后端/菜单配置，或编译并部署翻译 |
| `rootfs/overlay/`、OpenWrt 各系统适配目录 | 否 | 部署相应命令和配置，或重新构建系统包 |
| `boot/`、`kernel/`、`upstream/` | 否 | 根据改动构建并更新配套的启动镜像、内核或模块 |
| `gui/`、`i18n/`、电脑端工具 | 影响电脑端工具的后续运行 | 通常不需要部署到设备 |

本地开发代理仍使用真实设备的 LuCI 登录、接口和主题。它只替换本地存在的浏览器资源，不会启动另一套 OpenWrt，也不会自动上传改动。网页终端使用独立端口，应从设备地址访问。启动和限制见 [LuCI 本地开发说明](../openwrt/luci-app-mu300/README.md#local-interface-development)。
