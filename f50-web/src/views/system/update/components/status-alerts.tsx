import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { t } from "@/i18n";
import { formatTime } from "@/lib/format";
import { InfoIcon, TriangleAlertIcon } from "lucide-react";
import type { UpdateStatus } from "../types";

/** What needs attention: a pending restart, a half-done run, what recovery did, a lock held elsewhere. */
export function StatusAlerts({ status: s }: { status: UpdateStatus }) {
  return (
    <>
      {s.restart_pending && (
        <Alert>
          <InfoIcon />
          <AlertTitle>{t("maintenance.update.alerts.restartTitle")}</AlertTitle>
          <AlertDescription>
            {t("maintenance.update.alerts.restartBody")}
          </AlertDescription>
        </Alert>
      )}
      {s.unfinished && (
        <Alert variant="destructive">
          <TriangleAlertIcon />
          <AlertTitle>
            {t("maintenance.update.alerts.unfinishedTitle")}
          </AlertTitle>
          <AlertDescription>
            {t("maintenance.update.alerts.unfinishedBody")}
          </AlertDescription>
        </Alert>
      )}
      {s.recovery && s.recovery.lines.length > 0 && (
        <Alert>
          <InfoIcon />
          <AlertTitle>
            {t("maintenance.update.alerts.recoveryTitle", {
              time: formatTime(s.recovery.at),
            })}
          </AlertTitle>
          <AlertDescription>
            <ul className="flex list-disc flex-col gap-1 pl-4">
              {s.recovery.lines.map((l, i) => (
                <li key={i}>{l}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      )}
      {s.lock && !s.lock.this_daemon && (
        <Alert variant="destructive">
          <TriangleAlertIcon />
          <AlertTitle>{t("maintenance.update.alerts.lockTitle")}</AlertTitle>
          <AlertDescription>
            {s.lock.command
              ? t("maintenance.update.alerts.lockBodyCommand", {
                  pid: s.lock.pid,
                  command: s.lock.command,
                })
              : t("maintenance.update.alerts.lockBody", { pid: s.lock.pid })}
          </AlertDescription>
        </Alert>
      )}
    </>
  );
}
