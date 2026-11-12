import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import { formatAgo, formatTime } from "@/lib/format";
import { HardDriveDownloadIcon } from "lucide-react";
import { TextLink } from "./text-link";
import { Figure, Tile } from "./tile";
import type { Part, UpdateBrief } from "../types";

export function UpdateTile({ part }: { part: Part<UpdateBrief> | undefined }) {
  return (
    <Tile
      title={t("dashboard.update.title")}
      icon={HardDriveDownloadIcon}
      to="/system/update"
      part={part}
    >
      {(u) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure
              value={u.installed ?? t("dashboard.update.unknownVersion")}
              caption={t("dashboard.update.current")}
            />
            {u.job ? (
              <Badge>{t("dashboard.update.jobRunning")}</Badge>
            ) : u.notice ? (
              <Badge>{t("dashboard.update.available")}</Badge>
            ) : (
              <Badge variant="secondary">{t("dashboard.update.latest")}</Badge>
            )}
          </div>
          {u.restart_pending && (
            <Badge variant="outline">
              {t("dashboard.update.restartPending")}
            </Badge>
          )}
          {u.notice && (
            <p className="text-xs" title={formatTime(u.notice.at)}>
              {t("dashboard.update.found", {
                tag: u.notice.tag,
                ago: formatAgo(u.notice.at),
              })}
            </p>
          )}
          {(u.notice || u.job) && (
            <TextLink
              to="/system/update"
              className="self-start text-xs text-primary"
            >
              {u.job
                ? t("dashboard.update.progress")
                : t("dashboard.update.go")}
            </TextLink>
          )}
        </>
      )}
    </Tile>
  );
}
