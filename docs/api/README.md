# MU300 API / Swagger 文档

提取日期：2026-11-12。依据当前本地源码，不代表设备已部署版本。提取时仅生成文档，未运行测试、构建、OpenAPI 校验、浏览器检查或设备接口请求。之后在同一天对一台运行中的设备做过一次只读核对，结果见[设备核对记录](#设备核对记录2026-11-12)。


> **2026-11-12 精简**：VPN（`mu300vpn`）、网页终端（`mu300term`/ttyd）、设置备份、短信转发、语言包、软件包恢复、LED、电源、TTL、TCP、USSD、VoLTE、zram、Docker、内核与双系统切换、CPU 核心/低延迟/电压等接口已从代码中删除，本文与 `openapi.json` 已同步移除，现为 32 个 RPC 方法、49 个 CLI 方法。下文“设备核对记录”保留的是删减前的核对结果。之后 OpenWrt 上的接口已由 Rust 守护进程 `f50d` 取代：ubus 对象 `f50.*`，经 uhttpd 的 `/ubus` 用 rpcd 会话调用，方法表可用 `f50.meta describe` 取得，见 [f50d/README.md](../../f50d/README.md)。本文描述的是旧的 shell 接口。

## 文件与范围

| 文件 | 内容 |
|---|---|
| [openapi.json](openapi.json) | OpenAPI 3.1.0：32 个自定义 LuCI RPC 方法、共享传输入口、1 个文件上传接口；另含 49 个 ADB CLI 方法及设备识别探针的 schema |
| [stock-goform.openapi.json](stock-goform.openapi.json) | 原厂 Android Goform：仓库工具使用的 2 个 HTTP 路径，覆盖 3 个读取命令组合、3 个写入动作 |

将 JSON 文件导入支持 OpenAPI 3.1 的 Swagger Editor / Swagger UI / API 管理工具即可阅读。各 RPC 方法作为独立操作展示；共享入口只是别名，不重复计算为新的业务方法。

这份清单覆盖仓库公开的 MU300 应用接口。OpenWrt 通用 `uci`、`file`、`session`、Cockpit、ttyd 等平台 API 由外部组件提供，没有把它们的全部能力当作本项目接口。`scripts/dev/luci-dev.py` 是透传代理，没有另一套业务 API。内核 UAPI、内部 Unix socket 协议、命令行工具的内部子命令，以及 GitHub 等外部服务请求不属于本清单。`stock/` 没有完整原厂 Web 服务端源码，因此原厂文件仅为已使用子集。

有三类接口不是 RPC 方法，但属于本项目自己定义的约定：面板的 UCI 配置、WPA2 提示文件和蜂窝拨号协议的配置项。它们收录在[配套约定](#配套约定非-rpc-方法)一节，并以 `x-companion-contracts` 扩展写入 `openapi.json`。

## LuCI 调用与认证

默认示例服务器为 `http://192.168.77.1`，可在 Swagger 的 Servers 中修改；已有开发代理使用 `http://127.0.0.1:27547`。本文没有启动代理。

实际传输格式如下，`params` 四项依次为 sessionid、对象、方法、参数对象：

```http
POST /cgi-bin/luci/admin/ubus/mu300dash.sysinfo
Content-Type: application/json
```

```json
{"jsonrpc":"2.0","id":1,"method":"call","params":["REPLACE_WITH_LUCI_SESSION_ID","mu300dash","sysinfo",{}]}
```

使用已登录 LuCI 会话中的 `L.env.sessionid`（或页面使用的 `rpc.getSessionID()`），替换占位符。API 的 schema 示例用 32 个 0 占位，它不是有效的已授权会话。认证在请求体中，不是 Bearer Token。不要往参数对象手动添加 `ubus_rpc_session`，LuCI 分发器会添加。

`/cgi-bin/luci/admin/ubus/<object>.<method>` 是 LuCI 通配路由支持的后缀形式，客户端批量调用会使用这个后缀；实际方法仍由 JSON 请求体决定。也可直接 POST 到 `/cgi-bin/luci/admin/ubus`。JSON 对象请求返回对象；数组请求返回数组。本文的示例不依赖 uhttpd 的 `/ubus` 路径；核对的那台设备上 `uhttpd.main.ubus_prefix` 为 `/ubus`，即该路径是启用的，但本文没有为它单独建模。

响应有三个层次，不能只看 HTTP 状态：

```json
{"jsonrpc":"2.0","id":1,"result":[0,{"ok":1}]}
```

- `error.code/message`：JSON-RPC 请求或会话权限错误。
- `result[0]`：ubus 状态，`0` 表示调用完成，非零表示 ubus 失败。
- `result[1]`：业务回复；`ok:0` 或 `error` 仍可能表示失败。部分读取直接返回数据，没有 `ok`；终端失败用 `code`。未知值与不存在的能力保留 `null` 或源字段，不填成零。

每个操作都含参数 schema、调用示例、业务响应 schema、ACL 及 `x-source`（源码路径、起始行与 SHA-256）。嵌套数据若由底层工具原样返回，则保留开放 JSON schema，并在说明中标明；不将未确认字段伪装成固定契约。条件必填关系与动态硬件限制在方法说明中列出，schema 不代替设备自身检查。

## 两套接口容易混淆的差异

| 项目 | LuCI RPC | ADB `mu300-api` |
|---|---|---|
| 开关参数 | 声明为字符串 `"0"/"1"` | `flag()` 接受 JSON 布尔值或数字 `0/1` |
| 普通参数大小上限 | mu300dash 16 KiB | 64 KiB |

大小上限是后端读入的 JSON 字节数，不是字符串的字符数；HTTP 服务器还可能有自己的限制。`x-maxBytes` 保留源码中的 UTF-8 字节限制。

## 异步任务

| 提交操作 | 查询方式 |
|---|---|
| `mu300dash.at` 的扫描 | `mu300dash.at_job` |
| `mu300dash.sms_send` | `mu300dash.sms_job` |
| `mu300dash.lock_set` 返回任务 ID | `mu300dash.lock_job {id}` |
| `mu300dash.update_act` | `mu300dash.update_job` |
| CLI 返回 `job` 的方法 | `mu300-api job.status`，用上次 `next` 作为下次 `from` |

`started` 或 `job` 只代表已提交；以任务最终状态、退出码及错误字段为准。各对象的后台任务独立，不能交叉使用任务 ID。修改网络、ADB 或重启可能在回复后断开连接，这是相应源码的行为。

## 文件上传

`POST /cgi-bin/cgi-upload`，`multipart/form-data` 字段为 `sessionid`、`filename`、`filedata`。应用 ACL 授权的目标如下：

| filename | 消费接口 |
|---|---|
| `/mnt/mu300-disk/.mu300-upload/update.zip` | 先 `update_act(upload_begin,size)`，按返回 path 上传，再 `upload_done`，最后确认应用 |

## ADB CLI 调用

CLI 方法不提供 HTTP 路由，因此以 OpenAPI 扩展 `x-cli-methods` 收录，输入和输出模型都在 `components.schemas` 中，以 `cli.<method>.args/result/response` 命名，可在 Swagger 的 Schemas 中查看。以下只展示调用方式：

```sh
printf '%s' '{}' | adb shell /opt/mu300/bin/mu300-api hello
printf '%s' '{"from":0}' | adb shell /opt/mu300/bin/mu300-api job.status
adb shell /opt/mu300/bin/mu300-device --json
```

需要 root；方法名是唯一命令参数，请求是 stdin 中的一个平面 JSON 对象，回复为 stdout 的一行 JSON。成功 `ok=1` / 退出码 0；拒绝或失败 `ok=0,error,detail?` / 退出码 1；用法错误退出码 2。`hello.methods/features` 用于了解运行设备当前能力。仅记录源码，并未执行上述命令。

CLI 不接受任意未知字段。部分 setter 至少需要一项有效修改；通用 `word/int/flag/text` 将 null 视为未提供，直接使用 `fget` 的字段可能拒绝 null，以每个方法源码为准。

## 配套约定（非 RPC 方法）

以下内容依据源码整理。标注“设备值”的是 2026-11-12 在一台设备上读到的实际配置。

### UCI 配置 `unisoc_modem`

文件为 `/etc/config/unisoc_modem`。ACL 组 `luci-app-mu300` 对它有读写权限，所以登录面板的会话可以通过 LuCI 通用的 `uci` 接口修改它；“适配器设置”页面（`view/mu300/settings.js`）就是这样做的。

`main` 节（类型 `core`）：

| 选项 | 默认值 | 含义 | 后端校验 |
|---|---|---|---|
| `at_backend` | `auto` | AT 后端：`auto`、`mu300`、`atinout`、`custom` | — |
| `at_port` | `/dev/stty_nr1` | `atinout` 后端使用的串口 | 只接受 `/dev/stty*` 或 `/dev/tty*`，否则用默认值 |
| `at_command` | 空 | `custom` 后端的适配器程序；参数依次为超时秒数、完整 AT 命令 | 只接受 `/usr/libexec/unisoc-modem/platform/` 下的程序 |
| `sms_command` | 空 | 短信适配器，需实现 `list/show/send/delete/sync`；留空自动查找 `mu300-sms` | 只接受 `/opt/mu300/bin/mu300-sms`、`/usr/bin/mu300-sms` 或 platform 目录下的程序 |
| `sms_pool` | `/etc/mu300/sms-pool` | 短信池目录 | — |
| `data_interface` | `wan` | 蜂窝数据的逻辑接口 | 接口名规则 |
| `data_interface_v6` | `wan6` | 蜂窝 IPv6 的逻辑接口 | — |
| `data_device` | 空 | 蜂窝网络设备；留空由 netifd 检测 | — |
| `lan_device` | `br-lan` | LAN 桥 | 接口名规则 |
| `wifi_device` | `wlan0` | Wi-Fi 设备 | — |
| `usb_device` | `usb0` | USB 网卡设备 | — |
| `replay_timeout` | `90` | 等待 AT 就绪的最长秒数 | 页面限制为非负整数 |
| `home_refresh_interval` | `1.5` | 首页刷新间隔（秒），页面限制为 0.5–60 | 仅前端读取 |
| `state_dir` | `/etc/unisoc-modem/lock-state.d` | 锁定状态的持久目录 | 只接受 `/etc/unisoc-modem` 或 `/etc/mu300` 之下、不含 `.` 开头路径段的目录 |

接口名规则：最多 15 个字符，以字母或数字开头，只含字母、数字、`_`、`.`、`-`。不符合校验的值不会报错，后端会忽略它并改用内置默认值。“后端校验”一栏为“—”的选项，表示在共享校验库 `lib.sh` 中没有找到对应的校验，不代表使用它的脚本一定不做检查。

`usb` 节（类型 `device`）由 `mu300dash.usb_set` 经适配器 `device-usb` 写入，一般不需要直接修改：

| 选项 | 默认值 | 含义 |
|---|---|---|
| `role_auto` | `0` | USB 角色自动切换 |
| `net_mode` | `ncm` | USB 网络模式：`ncm`、`ecm`、`rndis` 或 `multi`（NCM + RNDIS） |
| `net_scope` | `permanent` | 网络模式的生效范围 |
| `net_auto` | `0` | USB 网络自动策略 |
| `role_command` | 空 | 自定义角色切换程序，以 `host` 或 `device` 为唯一参数调用，受 platform 目录限制 |

设备值：与上述默认值一致；`at_command`、`sms_command`、`data_device`、`role_command` 未设置，`/usr/libexec/unisoc-modem/platform/` 目录不存在。

### WPA2 提示文件

Wi-Fi 芯片不支持安全的 WPA3（SAE）时，`openwrt-wifi-wpa2-only` 在开机时把热点改为 WPA2-PSK，并留下 `/etc/mu300/wpa3-disabled`。状态概览页和面板首页据此显示提示。

- 读取：LuCI 通用的 `file.read`，由 ACL 组 `mu300-wpa2-notice` 的读权限授权。
- 关闭提示：`file.remove` 删除该文件，由同组的写权限授权。
- 文件内容是若干行 `key=value`，不含密码：

| 键 | 含义 |
|---|---|
| `state` | `migrated`（已改为 WPA2）或 `down`（切换失败，热点已关闭）。其他取值视为没有提示 |
| `time` | 发生时间 |
| `migrated` | 已改为 WPA2 的无线配置节 |
| `down` | 被关闭的无线配置节 |

这个 ACL 组在 `systems/openwrt/overlay/` 中，不属于 `luci-app-mu300` 插件，因此没有面板的普通 OpenWrt 镜像也有它。

设备值：文件不存在，即没有待处理的提示。

### 蜂窝拨号协议 `mu300cell` 的配置项

蜂窝数据是 `/etc/config/network` 中 `proto` 为 `mu300cell` 的接口（默认是 `wan`）。通过 LuCI 通用的 `uci` 接口或接口编辑页修改，ACL 由 LuCI 自带的网络模块授权。

| 选项 | 类型 | 含义 |
|---|---|---|
| `sim_slot` | `0` / `1` | SIM 来源：0 为外置卡，1 为内置卡。重启后生效。不支持内置卡的设备会忽略 `1` |
| `apn` | 字符串 | 外置卡 APN；留空则沿用 SIM 已定义的上下文 |
| `pdptype` | `IP` / `IPV4V6` | 外置卡 PDP 类型，默认 `IP` |
| `username`、`password` | 字符串 | APN 凭据，需同时设置 `apn`。最多 64 个可打印 ASCII 字符，不含引号和反斜杠 |
| `auth` | 空 / `pap` / `chap` / `both` / `none` | APN 认证方式；空表示自动 |
| `iccid` | 18–22 位字母数字 | 上述 APN、PDP 类型和凭据只用于这张卡；留空表示任意卡 |
| `apn_internal` | 字符串 | 内置卡 APN |
| `pdptype_internal` | `IPV4V6` / `IP` | 内置卡 PDP 类型，默认 `IPV4V6` |
| `roaming`、`roaming_internal` | `0` / `1` | 外置卡、内置卡的数据漫游；未设置视为允许。由 `mobile-data` 直接读取，约半分钟内生效 |
| `ipv6` | 空 / `extend` / `relay` | IPv6 取得方式。`relay` 由面板版首次启动时设置，只对双栈上下文生效 |
| `peerdns` | 布尔 | 是否使用运营商下发的 DNS，默认开启 |
| `dns` | IP 地址列表 | 自定义 DNS，`peerdns` 关闭时使用 |

`/etc/config/network` 可能含有 APN 密码，文件权限为仅 root 可读写。

设备值：`network.wan` 的 `proto` 为 `mu300cell`，`ipv6` 为 `relay`，`pdptype` 为 `IPV4V6`。APN、凭据和 ICCID 没有读取。

## 设备核对记录（2026-11-12）

通过 adb 以 root 连接一台运行 OpenWrt 25.12.6 面板版（镜像 `v2026.11.02.1`）的 F50，只执行读取命令：`ubus -v list`、`mu300-api --methods`、`sha256sum`、`uci show`、`ls`。没有调用任何 RPC 业务方法或写接口，没有重启设备或服务。

| 核对项 | 结果 |
|---|---|
| LuCI RPC 方法 | 设备上 `mu300dash`、`mu300vpn`、`mu300term`、`mu300rates` 共 82 个方法，名称与本文一致 |
| RPC 参数名 | 82 个方法的参数名全部一致 |
| ADB CLI 方法 | `mu300-api --methods` 列出 102 个，与本文一致 |
| 源文件 | `mu300dash`、`mu300vpn`、`mu300term`、`mu300rates.uc`、`mu300-api`、`mu300-device` 和 ACL 文件的 SHA-256 与仓库、与 `x-source` 记录的值相同 |
| 前端调用 | 前端页面调用的 `mu300*` 方法全部在本文中 |
| `/ubus` 路径 | 设备上已启用（`ubus_prefix='/ubus'`） |

一处差异：`mu300dash.backup_export.with_password`、`backup_import.dry_run` 和 `backup_import.force` 在本文中是 Boolean，设备的 `ubus -v list` 报告为 String。源码在方法声明里写的是字符串 `"Boolean"`，而 rpcd 按声明值本身的 JSON 类型推断参数类型，所以得到 String。面板前端对这三个参数发送的是 JSON 布尔值 `true`，本文按此保留 Boolean。传布尔值是否会被 rpcd 拒绝没有实测，因为这几个是写接口。

没有核对的内容：各方法的返回值结构、原厂 Goform 接口（设备当时运行的是 Linux）、Ubuntu 和 Cockpit 上的行为。

## LuCI RPC 完整清单

参数后 `*` 表示无条件必填；其他参数可能条件必填，具体见 OpenAPI 方法说明。

| 对象 / 方法 | 用途 | 参数 | ACL |
|---|---|---|---|
| `mu300dash.sysinfo` | 读取本地系统快照（不访问 AT） | — | `luci-app-mu300:read` |
| `mu300dash.status` | 读取首页状态与缓存信号 | — | `luci-app-mu300:read` |
| `mu300dash.signal` | 读取实时信号 | — | `luci-app-mu300:read` |
| `mu300dash.act` | 执行设备快捷操作 | `op*`, `arg` | `luci-app-mu300:write` |
| `mu300dash.at` | 发送带确认规则的 AT 命令 | `cmd*`, `confirm` | `luci-app-mu300:write` |
| `mu300dash.at_job` | 读取后台 AT 扫描任务 | — | `luci-app-mu300:write` |
| `mu300dash.at_history` | 读取 AT 历史 | — | `luci-app-mu300:write` |
| `mu300dash.lock_get` | 读取网络锁定与缓存状态 | `fresh` | `luci-app-mu300:read` |
| `mu300dash.lock_set` | 设置网络制式、频段或小区锁定 | `kind*`, `val` | `luci-app-mu300:write` |
| `mu300dash.lock_job` | 读取指定锁定任务状态 | `id*` | `luci-app-mu300:read` |
| `mu300dash.sms_list` | 分页读取本地短信池 | `page` | `luci-app-mu300:write` |
| `mu300dash.sms_show` | 读取短信文本 | `id*` | `luci-app-mu300:write` |
| `mu300dash.sms_send` | 提交后台短信发送 | `num*`, `text*` | `luci-app-mu300:write` |
| `mu300dash.sms_job` | 读取短信发送任务 | — | `luci-app-mu300:write` |
| `mu300dash.sms_delete` | 删除短信 | `id*`, `sim` | `luci-app-mu300:write` |
| `mu300dash.sms_sync` | 从 SIM 同步短信池 | — | `luci-app-mu300:write` |
| `mu300dash.usb_get` | 读取 USB 配置 | — | `luci-app-mu300:read` |
| `mu300dash.usb_set` | 修改 USB 角色或网络模式 | `kind*`, `value*`, `scope`, `auto*` | `luci-app-mu300:write` |
| `mu300dash.usb_net_list` | 列出外接 USB 网卡 | — | `luci-app-mu300:read` |
| `mu300dash.usb_net_add` | 将 USB 网卡加入 LAN 桥 | `iface*` | `luci-app-mu300:write` |
| `mu300dash.sim_get` | 读取 SIM 选择状态 | — | `luci-app-mu300:read` |
| `mu300dash.sim_set` | 设置默认卡、热切换或 V50 实体卡 | `op*`, `value*` | `luci-app-mu300:write` |
| `mu300dash.simcard_get` | 读取 VoLTE、VoNR 及无线电设置 | — | `luci-app-mu300:read` |
| `mu300dash.simcard_set` | 修改单卡语音或无线电设置 | `kind*`, `slot*`, `state*` | `luci-app-mu300:write` |
| `mu300dash.traffic_get` | 读取流量用量及配额 | — | `luci-app-mu300:read` |
| `mu300dash.traffic_set` | 修改流量统计与配额 | `op*`, `value` | `luci-app-mu300:write` |
| `mu300dash.cpu_get` | 读取 CPU 档位及频率策略 | — | `luci-app-mu300:read` |
| `mu300dash.cpu_set` | 设置 CPU 性能档位 | `profile*` | `luci-app-mu300:write` |
| `mu300dash.update_get` | 读取更新、上传及任务状态 | `sizes` | `luci-app-mu300:read` |
| `mu300dash.update_job` | 读取系统更新任务 | — | `luci-app-mu300:read` |
| `mu300dash.update_act` | 检查、应用、回滚或管理系统更新 | `op*`, `tag`, `systems`, `kernel`, `sha256`, `size`, `flags`, `confirm`, `url` | `luci-app-mu300:write` |
| `mu300rates.netdev` | 读取网络接口字节计数器 | — | `luci-app-mu300:read` |

## ADB CLI 完整清单

以下 49 个方法逐一对应源码 `METHODS` 清单；内部透传字段 `ubus_rpc_session` 不在表中重复展示。

| 方法 | 用途 | 参数 |
|---|---|---|
| `hello` | 读取 API 能力与设备信息 | — |
| `overview` | 读取设备概况 | — |
| `wifi.get` | 读取热点设置及连接客户端 | — |
| `wifi.set` | 修改热点设置 | `ssid`, `password`, `band`, `channel`, `hidden` |
| `wifi.power` | 开启或关闭热点 | `state*` |
| `wificlient.status` | 读取 Wi-Fi 客户端状态 | — |
| `wificlient.scan` | 扫描附近 Wi-Fi | — |
| `wificlient.join` | 连接 Wi-Fi | `ssid*`, `password`, `open` |
| `wificlient.leave` | 断开或忘记 Wi-Fi | `forget` |
| `data.get` | 读取移动数据与 APN | — |
| `data.set` | 修改 APN、凭据及漫游开关 | `apn`, `pdp`, `auth`, `user`, `password`, `clear_credentials`, `roaming` |
| `data.power` | 开启或关闭移动数据 | `state*` |
| `sim.get` | 读取 SIM 选择状态 | — |
| `sim.select` | 设置默认 SIM | `slot*` |
| `sim.card` | 选择 V50 下次启动的实体卡 | `card*` |
| `sim.state` | 读取当前 SIM 锁定状态 | `fresh` |
| `radio.get` | 读取 SIM 无线电状态 | — |
| `radio.set` | 开启或关闭 SIM 无线电 | `slot*`, `state*` |
| `lock.get` | 读取网络制式、频段及小区锁定 | `fresh` |
| `lock.set` | 修改网络锁定 | `kind*`, `val` |
| `sms.list` | 列出短信 | `page` |
| `sms.read` | 读取短信内容 | `id*` |
| `sms.send` | 发送短信 | `number*`, `text*` |
| `sms.job` | 读取短信发送任务 | — |
| `sms.delete` | 删除短信 | `id*`, `sim` |
| `lan.get` | 读取管理网络设置 | — |
| `lan.set` | 修改管理 IP | `ip*` |
| `usb.role` | 切换 USB 主机或设备角色 | `role*` |
| `usb.net` | 读取或设置 USB 网络模式 | `mode`, `scope`, `name` |
| `system.get` | 读取系统设置 | — |
| `system.hostname` | 修改主机名 | `name*` |
| `system.timezone` | 修改时区 | `zone*` |
| `system.password` | 修改系统登录密码 | `password*` |
| `system.ssh` | 开启或关闭 SSH | `state*` |
| `system.reboot` | 重启设备 | — |
| `update.check` | 检查系统更新 | — |
| `update.apply` | 应用系统更新 | — |
| `logs` | 读取系统日志 | `lines` |
| `at.send` | 发送 AT 命令 | `command*`, `timeout` |
| `at.history` | 读取 AT 历史 | — |
| `modem.restart` | 重启移动数据拨号 | — |
| `traffic.get` | 读取流量统计及配额 | — |
| `traffic.set` | 修改流量配额及统计设置 | `key*`, `value*` |
| `cpu.get` | 读取 CPU 与电压状态 | — |
| `cpu.profile` | 设置 CPU 性能档位 | `name*` |
| `update.status` | 读取系统更新状态及来源 | — |
| `job.status` | 读取后台任务及增量日志 | `from` |
| `job.cancel` | 取消可中断的后台任务 | — |
| `job.clear` | 清除已结束的后台任务 | — |

## 原厂 Goform 子集

另导入 [stock-goform.openapi.json](stock-goform.openapi.json)。主机地址可替换，端口为工具源码中的 8080。该文件描述的是客户端实际使用的字段，未将其推断为原厂服务端的全部可接受输入。

| HTTP 路径 | 选择参数 | 用途 |
|---|---|---|
| GET `/goform/goform_get_cmd_process` | cmd=LD | 获取登录挑战 |
| GET 同上 | cmd=Language,cr_version,wa_inner_version；multi_data=1 | 获取版本字段 |
| GET 同上 | cmd=RD | 获取写入挑战 |
| POST `/goform/goform_set_cmd_process` | goformId=LOGIN | 获取会话 Cookie |
| POST 同上 | goformId=USB_PORT_SETTING | usb_port_switch=0/1 |
| POST 同上 | goformId=REBOOT_DEVICE | 重启 |

所有调用携带 `isTest=false`。GET 还带 Unix 毫秒 `_`；POST 为 `application/x-www-form-urlencoded; charset=UTF-8`。设 `H(s)=SHA256(s).upper()`，登录密码字段为 `H(H(password)+LD)`，写入 AD 为 `H(H(wa_inner_version+cr_version)+RD)`。LOGIN 返回的首个 Set-Cookie 名值对用于后续 Cookie；名称不在本仓库固定。脚本设置 Referer 为 `http://<host>:8080/index.html`。

## 源码与标准依据

- [mu300-api](../../systems/base/overlay/opt/mu300/bin/mu300-api)：49 个方法、字段检查及返回值；[既有 ADB 说明](../guides/ADB-INTERFACE.md)仅作辅助，差异以代码为准。
- [mu300dash](../../systems/openwrt/luci-app-mu300/root/usr/libexec/rpcd/mu300dash)：RPC 方法、分支及参数类型。
- [mu300rates](../../systems/openwrt/luci-app-mu300/root/usr/share/rpcd/ucode/mu300rates.uc)、[ACL](../../systems/openwrt/luci-app-mu300/root/usr/share/rpcd/acl.d/luci-app-mu300.json)：流量读取与方法权限。
- [common.js](../../systems/openwrt/luci-app-mu300/htdocs/luci-static/resources/mu300/common.js)、[上传页面](../../systems/openwrt/luci-app-mu300/htdocs/luci-static/resources/view/mu300/update.js)：调用格式及上传参数。
- [原厂调用脚本](../../scripts/device/f50-enable-adb-macos.sh)：Goform 路径、查询、表单、Cookie 与摘要算法。
- [OpenAPI 3.1.0](https://spec.openapis.org/oas/v3.1.0.html)：使用 JSON Schema 的数组位置模型表达 JSON-RPC params，而不将 RPC 改写成 REST。
- LuCI 上游 [rpc.js](https://github.com/openwrt/luci/blob/master/modules/luci-base/htdocs/luci-static/resources/rpc.js)、[通配路由](https://github.com/openwrt/luci/blob/master/modules/luci-base/root/usr/share/luci/menu.d/luci-base.json)、[JSON-RPC 分发器](https://github.com/openwrt/luci/blob/master/modules/luci-base/ucode/controller/admin/index.uc)：共享入口、方法后缀、会话位置与回复封装的参考；它们不证明设备安装了该上游版本。
