# ZTE F50 控制台

基于 f50d 的设备管理前端：React + Vite + shadcn/ui（radix-nova）+ TanStack Query/Table + Jotai + React Hook Form/Zod。
所有数据都来自设备上的 f50d：浏览器通过 uhttpd 的 `/ubus` JSON-RPC 调用 `f50.*` 对象，用 rpcd 会话登录（OpenWrt 的系统账户）。
接口清单见 [`../f50d/docs/API.md`](../f50d/docs/API.md)，运行时也可调用 `f50.meta describe` 取得。

```sh
make dev            # 127.0.0.1:5173，/ubus 和 /cgi-bin 代理到设备（F50_DEVICE=http://192.168.77.1）
make build          # 生产构建到 dist/（base 为 /f50/）
make deploy         # 构建并经 SSH 复制到设备的 /www/f50（DEVICE=root@192.168.77.1）
make deploy-stage   # stage 构建，复制到 STAGE_DEVICE
```

镜像构建时由 `f50d/openwrt/install.sh ROOT BINARY SRC dist` 安装到 `/www/f50`；页面在站点根路径（`/`、`/sms`），`/www/index.html` 链接到 `f50/index.html`。LuCI 仍在 `/cgi-bin/luci`。

### 与脚手架默认的不同

- **History 路由，页面在根路径**：文件在 `/www/f50`（构建的 `base` 是 `/f50/`），页面地址是 `/`、`/sms`。uhttpd 自身不会把深层路径回退到 `index.html`：uci-defaults `93-f50-web`（以及 `make deploy`）把 `uhttpd.main.error_page` 设为 `/f50/index.html`，没有对应文件的路径都交给控制台，由它显示自己的 404。旧的 `/f50/#/sms` 地址在启动时改写为 `/sms`。路由不能用 `/f50`、`/ubus`、`/cgi-bin`、`/luci-static` 开头。
- **布局**：用 shadcn Sidebar 的管理后台布局取代了首页/关于的分段标签；去掉了 Cookie 提示（路由器管理页没有用）。
- **请求**：`src/lib/request.ts` 是唯一的 fetch 封装；`src/lib/ubus.ts` 提供 `f50(object, method, args)`，解开 f50d 的 `{ok,data}` / `{ok:false,code,error,field}`，失败时抛出 `ApiError`。会话过期会回到登录页。
- **后台任务**：返回 `{job}` 的方法用 `useJob` / `JobProgress` 跟踪（`src/hooks/use-job.ts`，`src/components/f50/job-progress.tsx`）。

- **API 文档**：`public/api-docs/` 是 Swagger UI，读同目录的 `openapi.json`（由 `f50d/docs/api/generate.sh` 从设备生成）。设备上是
  `/f50/api-docs/`，开发时打开 `/api-docs/index.html`。“Try it out” 会把虚拟路径 `POST /{object}/{method}` 转成 `/ubus` 的
  JSON-RPC 调用，并使用控制台当前的登录。

### 页面

仪表盘；蜂窝网络（信号与小区、SIM、网络锁定、移动数据、流量统计、短信）；网络（Wi-Fi 热点、Wi-Fi 上网、局域网、USB 与 adb）；
系统（信息、设置与重启、CPU、指示灯、更新与离线升级、启动、日志）；工具（AT 终端、后台任务）。每个页面在 `src/views/<路由>/`，
包含 `index.tsx`、`api.ts`、`types.ts` 和自己的 `components/`。三个以上页面共用的组件在 `src/components/f50/`，工具函数在 `src/lib/`。

---

## 移动端与嵌入限制

- viewport 支持安全区域，布局使用 `env(safe-area-inset-*)` 和动态视口高度。
- `touch-action: manipulation` 禁用双击缩放，保留双指缩放；移动端输入框至少 16px，避免 iOS Safari 聚焦自动放大。
- 包含 Safari 主屏幕、状态栏、浅深色浏览器外观和关闭自动电话号码等识别的 meta；这些不是完整 PWA 配置。
- 开发与预览服务发送禁止 iframe 嵌入的 CSP 和 X-Frame-Options 响应头。
- `public/_headers` 适用于支持该文件的静态托管平台；其他服务器需自行设置这两个响应头（`Content-Security-Policy: frame-ancestors 'none'` 和 `X-Frame-Options: DENY`）。`frame-ancestors` 和 X-Frame-Options 不能通过 HTML meta 生效，HTML 脚本隐藏仅为兜底，不能替代响应头。

