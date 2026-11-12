import { useCallback, useState } from "react";
import { JobProgress } from "@/components/f50/job-progress";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import type { JobStatus } from "@/lib/jobs";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { toast } from "sonner";
import { clientKeys, getClientStatus } from "./api";
import { JoinDialog, type JoinTarget } from "./components/join-dialog";
import { ScanCard } from "./components/scan-card";
import { ClientStatusCard } from "./components/status-card";
import type { ScanNetwork } from "./types";

interface WifiJob {
  id: string;
  title: string;
  running: boolean;
}

export default function WifiClientPage() {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: clientKeys.status,
    queryFn: getClientStatus,
    refetchInterval: 10_000,
  });
  const [job, setJob] = useState<WifiJob | null>(null);
  const [target, setTarget] = useState<JoinTarget | null>(null);
  const busy = job?.running ?? false;

  const onJoin = useCallback(
    (n: ScanNetwork | null) =>
      setTarget(
        n
          ? { ssid: n.ssid, open: n.security === "open" }
          : { ssid: "", open: false },
      ),
    [],
  );
  const onJobDone = (j: JobStatus) => {
    setJob((cur) =>
      cur && cur.id === j.id ? { ...cur, running: false } : cur,
    );
    if (j.state === "done")
      toast.success(
        j.method.endsWith("join") ? "已加入网络" : "已断开，热点正在恢复",
      );
    qc.invalidateQueries({ queryKey: clientKeys.status });
    qc.invalidateQueries({ queryKey: ["f50.wifi"] });
  };

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Wi-Fi 上网"
        description="把其他 Wi-Fi 网络作为上网线路（优先于移动数据）"
        actions={
          <PendingButton
            variant="outline"
            size="sm"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {query.isFetching ? "刷新中" : "刷新"}
          </PendingButton>
        }
      />
      {job && (
        <JobProgress
          key={job.id}
          id={job.id}
          title={job.title}
          onDone={onJobDone}
        />
      )}
      <QueryState query={query} skeletonRows={5}>
        {(status) => (
          <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
            <ClientStatusCard
              status={status}
              busy={busy}
              onLeaveStarted={(id) =>
                setJob({ id, title: "断开网络", running: true })
              }
            />
            <ScanCard disabled={busy} onJoin={onJoin} />
            <JoinDialog
              target={target}
              hotspotOn={status.mode === "hotspot"}
              onOpenChange={(o) => !o && setTarget(null)}
              onStarted={(id, ssid) => {
                setTarget(null);
                setJob({ id, title: `加入 ${ssid}`, running: true });
              }}
            />
          </div>
        )}
      </QueryState>
    </div>
  );
}
