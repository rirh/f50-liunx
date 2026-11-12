# OpenWrt × MU300 适配层评审与改造方案（2026-11-12）

范围：设备端控制链路的接口层，不含页面。
- rpcd：`luci-app-mu300/root/usr/libexec/rpcd/mu300dash`、`mu300term`、`mu300rates.uc`、ACL
- 适配器：`unisoc-modem/*` 和 `lib.sh`
- `/opt/mu300/bin`：`mu300-api`、`mu300-at`、`mu300-atd`

方法：只读代码评审，没有在设备上运行。标“已核对”的条目已对照源码逐行确认。

## 1. 现状的核心问题

1. **两套 API 互相调用。**
   - 浏览器的链路：`mu300dash` → `unisoc-modem` 适配器 → `/opt/mu300/bin`。
   - adb/Cockpit 的链路：`mu300-api` → `/opt/mu300/bin`。
   - 在 openwrt-luci 上，`mu300-api` 的 `lock.*` 和 `sms.*` 反过来调用 `mu300dash`；`mu300dash` 的 `backup_*` 又调用 `mu300-api`。
   - 同一功能在两边的校验和返回结构不同，例如 `sms.read` 在 luci 上返回 `{data}`，在 Ubuntu 上返回 `{text}`。
2. **返回结构没有统一约定，是自己做前端的最大障碍。**
   - 错误有三种写法：`{ok:0,error}`、`{error}`、`{busy:1,error}`。
   - 任务（job）有四种模型。
   - `job` 有时是 `1`，有时是 id 字符串。
   - 错误信息是英文句子，没有机器可读的错误码。
3. **每次调用都要启动大量进程。** rpcd 用 shell 脚本处理请求：
   - 每次调用的固定开销约 20 个进程；`update_act` 一次调用超过 60 个。
   - 首页每 1.5 s 调一次 `status`，每次都完整执行 500 行的 `dashboard-info`。
   - 每条 AT 命令要启动 3 次 `uci`；`mu300-at` 等待回复时每 10 ms 启动一次 `sleep`，约每秒 200 个进程。

## 2. 需要修的问题

### 高

| 位置 | 问题 | 状态 |
|---|---|---|
| `mu300dash:1311-1315` | 每个方法的完整返回都追加到 `last-outputs.log`，包括短信正文、AT 回复（IMSI/ICCID）、转发配置，与文件头的隐私说明矛盾，每次调用还多启动 2 个进程 | 已核对 |
| `mu300-api:1371` `at.send` | 缺少 dash 的保护：`AT+SPENGMD=0,1` 会让 AT 口卡死到重启；没有拦截切 SIM 的命令和 `;` 串联，也没有确认步骤。adb/Cockpit 一次调用就能让调制解调器通道卡死 | 已核对 |
| `dashboard-info:297-305` | 每个 Wi-Fi 客户端调用一次 `iw station get`（超时 2 s，最多 16 个），最坏情况超过 rpcd 的 30 s 上限 | |

### 中

- `lock:293`：缓存兜底用了 `exit 0` 而不是 `return`，`lock get fresh` 走到这里时没有输出（已核对）。
- `mu300dash:414-420`：扫描任务先 `rm -rf` 再 `mkdir`，不是原子操作，两个并发请求会各启动一次扫描（已核对）。
- `mu300dash:214-243`：`signal` 在请求里直接读 AT，最长 14 s；锁频页每 2 s 轮询一次；临时文件可能残留。
- `mu300dash:421,440`：AT 历史保存了 `CPIN`、`CPWD` 命令里的密码明文。`mu300-api` 已经做了脱敏，dash 没有。
- `mu300dash`：`$HIST.t`、`$OPS.t`、`$LASTLOG.t` 在并发调用之间共用同一个文件名。
- `mu300-at:128-137`：接管过期锁时有竞争，两个客户端可能同时持有锁。应改用 `flock`。
- `mu300-api:288-298`：`signal_json` 在请求里同步读 3 条 AT，最长可等几分钟。
- `dashboard-info:82`、`cell:76`：`mu300-sim active` 失败时整个快照变成空，而且没有超时。
- `lock:245-258`：8 次串行 AT 读，没有总的截止时间。
- 适配器约定不一致：出错时的退出码有 0、1、2 三种；`lock` 把一部分错误写到 stderr 而不是 JSON；`power get` 的返回没有 `ok` 字段。
- ACL 给了 `unisoc_modem.*` 的 uci 写权限，前端可以改 `at_port`、`sms_command` 等。

