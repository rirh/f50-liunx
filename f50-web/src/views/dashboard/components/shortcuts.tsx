import { Link } from "react-router-dom";
import { type Key, t } from "@/i18n";
import {
  GaugeIcon,
  LightbulbIcon,
  ListChecksIcon,
  LockIcon,
  type LucideIcon,
  NetworkIcon,
  ScrollTextIcon,
  SettingsIcon,
  TerminalIcon,
} from "lucide-react";

/** Pages the rest of the dashboard does not already lead to. */
const LINKS: { to: string; icon: LucideIcon; title: Key }[] = [
  {
    to: "/cellular/traffic",
    icon: GaugeIcon,
    title: "dashboard.quick.links.traffic",
  },
  {
    to: "/cellular/locks",
    icon: LockIcon,
    title: "dashboard.quick.links.locks",
  },
  { to: "/network/lan", icon: NetworkIcon, title: "dashboard.quick.links.lan" },
  { to: "/tools/at", icon: TerminalIcon, title: "dashboard.quick.links.at" },
  {
    to: "/system/leds",
    icon: LightbulbIcon,
    title: "dashboard.quick.links.leds",
  },
  {
    to: "/tools/jobs",
    icon: ListChecksIcon,
    title: "dashboard.quick.links.jobs",
  },
  {
    to: "/system/logs",
    icon: ScrollTextIcon,
    title: "dashboard.quick.links.logs",
  },
  {
    to: "/system/settings",
    icon: SettingsIcon,
    title: "dashboard.quick.links.settings",
  },
];

export function Shortcuts() {
  return (
    <section className="rounded-2xl border bg-card px-2 pt-3.5 pb-2">
      <h2 className="px-2 pb-1.5 text-sm font-semibold text-muted-foreground">
        {t("dashboard.panel.more")}
      </h2>
      <div className="grid grid-cols-4 gap-1">
        {LINKS.map(({ to, icon: Icon, title }) => (
          <Link
            key={to}
            to={to}
            className="group flex flex-col items-center gap-1.5 rounded-xl px-1 py-2.5 text-center transition-colors hover:bg-muted/70 focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
          >
            <Icon
              aria-hidden
              className="size-5 text-foreground/70 transition-colors group-hover:text-primary"
              strokeWidth={1.8}
            />
            <span className="line-clamp-1 text-xs font-medium text-foreground/80">
              {t(title)}
            </span>
          </Link>
        ))}
      </div>
    </section>
  );
}
