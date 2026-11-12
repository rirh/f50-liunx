import {
  ActivityIcon,
  ArrowUpDownIcon,
  CpuIcon,
  GaugeIcon,
  HardDriveDownloadIcon,
  InfoIcon,
  LayoutDashboardIcon,
  LightbulbIcon,
  ListChecksIcon,
  LockIcon,
  type LucideIcon,
  MessageSquareIcon,
  NetworkIcon,
  PowerIcon,
  ScrollTextIcon,
  SettingsIcon,
  SignalIcon,
  CardSimIcon,
  TerminalIcon,
  UsbIcon,
  WifiIcon,
} from "lucide-react";

export interface NavItem {
  title: string;
  path: string;
  icon: LucideIcon;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
}

/** Every page of the app, as the sidebar shows them (the routes in App.tsx match these paths). */
export const NAV: NavGroup[] = [
  {
    title: "概览",
    items: [{ title: "仪表盘", path: "/", icon: LayoutDashboardIcon }],
  },
  {
    title: "蜂窝网络",
    items: [
      { title: "信号与小区", path: "/cellular/signal", icon: SignalIcon },
      { title: "SIM 卡", path: "/cellular/sim", icon: CardSimIcon },
      { title: "网络锁定", path: "/cellular/locks", icon: LockIcon },
      { title: "移动数据", path: "/cellular/data", icon: ArrowUpDownIcon },
      { title: "流量统计", path: "/cellular/traffic", icon: GaugeIcon },
      { title: "短信", path: "/sms", icon: MessageSquareIcon },
    ],
  },
  {
    title: "网络",
    items: [
      { title: "Wi-Fi 热点", path: "/network/wifi", icon: WifiIcon },
      { title: "Wi-Fi 上网", path: "/network/wifi-client", icon: ActivityIcon },
      { title: "局域网", path: "/network/lan", icon: NetworkIcon },
      { title: "USB", path: "/network/usb", icon: UsbIcon },
    ],
  },
  {
    title: "系统",
    items: [
      { title: "系统信息", path: "/system/overview", icon: InfoIcon },
      { title: "系统设置", path: "/system/settings", icon: SettingsIcon },
      { title: "CPU", path: "/system/cpu", icon: CpuIcon },
      { title: "指示灯", path: "/system/leds", icon: LightbulbIcon },
      {
        title: "系统更新",
        path: "/system/update",
        icon: HardDriveDownloadIcon,
      },
      { title: "启动", path: "/system/boot", icon: PowerIcon },
      { title: "系统日志", path: "/system/logs", icon: ScrollTextIcon },
    ],
  },
  {
    title: "工具",
    items: [
      { title: "AT 终端", path: "/tools/at", icon: TerminalIcon },
      { title: "后台任务", path: "/tools/jobs", icon: ListChecksIcon },
    ],
  },
];

export function titleOf(pathname: string): string {
  for (const g of NAV)
    for (const i of g.items) if (i.path === pathname) return i.title;
  return "";
}
