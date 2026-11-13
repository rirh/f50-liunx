import { KvList } from "@/components/f50/kv-list";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import type { BootInfo } from "../types";

const yes = (b: boolean, on: string, off: string) => (
  <Badge variant={b ? "secondary" : "outline"}>{b ? on : off}</Badge>
);

export function BootSummary({ info }: { info: BootInfo }) {
  const linuxDefault = info.default_boot === "linux";
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("maintenance.boot.summary.title")}</CardTitle>
        <CardDescription>
          {t("maintenance.boot.summary.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <KvList
          items={[
            {
              label: t("maintenance.boot.summary.slot"),
              value: (
                <span className="tabular-nums">{info.slot.toUpperCase()}</span>
              ),
            },
            {
              label: t("maintenance.boot.summary.defaultBoot"),
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  <Badge variant={linuxDefault ? "secondary" : "outline"}>
                    {linuxDefault ? "Linux" : "Android"}
                  </Badge>
                  <span className="text-muted-foreground">
                    {linuxDefault
                      ? t("maintenance.boot.summary.linuxDefault")
                      : t("maintenance.boot.summary.androidDefault")}
                  </span>
                </span>
              ),
            },
            {
              label: t("maintenance.boot.summary.attempts"),
              value: t("maintenance.boot.summary.attemptsValue", {
                count: info.attempts,
              }),
            },
            {
              label: t("maintenance.boot.summary.locked"),
              value: (
                <span className="flex flex-wrap items-center gap-2">
                  {yes(
                    info.locked,
                    t("maintenance.boot.summary.lockedOn"),
                    t("maintenance.boot.summary.lockedOff"),
                  )}
                  {info.locked && (
                    <span className="text-muted-foreground">
                      {t("maintenance.boot.summary.lockedHint")}
                    </span>
                  )}
                </span>
              ),
            },
            {
              label: t("maintenance.boot.summary.sinceGood"),
              value:
                info.boots_since_good == null
                  ? "—"
                  : String(info.boots_since_good),
            },
            {
              label: t("maintenance.boot.summary.writable"),
              value: yes(
                info.writable,
                t("maintenance.boot.summary.writableOn"),
                t("maintenance.boot.summary.writableOff"),
              ),
            },
          ]}
        />
      </CardContent>
    </Card>
  );
}
