import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { type Key, t } from "@/i18n";
import { formatBytes } from "@/lib/format";
import { SystemsTable } from "./systems-table";
import type { UpdateStatus } from "../types";

const KEY_LABEL: Record<string, Key> = {
  ok: "maintenance.update.status.key.ok",
  none: "maintenance.update.status.key.none",
  unsafe: "maintenance.update.status.key.unsafe",
} satisfies Record<UpdateStatus["signing"]["key"], Key>;
const FALLBACK_LABEL: Record<string, Key> = {
  auto: "maintenance.update.status.fallback.auto",
  never: "maintenance.update.status.fallback.never",
  always: "maintenance.update.status.fallback.always",
} satisfies Record<UpdateStatus["source"]["fallback"], Key>;

function Disk({ disk }: { disk: UpdateStatus["disk"] }) {
  if (disk.free_kb == null || disk.total_kb == null) return <>—</>;
  const used = ((disk.total_kb - disk.free_kb) / disk.total_kb) * 100;
  const enough = disk.free_kb >= disk.need_all_kb;
  return (
    <div className="flex flex-col gap-1">
      <span className="flex flex-wrap items-center gap-2">
        {t("maintenance.update.status.diskFree", {
          free: formatBytes(disk.free_kb * 1024),
          total: formatBytes(disk.total_kb * 1024),
        })}
        {!enough && (
          <Badge variant="destructive">
            {t("maintenance.update.status.diskShort", {
              need: formatBytes(disk.need_all_kb * 1024),
            })}
          </Badge>
        )}
      </span>
      <Progress
        value={used}
        aria-label={t("maintenance.update.status.diskUsed")}
      />
    </div>
  );
}

export function StatusCard({ status: s }: { status: UpdateStatus }) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{t("maintenance.update.status.title")}</CardTitle>
        <CardDescription>
          {t("maintenance.update.status.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <KvList
          items={[
            {
              label: t("maintenance.update.status.version"),
              value: (
                <span className="tabular-nums">
                  {s.installed ?? t("common.state.unknown")}
                </span>
              ),
            },
            {
              label: t("maintenance.update.status.kernel"),
              value: (
                <span className="tabular-nums">{s.kernel.running ?? "—"}</span>
              ),
            },
            {
              label: t("maintenance.update.status.kernelSeries"),
              value: t("maintenance.update.status.kernelChoice", {
                choice: s.kernel.choice,
                choices: s.kernel.choices.join(" / "),
              }),
            },
            {
              label: t("maintenance.update.status.boot"),
              value: (
                <span className="tabular-nums">
                  {s.boot.release ?? "—"} · {s.boot.kernel ?? "—"} ·{" "}
                  {t("maintenance.update.status.slot", { slot: s.boot.slot })}
                </span>
              ),
            },
            {
              label: t("maintenance.update.status.bootBackup"),
              value: s.boot.previous ? (
                <span className="flex flex-wrap items-center gap-2 tabular-nums">
                  {s.boot.previous.release ??
                    t("maintenance.update.status.unknownRelease")}{" "}
                  · {s.boot.previous.kernel ?? "—"}
                  {!s.boot.previous.usable && (
                    <Badge variant="outline">
                      {t("maintenance.update.status.slotMismatch")}
                    </Badge>
                  )}
                </span>
              ) : (
                t("common.state.none")
              ),
            },
            {
              label: t("maintenance.update.status.defaultBoot"),
              value: s.boot.default === "linux" ? "Linux" : "Android",
            },
            {
              label: t("maintenance.update.status.disk"),
              value: <Disk disk={s.disk} />,
            },
            {
              label: t("maintenance.update.status.signingKey"),
              value: KEY_LABEL[s.signing.key]
                ? t(KEY_LABEL[s.signing.key])
                : s.signing.key,
            },
            {
              label: t("maintenance.update.status.source"),
              value: s.source.mirror
                ? t(
                    s.source.fell_back
                      ? "maintenance.update.status.mirrorFellBack"
                      : "maintenance.update.status.mirror",
                    {
                      fallback: FALLBACK_LABEL[s.source.fallback]
                        ? t(FALLBACK_LABEL[s.source.fallback])
                        : s.source.fallback,
                    },
                  )
                : "GitHub",
            },
          ]}
        />
        <SystemsTable systems={s.systems} />
      </CardContent>
    </Card>
  );
}
