import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import {
  ChevronRightIcon,
  GlobeIcon,
  HardDriveDownloadIcon,
  type LucideIcon,
  MessageSquareIcon,
  RadioTowerIcon,
  UsbIcon,
  WifiIcon,
} from "lucide-react";
import { DataSwitch, WifiSwitch } from "./switches";
import {
  DATA_HOLD_LABEL,
  UPLINK_MODE_LABEL,
  USB_ROLE_LABEL,
  label,
} from "../labels";
import type { DashboardStatus, Part } from "../types";

/** iOS Settings' icon tile: a white glyph on the row's colour. */
function Glyph({ icon: Icon, color }: { icon: LucideIcon; color: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "flex size-[30px] shrink-0 items-center justify-center rounded-[8px] text-white",
        color,
      )}
    >
      <Icon className="size-[17px]" strokeWidth={2.2} />
    </span>
  );
}

const rowBase =
  "flex min-h-[56px] items-center gap-3 px-4 py-2 transition-colors";

/**
 * One row: glyph, title over a short detail, then the row's control (a switch) or its value and a chevron when the
 * whole row opens the page.
 */
function Row({
  icon,
  color,
  title,
  detail,
  value,
  to,
  control,
}: {
  icon: LucideIcon;
  color: string;
  title: string;
  detail?: ReactNode;
  value?: ReactNode;
  to: string;
  control?: ReactNode;
}) {
  const body = (
    <>
      <Glyph icon={icon} color={color} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm leading-5 font-medium">{title}</span>
        {detail && (
          <span className="truncate text-xs leading-4 text-muted-foreground tabular-nums">
            {detail}
          </span>
        )}
      </span>
      {value != null && (
        <span className="max-w-[45%] shrink-0 truncate text-sm text-muted-foreground tabular-nums">
          {value}
        </span>
      )}
    </>
  );
  if (control)
    return (
      <li className={rowBase}>
        <Link
          to={to}
          className="flex min-w-0 flex-1 items-center gap-3 hover:text-primary"
        >
          {body}
        </Link>
        {control}
      </li>
    );
  return (
    <li>
      <Link to={to} className={cn(rowBase, "hover:bg-muted/60")}>
        {body}
        <ChevronRightIcon
          aria-hidden
          className="size-4 shrink-0 text-muted-foreground/60"
        />
      </Link>
    </li>
  );
}

/** A part's data, or null; a failed part shows "read failed" in its row instead of its value. */
const ok = <T,>(p: Part<T> | undefined) => (p?.ok ? p.data : null);
const failed = (p: Part<unknown> | undefined) =>
  p && !p.ok && p.code !== "unsupported";

/**
 * What the device serves and what can be switched, one row each and said once: mobile data and the hotspot (with
 * their switches), the Wi-Fi uplink, USB, messages and the system version. The network itself (carrier, speed, usage,
 * signal) is the device card's.
 */
export function ServiceList({
  s,
  aside,
}: {
  s: DashboardStatus;
  /** Shown at the right of the heading (the page's health and, while polling, refresh). */
  aside?: ReactNode;
}) {
  const data = ok(s.data);
  const wifi = ok(s.wifi);
  const up = ok(s.wificlient);
  const usb = ok(s.usb);
  const sms = ok(s.sms);
  const update = ok(s.update);
  const readFailed = t("common.state.readFailed");

  const usbRole =
    usb &&
    (usb.role === "unavailable" && usb.link?.bound
      ? label(USB_ROLE_LABEL, "device")
      : label(USB_ROLE_LABEL, usb.role));

  return (
    <section className="overflow-hidden rounded-2xl border bg-card">
      <div className="flex min-h-11 items-center justify-between gap-2 px-4 pt-3 pb-1">
        <h2 className="text-sm font-semibold text-muted-foreground">
          {t("dashboard.panel.services")}
        </h2>
        {aside && <div className="flex items-center gap-1.5">{aside}</div>}
      </div>
      <ul className="divide-y divide-border/70">
        <Row
          icon={GlobeIcon}
          color="bg-[#34C759]"
          title={t("dashboard.quick.data")}
          detail={
            data
              ? data.hold
                ? label(DATA_HOLD_LABEL, data.hold)
                : data.error && data.state !== "connected"
                  ? data.error
                  : `APN ${data.apn || t("dashboard.data.defaultApn")}`
              : failed(s.data)
                ? readFailed
                : undefined
          }
          to="/cellular/data"
          control={data ? <DataSwitch data={data} /> : undefined}
          value={data ? undefined : "—"}
        />
        <Row
          icon={WifiIcon}
          color="bg-[#007AFF]"
          title={t("dashboard.quick.wifi")}
          detail={
            wifi
              ? wifi.client_mode
                ? t("dashboard.wifi.clientMode")
                : [
                    wifi.ssid,
                    `${wifi.band} GHz`,
                    wifi.enabled
                      ? t("dashboard.wifi.clients", {
                          count: wifi.clients.length,
                        })
                      : t("dashboard.wifi.off"),
                  ]
                    .filter(Boolean)
                    .join(" · ")
              : failed(s.wifi)
                ? readFailed
                : undefined
          }
          to="/network/wifi"
          control={wifi ? <WifiSwitch wifi={wifi} /> : undefined}
          value={wifi ? undefined : "—"}
        />
        <Row
          icon={RadioTowerIcon}
          color="bg-[#5AC8FA]"
          title={t("dashboard.uplink.title")}
          detail={
            up?.active && !up.firewall_ok
              ? t("dashboard.uplink.firewall")
              : undefined
          }
          value={
            up
              ? up.mode === "client"
                ? `${up.ssid ?? "—"} · ${up.connected ? t("dashboard.uplink.online") : t("dashboard.uplink.offline")}`
                : label(UPLINK_MODE_LABEL, up.mode)
              : failed(s.wificlient)
                ? readFailed
                : "—"
          }
          to="/network/wifi-client"
        />
        <Row
          icon={UsbIcon}
          color="bg-[#8E8E93]"
          title={t("dashboard.usb.title")}
          value={
            usb
              ? [
                  usbRole,
                  (usb.net_current || usb.net_mode).toUpperCase(),
                  usb.adb.on ? "ADB" : null,
                ]
                  .filter(Boolean)
                  .join(" · ")
              : failed(s.usb)
                ? readFailed
                : "—"
          }
          to="/network/usb"
        />
        <Row
          icon={MessageSquareIcon}
          color="bg-[#30B94D]"
          title={t("dashboard.sms.title")}
          value={
            sms ? (
              sms.unread > 0 ? (
                <span className="rounded-full bg-[#FF3B30] px-2 py-0.5 text-xs font-semibold text-white">
                  {sms.unread}
                </span>
              ) : (
                t("dashboard.overview.noUnread")
              )
            ) : (
              "—"
            )
          }
          to="/sms"
        />
        <Row
          icon={HardDriveDownloadIcon}
          color="bg-[#636366]"
          title={t("dashboard.update.title")}
          detail={
            update?.job
              ? t("dashboard.update.jobRunning")
              : update?.restart_pending
                ? t("dashboard.update.restartPending")
                : undefined
          }
          value={
            update ? (
              update.notice ? (
                <span className="rounded-full bg-[#FF9500]/15 px-2 py-0.5 text-xs font-semibold text-[#C93400] dark:text-[#FF9F0A]">
                  {update.notice.tag}
                </span>
              ) : (
                (update.installed ?? t("dashboard.update.unknownVersion"))
              )
            ) : (
              "—"
            )
          }
          to="/system/update"
        />
      </ul>
    </section>
  );
}
