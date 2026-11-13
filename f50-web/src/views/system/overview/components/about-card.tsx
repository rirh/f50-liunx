import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { type Key, t } from "@/i18n";
import {
  errorMessage,
  formatBytes,
  formatDuration,
  formatTime,
} from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { EyeIcon, EyeOffIcon, PencilIcon } from "lucide-react";
import { toast } from "sonner";
import { Group, Row } from "./grouped-list";
import { OwnNumberDialog } from "./own-number-dialog";
import { getOwnNumber } from "../api";
import type { LiveStatus } from "../hooks/use-live-status";
import type { Disk, SystemInfo } from "../types";
import { diskPercent, formatPercent, kb } from "../utils";

const MSISDN_SRC = {
  sim: "system.overview.info.msisdnSrc.sim",
  ims: "system.overview.info.msisdnSrc.ims",
} as const satisfies Record<string, Key>;

/** The SIM's own number: private, hidden until asked for (a write-access method); settable for SIMs without it. */
function OwnNumber() {
  const [editing, setEditing] = useState(false);
  const reveal = useMutation({
    mutationFn: getOwnNumber,
    onError: (e) => toast.error(errorMessage(e)),
  });
  if (!reveal.isSuccess) {
    return (
      <PendingButton
        variant="outline"
        size="xs"
        pending={reveal.isPending}
        icon={<EyeIcon data-icon="inline-start" />}
        onClick={() => reveal.mutate()}
      >
        {t("common.action.show")}
      </PendingButton>
    );
  }
  const d = reveal.data;
  return (
    <span className="flex flex-wrap items-center gap-2">
      {d.msisdn ? (
        <span className="font-mono">
          {d.msisdn}
          {d.msisdn_src && (
            <span className="text-muted-foreground">
              {" "}
              {t("system.overview.info.numberSource", {
                slot: d.slot,
                src: t(MSISDN_SRC[d.msisdn_src]),
              })}
            </span>
          )}
        </span>
      ) : (
        <span className="text-muted-foreground">
          {t("system.overview.info.notRead")}
        </span>
      )}
      <Button variant="outline" size="xs" onClick={() => setEditing(true)}>
        <PencilIcon data-icon="inline-start" />
        {t("system.overview.info.ownNumberSet.action")}
      </Button>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={t("system.overview.info.hideNumber")}
        onClick={() => reveal.reset()}
      >
        <EyeOffIcon />
      </Button>
      <OwnNumberDialog
        open={editing}
        onOpenChange={setEditing}
        current={d.msisdn}
        onSaved={() => reveal.mutate()}
      />
    </span>
  );
}

function Usage({
  used,
  total,
  percent,
}: {
  used: number | null;
  total: number | null;
  percent: number | null;
}) {
  return (
    <>
      {formatBytes(used)} / {formatBytes(total)}
      <span className="text-muted-foreground"> · {formatPercent(percent)}</span>
    </>
  );
}

function DiskRow({ label, disk }: { label: string; disk: Disk | null }) {
  return (
    <Row label={label}>
      {disk ? (
        <Usage
          used={kb(disk.used_kb)}
          total={kb(disk.size_kb)}
          percent={diskPercent(disk)}
        />
      ) : (
        <span className="text-muted-foreground">
          {t("system.overview.info.notMounted")}
        </span>
      )}
    </Row>
  );
}

function usbText(u: LiveStatus["status"]["usb_link"]) {
  if (!u) return t("common.state.disconnected");
  const role =
    u.role === "device"
      ? t("system.overview.live.usbDevice")
      : t("system.overview.live.usbHost");
  const speed = u.mbps != null ? `${u.mbps} Mbps` : u.speed;
  return speed ? `${role} · ${speed}` : role;
}

/** The device's facts as grouped rows: device, time, hardware, storage and connections. */
export function AboutCard({
  info,
  live,
}: {
  info: SystemInfo | undefined;
  live: LiveStatus | undefined;
}) {
  const s = live?.status;
  const m = info?.memory;
  const swapUsed =
    m?.swap_total_kb != null && m.swap_free_kb != null
      ? m.swap_total_kb - m.swap_free_kb
      : null;
  const z = info?.zram;
  return (
    <Card className="gap-4">
      <CardHeader>
        <CardTitle>{t("system.overview.info.title")}</CardTitle>
      </CardHeader>
      <CardContent>
        {!info ? (
          <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
            {Array.from({ length: 4 }, (_, i) => (
              <Skeleton key={i} className="h-36 rounded-xl" />
            ))}
          </div>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <Group title={t("system.overview.info.sections.device")}>
              <Row label={t("system.overview.info.model")}>{info.model}</Row>
              <Row label={t("system.overview.info.release")}>
                {info.release}
              </Row>
              <Row label={t("system.overview.info.kernel")} mono>
                {info.kernel}
              </Row>
              <Row label="CPU">
                {info.load
                  ? t("system.overview.info.cpuValue", {
                      n: info.load.n,
                      clusters: info.load.freqs.length,
                    })
                  : null}
              </Row>
              <Row label={t("system.overview.info.hostname")}>
                {info.hostname}
              </Row>
              <Row label={t("system.overview.info.ownNumber")}>
                <OwnNumber />
              </Row>
            </Group>
            <Group title={t("system.overview.info.sections.time")}>
              <Row label={t("system.overview.info.timezone")}>
                {info.timezone ?? "UTC"}
              </Row>
              <Row label={t("system.overview.info.time")}>
                {formatTime(info.time)}
              </Row>
              <Row label={t("system.overview.info.uptime")}>
                {formatDuration(s?.uptime ?? info.uptime)}
              </Row>
            </Group>
            <Group title={t("system.overview.info.sections.storage")}>
              <DiskRow
                label={t("system.overview.info.rootfs")}
                disk={info.storage.root}
              />
              <DiskRow
                label={t("system.overview.info.data")}
                disk={info.storage.disk}
              />
              {!!m?.swap_total_kb && (
                <Row label={t("system.overview.info.swap")}>
                  <Usage
                    used={kb(swapUsed)}
                    total={kb(m.swap_total_kb)}
                    percent={
                      swapUsed != null
                        ? (swapUsed / m.swap_total_kb) * 100
                        : null
                    }
                  />
                </Row>
              )}
              <Row label={t("system.overview.info.zram")}>
                {z?.on
                  ? `${formatBytes(z.size_bytes)} · ${z.algorithm ?? "—"}`
                  : z?.enabled
                    ? t("common.state.stopped")
                    : t("common.state.disabled")}
              </Row>
            </Group>
            <Group title={t("system.overview.info.sections.network")}>
              <Row label={t("system.overview.live.wifi")}>
                {s?.wifi_clients}
              </Row>
              <Row label={t("system.overview.info.leases")}>
                {s?.lan_leases}
              </Row>
              <Row label={t("system.overview.live.conns")}>{s?.conns}</Row>
              <Row label={t("system.overview.live.usb")}>
                {s ? usbText(s.usb_link) : null}
              </Row>
            </Group>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