## 后台更新检测

`public/app-update-checker.worker.js` 是浏览器 Web Worker（不是 Service Worker），生产环境首次挂载时读取部署入口 `index.html` 的 ETag / Last-Modified 建立基线；之后仅在页面重新可见时复查，无定时轮询。检测请求在 Worker 中执行，不走版本接口。

- 优先 HEAD + `cache: no-cache`；服务器返回 405 / 501 时回退 GET 并取消响应体。
- ETag 或 Last-Modified 变化时显示更新提示，关闭不会刷新页面；确认刷新时保留当前路由、查询参数和 hash，添加 `t` 时间戳跳过旧缓存。
- Worker 和 HTML 地址支持 Vite `base` 子路径部署；状态使用 Jotai，提示使用 SweepShine 展示刷新中状态。
- 部署服务器需要为入口 HTML 返回稳定的 ETag 或 Last-Modified，并允许重新验证缓存。没有这些响应头时不会提示更新。
- 检测基于首次成功请求的基线：若页面加载前已拿到旧缓存，而首次检测时已部署新版，无法判断该页面已过期。请为 HTML 配置 `Cache-Control: no-cache`，静态 hash 资源可长期缓存。
- 开发环境不启动 Worker；未保存的输入不会被自动刷新打断。

## UI 与表格约定

- 表格统一使用默认依赖 `@tanstack/react-table` 管理列定义、排序、筛选、分页和行选择；使用 shadcn/ui 的 `Table` 组件渲染，不自行实现另一套表格状态。

## 默认功能

- BuildInfo 直接从 `package.json` 导入名称、版本，不定义 `VITE_APP_NAME` / `VITE_APP_VERSION`。`VITE_BUILD_TIME` 由 Vite `define` 注入 ISO 时间，无需在 `.env` 配置；生产环境为打包时间，开发环境为开发服务启动时间。使用 date-fns 按访问者本地时区格式化；页面紧凑显示 `yyyy-MM-dd HH:mm`，悬停显示带秒、时区偏移的完整时间及中文相对时间。控制台用彩色标签输出版本和时间，其他浏览器公开环境变量在默认折叠的表格分组中显示。不要在 `VITE_*` 变量中放置密钥。
- NProgress 顶部进度条统一监听 React Router 的 location，覆盖侧栏导航、Link、程序跳转和浏览器前进/后退。默认不显示转圈图标，使用主题主色，支持减少动态效果。进度条是页面提交后的短暂切换反馈，不代表接口或异步资源加载进度。
- SweepShine 提供文字和表面两种加载效果，CSS 统一在 `src/index.css`，减少动态效果偏好下不播放动画。
- 默认使用 `components/theme/theme-toggle-button.tsx` 的紧凑日/月按钮；明暗两种方向都从实际点击位置向外扩散，键盘触发时从按钮中心扩散，不支持 View Transitions 或偏好减少动态效果时直接切换。主题持久化到 localStorage。
- 原 `ThemeToggle` 完整外观面板保留为可选组件，不再默认展示。
- Jotai Provider 位于入口，主题状态在 `src/store/theme.ts`；表单状态由 RHF 管理，服务端数据由 Query 管理。
- `@/*` 对应 `src/*`，已同时配置 TypeScript 路径映射和 Vite 别名，例如 `@/views/dashboard`、`@/layout`。
- `src/index.css` 是唯一全局 CSS 入口：保留 shadcn CLI 生成的 Tailwind 导入和主题变量，生成器追加主色、圆角、字号、布局及移动端样式，不再单独使用 `appearance.css`。后续全局样式统一写在此文件。
- `index.html` 在模块脚本启动前恢复主题，并为 Internet Explorer 显示不支持提示；不提供 IE polyfill。
- `src/lib/request.ts` 提供请求封装，环境变量示例见 `.env.development` 和 `.env.production`。
