.PHONY: help

help:
	@printf '%s\n' \
	  '首次安装（macOS + 原厂 Android 的 F50；安装/维护命令会显式准备 ADB 与 root）' \
	  '  make install         构建 f50d 和网页，开启 ADB，刷入 OpenWrt（本地构建，不下载上游镜像）' \
	  '  make install-check   只检查设备空闲区域，不写入' \
	  '  make adb             USB ADB 已连接则提示成功并退出；未连接时恢复，需 F50 Wi-Fi 和 Go' \
	  '  make uninstall       卸载 Linux，恢复原厂 Android' \
	  '  make backup          备份不可替代分区；BACKUP_DIR=... 指定目录' \
	  '  make backup-full     全分区备份（大）' \
	  '  make collect-logs    回落 Android 后收集启动日志' \
	  '  make reset-password  从 Android 重置 OpenWrt 的 root 密码' \
	  '' \
	  '构建' \
	  '  make build           f50d（aarch64 静态）和网页' \
	  '  make f50d            只构建 f50d' \
	  '  make web             只构建网页（f50-web/dist）' \
	  '  make kernel          用 docker 构建 5.4 内核和模块到 out/（install 需要）' \
	  '  make clean           清理 f50d 和网页的构建产物' \
	  '' \
	  '测试' \
	  '  make test            f50d 单元测试 + 格式和 clippy + 网页 lint 和类型检查' \
	  '  make test-f50d       只跑 f50d 单元测试' \
	  '  make lint            只做格式、clippy 和网页 lint' \
	  '  make test-installer  安装器和镜像的 Python 测试' \
	  '  make test-all        以上全部' \
	  '' \
	  '部署（设备已运行 OpenWrt + f50d，通过 SSH，不重刷）' \
	  '  make deploy          部署 f50d 和网页' \
	  '  make deploy-f50d     只换 f50d 并重启守护进程' \
	  '  make deploy-web      只部署网页到 /www/f50' \
	  '  make deploy-reboot   部署后整机重启（AT 通道进程也换成新版本）' \
	  '' \
	  '设备' \
	  '  make ssh             登录设备终端' \
	  '  make status          内核、镜像版本、内存和 f50d 服务状态' \
	  '  make logs            跟踪系统日志（Ctrl+C 退出）' \
	  '  make restart         重启 f50d 守护进程' \
	  '  make reboot          整机重启' \
	  '' \
	  '开发' \
	  '  make dev             网页开发服务器（127.0.0.1:5173，/ubus 转发到设备）' \
	  '' \
	  '配置：复制 make/local.mk.example 为 make/local.mk（已被 Git 忽略）' \
	  'DEVICE_IP 设备地址（默认 $(DEVICE_IP)）；SSH_TARGET 默认 root@DEVICE_IP；SSH_OPTS 追加 ssh 参数。' \
	  'F50_IP 原厂管理地址（默认 $(F50_IP)）；F50_SERIAL 指定 USB 序列号；INSTALL_ARGS 传给 install.sh。'
