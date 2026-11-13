import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { t } from "@/i18n";
import { JobPanel } from "./components/job-panel";
import { OfflineCard } from "./components/offline-card";
import { ReleasesCard } from "./components/releases-card";
import { RollbackCard } from "./components/rollback-card";
import { StatusAlerts } from "./components/status-alerts";
import { StatusCard } from "./components/status-card";
import { useUpdateStatus } from "./hooks";

export default function UpdatePage() {
  const status = useUpdateStatus();
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("maintenance.update.title")}
        description={t("maintenance.update.description")}
      />
      <QueryState query={status} skeletonRows={6}>
        {(s) => {
          // one update at a time: a running job here, or the lock held by another updater
          const busy = !!s.job || (!!s.lock && !s.lock.this_daemon);
          return (
            <>
              <JobPanel status={s} />
              <StatusAlerts status={s} />
              {/* on a wide screen the update paths on the left, the installed systems on the right; on a phone one
                  column in order of use: releases, what runs now, a file, rollback */}
              <div className="flex flex-col gap-4 lg:grid lg:grid-cols-2 lg:items-start">
                <div className="contents lg:flex lg:flex-col lg:gap-4">
                  <div className="order-1 lg:order-none">
                    <ReleasesCard status={s} busy={busy} />
                  </div>
                  <div className="order-3 lg:order-none">
                    <OfflineCard status={s} busy={busy} />
                  </div>
                </div>
                <div className="contents lg:flex lg:flex-col lg:gap-4">
                  <div className="order-2 lg:order-none">
                    <StatusCard status={s} />
                  </div>
                  <div className="order-4 lg:order-none">
                    <RollbackCard status={s} busy={busy} />
                  </div>
                </div>
              </div>
            </>
          );
        }}
      </QueryState>
    </div>
  );
}