### 低

- `led:14`：`mu300-led` 返回 `{}` 时会输出 `{"ok":1,}`，不是合法 JSON（已核对）。
- `traffic` 直接透传底层输出，不带 `ok`；`lock:436,448` 有字段没经过 `json_str`。
- `lib.sh` 的 `json_str` 不校验 UTF-8，非法字节会产生非法 JSON。
- `fast.lck` 从来没有被创建，`MU300_DASH_LOCK_DIR` 也没有被读取，都是死代码。
- 覆盖用的环境变量命名不统一：`MU300_*_CMD`、`*_BIN`、`UNISOC_AT_BIN`、`MU300_AT` 并存。
- `systems/base/overlay/usr/libexec/unisoc-modem/` 里的 `at`、`boot-replay`、`forward`、`lock`、`lib.sh` 与 luci 包中的文件逐字节相同。OpenWrt 用的是 luci 包里的那份，Ubuntu 用的是 base 里的那份，只有 `forward-feed` 是 base 独有的。

## 3. 适配层方案

目标：LuCI、自己的前端、adb 和 Cockpit 都调用同一个接口。

```
自己的前端 / LuCI ──HTTP──► uhttpd /ubus ──► rpcd 对象 "mu300"（薄转发层，ACL 分读/写组）
adb / Cockpit ───────────────────────────────┐        │
                                             ▼        ▼
                                    mu300-api（唯一的接口约定，方法名 noun.verb）
                                             │
                     /opt/mu300/lib/*.sh（AT 保护、任务、缓存、校验，两边共用）
                                             │
                     /opt/mu300/bin 设备命令 ──► mu300-atd ──► 调制解调器
```

返回约定（所有方法相同）：

```json
{"ok": true,  "data": { ... }}
{"ok": false, "code": "busy|bad-value|unsupported|timeout|...", "error": "可读说明"}
{"ok": true,  "data": {"job": "<id>"}}   // 超过约 15 s 的操作一律改成任务，用 job.status{id} 查询
```

读取类方法（status、signal、cell）只读缓存并附带 `age`，由后台采集进程负责刷新。这样任何前端轮询都不会增加调制解调器的负载。

### 分阶段实施

1. **修 bug，接口不变，可以单独上线。** 修复上面的高/中级问题；`last-outputs.log` 改为调试开关控制；`mu300-api at.send` 接入 AT 保护；`lock`/`led` 的 bug；扫描任务改用原子的 `mkdir` 占位；临时文件名加 `$$`；去掉 base 里的重复文件，Ubuntu 从 luci 包安装。
2. **抽出共用库。** 把 dash 里的 AT 保护、短信发送任务、锁频队列、信号缓存移到 `/opt/mu300/lib/`，`mu300-api` 和 `mu300dash` 都引用它。
3. **统一接口。** 补齐 `mu300-api` 缺的方法：`lock.job`、`at.job`、`sms.sync`、`status`/`signal`（读缓存）、`usb.net_*`、`lang.*`、`boot.get`、`ops.log`。所有方法使用上面的返回约定，然后新增 rpcd 转发层 `mu300`，同步更新 `docs/api/openapi.json`。
4. **迁移并清理。** `mu300dash` 保留为旧方法名到新方法的映射表，LuCI 页面逐个迁移，迁移完删除它。
5. **性能（可选）。** 把热路径（status、signal、netdev）改写成 ucode rpcd 插件，像 `mu300rates.uc` 那样不再为每次调用启动进程；`mu300-atd` 改为 unix socket，去掉轮询和应答文件。
