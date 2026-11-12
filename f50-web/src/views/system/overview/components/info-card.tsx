import { KvList } from "@/components/f50/kv-list";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { type Key, t } from "@/i18n";
import {
  errorMessage,
  formatBytes,
  formatDuration,
  formatTime,
} from "@/lib/format";
import { useMutation, type UseQueryResult } from "@tanstack/react-query";
import { EyeIcon, EyeOffIcon } from "lucide-react";
import { toast } from "sonner";
import { getOwnNumber } from "../api";
import type { Disk, SystemInfo } from "../types";
import { diskPercent, formatPercent, kb, memPercent } from "../utils";

function Usage({
  label,
  used,
  total,
  percent,
}: {
  label: string;
  used: number | null;
  total: number | null;
  percent: number | null;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline justify-between gap-2 text-sm">
        <span className="text-muted-foreground">{label}</span>
        <span className="tabular-nums">
          {formatBytes(used)} / {formatBytes(total)} · {formatPercent(percent)}
        </span>
      </div>
      <Progress value={percent ?? 0} aria-label={label} />
    </div>
  );
}

function DiskUsage({ label, disk }: { label: string; disk: Disk | null }) {
  if (!disk) {
    return (
      <div className="flex items-baseline justify-between gap-2 text-sm">
        <span className="text-muted-foreground">{label}</span>
        <span>{t("system.overview.info.notMounted")}</span>
      </div>
    );
  }
  return (
    <Usage
      label={label}
      used={kb(disk.used_kb)}
      total={kb(disk.size_kb)}
      percent={diskPercent(disk)}
    />
  );
}

const MSISDN_SRC = {
  sim: "system.overview.info.msisdnSrc.sim",
  ims: "system.overview.info.msisdnSrc.ims",
} as const satisfies Record<string, Key>;

/** The SIM's own number: private, hidden until asked for (a write-access method). */
function OwnNumber() {
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
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={t("system.overview.info.hideNumber")}
        onClick={() => reveal.reset()}
      >
        <EyeOffIcon />
      </Button>
    </span>
  );
}

function InfoBody({ info }: { info: SystemInfo }) {
  const m = info.memory;
  const swapUsed =
    m.swap_total_kb != null && m.swap_free_kb != null
      ? m.swap_total_kb - m.swap_free_kb
      : null;
  return (
    <div className="flex flex-col gap-4">
      <KvList
        items={[
          { label: t("system.overview.info.model"), value: info.model },
          { label: t("system.overview.info.release"), value: info.release },
          {
            label: t("system.overview.info.kernel"),
            value: info.kernel && (
              <span className="font-mono">{info.kernel}</span>
            ),
          },
          { label: t("system.overview.info.hostname"), value: info.hostname },
          { label: t("system.overview.info.ownNumber"), value: <OwnNumber /> },
          {
            label: t("system.overview.info.timezone"),
            value: info.timezone ?? "UTC",
          },
          {
            label: t("system.overview.info.time"),
            value: formatTime(info.time),
          },
          {
            label: t("system.overview.info.uptime"),
            value: formatDuration(info.uptime),
          },
          {
            label: "CPU",
            value: info.load
              ? t("system.overview.info.cpuValue", {
                  n: info.load.n,
                  clusters: info.load.freqs.length,
                })
              : null,
          },
          {
            label: t("system.overview.info.zram"),
            value: info.zram.on ? (
              <span className="flex flex-wrap items-center gap-2">
                <Badge variant="secondary">
                  {t("system.overview.info.zramOn")}
                </Badge>
                {t("system.overview.info.zramDetail", {
                  size: formatBytes(info.zram.size_bytes),
                  algorithm: info.zram.algorithm ?? "—",
                  stored: formatBytes(info.zram.stored_bytes),
                  ram: formatBytes(info.zram.ram_used_bytes),
                })}
              </span>
            ) : (
              <Badge variant="outline">
                {info.zram.enabled
                  ? t("common.state.stopped")
                  : t("common.state.disabled")}
              </Badge>
            ),
          },
        ]}
      />
      <Separator />
      <div className="flex flex-col gap-3">
        <Usage
          label={t("system.overview.info.mem")}
          used={kb(
            m.total_kb != null && m.avail_kb != null
              ? m.total_kb - m.avail_kb
              : null,
          )}
          total={kb(m.total_kb)}
          percent={memPercent(m)}
        />
        {!!m.swap_total_kb && (
          <Usage
            label={t("system.overview.info.swap")}
            used={kb(swapUsed)}
            total={kb(m.swap_total_kb)}
            percent={
              swapUsed != null && m.swap_total_kb
                ? (swapUsed / m.swap_total_kb) * 100
                : null
            }
          />
        )}
        <DiskUsage
          label={t("system.overview.info.rootfs")}
          disk={info.storage.root}
        />
        <DiskUsage
          label={t("system.overview.info.data")}
          disk={info.storage.disk}
        />
      </div>
    </div>
  );
}

/** The device's facts (read once, refreshed with the page). */
export function InfoCard({ query }: { query: UseQueryResult<SystemInfo> }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("system.overview.info.title")}</CardTitle>
        <CardDescription>
          {t("system.overview.info.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState query={query} skeletonRows={10}>
          {(info) => <InfoBody info={info} />}
        </QueryState>
      </CardContent>
    </Card>
  );
}
