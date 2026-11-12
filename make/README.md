# 开发、部署与设备命令

从仓库根目录执行 `make help` 查看命令。直接运行 `make` 仍然等同于 `make dev`。

## 文件分工

| 文件 | 用途 |
|---|---|
| 根目录 `Makefile` | 统一入口，只加载下列文件 |
| `config.mk` | Python、设备地址、端口及 SSH 参数 |
| `dev.mk` | 本地开发代理、浏览器入口 |
| `deploy.mk` | 前端、插件后端和组合部署 |
| `device.mk` | SSH、状态、缓存、服务重启和设备重启 |
| `debug.mk` | 系统日志、状态 JSON 和 RPC 方法查看 |
| `help.mk` | 命令帮助 |
| `local.mk.example` | 本机配置示例，可复制成 `local.mk`；后者已被 Git 忽略 |

本地代理由 `tools/luci-dev.py` 实现；SSH 和部署操作由 `tools/dev/device.py` 实现。
只依赖 Python 3.8+ 和电脑已有的 `ssh`，设备端使用 OpenWrt 自带的工具。

## 本机配置

```sh
cp make/local.mk.example make/local.mk
```

| 配置 | 默认值与作用 |
|---|---|
| `PYTHON` | `python3` |
| `DEVICE` | `http://192.168.77.1`，本地代理转发的 HTTP 地址 |
| `PORT` | `27547`，已登记的本地开发端口 |
| `SSH_HOST` | 留空时使用 `DEVICE` 中的主机地址 |
| `SSH_USER` | `root`，部署和设备管理需要此权限 |
| `SSH_PORT` | `22`，与网页 HTTP 端口独立 |
| `SSH_IDENTITY` | 可选，SSH 私钥文件路径 |
| `SSH_KNOWN_HOSTS` | 可选，指定已有的 known_hosts 文件 |
| `SSH_HOST_KEY_ALIAS` | 可选，配合 known_hosts 中记录的主机别名 |
| `RPC_OBJECT` | `mu300dash`，`make rpc` 要查看的接口对象 |

SSH 沿用 OpenSSH 的主机密钥检查与认证；可以使用密钥、ssh-agent 或终端密码提示。脚本不读取安装目录中的登录密码，也不保存密码。私钥和 known_hosts 的相对路径以仓库根目录为基准。

也可以单次覆盖，例如 `make status DEVICE=http://192.168.77.1 SSH_PORT=22`。

## 本地开发

```sh
make dev
```

打开 `http://127.0.0.1:27547/`。编辑源码、保存、刷新即可；Ctrl+C 停止代理。
另一个终端执行 `make open` 可以打开本地页面，但不会启动代理。

此流程仍使用现有 LuCI 和真实设备后端，未接入新的独立前端。

## 部署

| 命令 | 覆盖范围 |
|---|---|
| `make deploy` / `make deploy-ui` | 合并两处前端文件到设备 `/www/`，并应用 Aurora 顶栏定制；同路径时插件文件优先 |
| `make deploy-backend` | `openwrt/luci-app-mu300/root/usr/` 安装到设备 `/usr/`，包括 RPC、适配器、菜单和 ACL |
| `make deploy-all` | 以上两部分一起部署 |

前端文件来自 `openwrt/overlay/www/` 和 `openwrt/luci-app-mu300/htdocs/`。
部署同时安装并运行 `mu300-ui-theme`，直接移除 Aurora 主题模板中的顶栏主机名标签，
因此标准 LuCI 页面也生效，不依赖 MU300 页面是否加载公共 JS。
主题模板会和被覆盖的文件一起备份。主题后续升级可能恢复此标签，重新执行 `make deploy` 可再次应用。

部署前会接收完整文件包，并将要覆盖的现有文件备份到设备
`/root/mu300-dev-backups/<时间>-<进程号>/`；命令输出中会显示该目录。
`previous.tar.gz` 保存原文件（有原文件时才生成），`existing.list` 列出覆盖的文件，
`new.list` 列出此次新建的文件。部署不会自动清理这些备份。

这些命令是开发文件部署，不是系统刷写：不替换内核、启动镜像、固件，不复制 `/etc/config/`
或重新执行开机初始化，不编译翻译目录。只覆盖所选源码中的文件，不自动删除设备上其他文件；
如果删除或重命名了源码文件，需要单独处理设备上的旧文件。

部署会清理 LuCI 服务端缓存，但不会自动重启服务。仅改前端后，直接打开设备页面并强制刷新；
本地代理本来就读取源码，因此不能用本地代理的页面来判断设备文件是否已更新。

后端、菜单或 ACL 改动通常使用：

```sh
make deploy-backend && make restart
```

`deploy-all` 也仅指前端与插件后端，不包含 `rootfs/overlay/` 中的全部设备命令。

## 重启和设备管理

| 命令 | 行为 |
|---|---|
| `make ssh` | 打开设备 SSH 终端 |
| `make status` | 查看内核、镜像版本、运行时间、内存和 Web 服务状态 |
| `make cache-clear` | 清理 LuCI 菜单与模块缓存，不重启服务 |
| `make restart` | 清缓存，重启 `rpcd` 和 `uhttpd`；管理会话会中断，可能需要重新登录 |
| `make reboot` | 整机重启，依据设备当前启动设置选择系统；不修改默认启动选择 |

`restart` 不会调用 Wi-Fi、蜂窝网络或锁频服务的重启命令。

## 调试

| 命令 | 输出 |
|---|---|
| `make logs` | 持续输出设备 `logread -f`，Ctrl+C 结束 |
| `make debug` | `system board` 与 `mu300dash status` 的 JSON 输出；状态接口可能刷新后台状态缓存 |
| `make rpc` | `mu300dash` 的方法及参数声明，不执行这些方法 |
| `make rpc RPC_OBJECT=mu300vpn` | 查看 VPN 接口的方法及参数声明 |

这些命令是需要时手动执行的工具，`make dev` 不会自动运行部署、重启或调试命令。
