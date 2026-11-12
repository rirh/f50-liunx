.PHONY: help

help:
	@printf '%s\n' \
	  '本地开发' \
	  '  make dev             启动本地代理（127.0.0.1:$(PORT)，Ctrl+C 停止）' \
	  '  make open            浏览器打开本地页面，不启动代理' \
	  '' \
	  '部署（通过 SSH，只覆盖所选文件，先备份，不自动重启）' \
	  '  make deploy          部署前端，等同 deploy-ui' \
	  '  make deploy-ui       部署 LuCI 插件前端和 OpenWrt 额外网页资源' \
	  '  make deploy-backend  部署插件的接口、菜单和 ACL，不覆盖设备配置' \
	  '  make deploy-all      部署以上两部分，不刷系统或内核' \
	  '' \
	  '设备管理' \
	  '  make ssh             登录设备终端' \
	  '  make status          查看系统版本、内存和 Web 服务状态' \
	  '  make cache-clear     清除 LuCI 服务端缓存' \
	  '  make restart         清缓存并重启 rpcd、uhttpd，会中断管理会话' \
	  '  make reboot          整机重启，按设备当前启动设置进入系统' \
	  '' \
	  '调试' \
	  '  make logs            跟踪设备系统日志（Ctrl+C 退出）' \
	  '  make debug           输出设备与 MU300 状态 JSON' \
	  '  make rpc             列出 RPC 对象方法；RPC_OBJECT=mu300vpn 可切换对象' \
	  '' \
	  '配置：make/local.mk.example 可复制为 make/local.mk（已被 Git 忽略）' \
	  'DEVICE 设置 HTTP 地址；SSH_HOST 默认跟随 DEVICE；SSH_PORT 默认 22。'
