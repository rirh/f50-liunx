import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
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
        title="系统更新"
        description="在线或离线更新系统与启动镜像，必要时回滚"
      />
      <QueryState query={status} skeletonRows={6}>
        {(s) => {
          // one update at a time: a running job here, or the lock held by another updater
          const busy = !!s.job || (!!s.lock && !s.lock.this_daemon);
          return (
            <>
              <JobPanel status={s} />
              <StatusAlerts status={s} />
              <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
                <div className="flex flex-col gap-4">
                  <ReleasesCard status={s} busy={busy} />
                  <OfflineCard status={s} busy={busy} />
                </div>
                <div className="flex flex-col gap-4">
                  <StatusCard status={s} />
                  <RollbackCard status={s} busy={busy} />
                </div>
              </div>
            </>
          );
        }}
      </QueryState>
    </div>
  );
}
