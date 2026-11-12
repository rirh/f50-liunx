import { type Key, t } from "@/i18n";
import {
  ActivityIcon,
  ArrowUpDownIcon,
  CardSimIcon,
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
  TerminalIcon,
  UsbIcon,
  WifiIcon,
} from "lucide-react";

export interface NavItem {
  title: Key;
  path: string;
  icon: LucideIcon;
}

export interface NavGroup {
  title: Key;
  items: NavItem[];
}

/**
 * Every page of the app, as the sidebar shows them (the routes in App.tsx match these paths). Grouped by how often
 * they are used: the everyday figures first, then the connections, the device, and last what needs care (locks, AT
 * commands) or is for diagnosis.
 */
export const NAV: NavGroup[] = [
  {
    title: "nav.group.overview",
    items: [
      { title: "nav.page.dashboard", path: "/", icon: LayoutDashboardIcon },
      {
        title: "nav.page.systemInfo",
        path: "/system/overview",
        icon: InfoIcon,
      },
    ],
  },
  {
    title: "nav.group.mobile",
    items: [
      { title: "nav.page.signal", path: "/cellular/signal", icon: SignalIcon },
      { title: "nav.page.data", path: "/cellular/data", icon: ArrowUpDownIcon },
      { title: "nav.page.traffic", path: "/cellular/traffic", icon: GaugeIcon },
      { title: "nav.page.sms", path: "/sms", icon: MessageSquareIcon },
      { title: "nav.page.sim", path: "/cellular/sim", icon: CardSimIcon },
    ],
  },
  {
    title: "nav.group.connect",
    items: [
      { title: "nav.page.wifi", path: "/network/wifi", icon: WifiIcon },
      {
        title: "nav.page.wifiClient",
        path: "/network/wifi-client",
        icon: ActivityIcon,
      },
      { title: "nav.page.lan", path: "/network/lan", icon: NetworkIcon },
      { title: "nav.page.usb", path: "/network/usb", icon: UsbIcon },
    ],
  },
  {
    title: "nav.group.device",
    items: [
      {
        title: "nav.page.settings",
        path: "/system/settings",
        icon: SettingsIcon,
      },
      {
        title: "nav.page.update",
        path: "/system/update",
        icon: HardDriveDownloadIcon,
      },
      { title: "nav.page.cpu", path: "/system/cpu", icon: CpuIcon },
      { title: "nav.page.leds", path: "/system/leds", icon: LightbulbIcon },
    ],
  },
  {
    title: "nav.group.advanced",
    items: [
      { title: "nav.page.locks", path: "/cellular/locks", icon: LockIcon },
      { title: "nav.page.at", path: "/tools/at", icon: TerminalIcon },
      { title: "nav.page.jobs", path: "/tools/jobs", icon: ListChecksIcon },
      { title: "nav.page.logs", path: "/system/logs", icon: ScrollTextIcon },
      { title: "nav.page.boot", path: "/system/boot", icon: PowerIcon },
    ],
  },
];

/** The page title for the header, in the current language. */
export function titleOf(pathname: string): string {
  for (const g of NAV)
    for (const i of g.items) if (i.path === pathname) return t(i.title);
  return "";
}
