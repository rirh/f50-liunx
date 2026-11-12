import { ConfirmButton } from "@/components/f50/confirm-button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { rollback } from "../api";
import { useStartJob } from "../hooks";
import type { RollbackWhat, UpdateStatus } from "../types";

export function RollbackCard({
  status: s,
  busy,
}: {
  status: UpdateStatus;
  busy: boolean;
}) {
  const started = useStartJob();
  const run = useMutation({
    mutationFn: (what: RollbackWhat) => rollback(what),
    onSuccess: (r, what) =>
      started(
        t(
          what === "boot"
            ? "maintenance.update.rollback.boot"
            : "maintenance.update.rollback.system",
        ),
      )(r),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const kept = s.systems.filter((x) => x.previous);
  const bootPrev = s.boot.previous;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{t("maintenance.update.rollback.title")}</CardTitle>
        <CardDescription>
          {t("maintenance.update.rollback.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-wrap gap-2">
        <ConfirmButton
          size="sm"
          variant="outline"
          pending={run.isPending && run.variables === "system"}
          disabled={busy || run.isPending || kept.length === 0}
          title={t("maintenance.update.rollback.systemConfirm")}
          confirmText={t("maintenance.update.rollback.system")}
          description={
            <span className="flex flex-col gap-2">
              <span>
                {t("maintenance.update.rollback.willRestore", {
                  list:
                    kept
                      .map(
                        (x) =>
                          `${x.name} → ${x.previous?.release ?? t("maintenance.update.status.unknownRelease")}`,
                      )
                      .join(t("maintenance.update.rollback.separator")) ||
                    t("common.state.none"),
                })}
              </span>
              <span className="font-medium text-destructive">
                {t("maintenance.update.rollback.warning")}
              </span>
            </span>
          }
          onConfirm={() => run.mutate("system")}
        >
          {kept.length
            ? t("maintenance.update.rollback.system")
            : t("maintenance.update.rollback.noSystem")}
        </ConfirmButton>
        <ConfirmButton
          size="sm"
          variant="outline"
          pending={run.isPending && run.variables === "boot"}
          disabled={busy || run.isPending || !bootPrev?.usable}
          title={t("maintenance.update.rollback.bootConfirm")}
          confirmText={t("maintenance.update.rollback.boot")}
          description={
            <span className="flex flex-col gap-2">
              <span>
                {t("maintenance.update.rollback.bootRestore", {
                  release:
                    bootPrev?.release ??
                    t("maintenance.update.status.unknownRelease"),
                  kernel: bootPrev?.kernel ?? "—",
                })}
              </span>
              <span className="font-medium text-destructive">
                {t("maintenance.update.rollback.warning")}
              </span>
            </span>
          }
          onConfirm={() => run.mutate("boot")}
        >
          {bootPrev?.usable
            ? t("maintenance.update.rollback.boot")
            : t("maintenance.update.rollback.noBoot")}
        </ConfirmButton>
      </CardContent>
    </Card>
  );
}
