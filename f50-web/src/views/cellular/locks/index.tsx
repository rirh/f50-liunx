import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { t } from "@/i18n";
import { reportFormError } from "@/lib/form-error";
import { formatAgo, formatTime } from "@/lib/format";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import { getLocks, lockKeys } from "./api";
import { BandsCard } from "./components/bands-card";
import { CellCard } from "./components/cell-card";
import { ModeCard } from "./components/mode-card";
import { OptionsCard } from "./components/options-card";
import { ResetCard } from "./components/reset-card";
import type { LockStatus } from "./types";
import { JobSlot } from "../shared/job-slot";
import { useJobSlot } from "../shared/use-job-slot";
import { getSim, simKeys } from "../sim/api";

function Editors({
  s,
  disabled,
  onJob,
}: {
  s: LockStatus;
  disabled: boolean;
  onJob: (id: string, t: string) => void;
}) {
  const r = s.lock;
  return (
    <div className="flex flex-col gap-4">
      {r ? (
        <div key={r.ts} className="grid gap-4 lg:grid-cols-2">
          <ModeCard current={r.mode.label} disabled={disabled} onJob={onJob} />
          <OptionsCard
            endc={r.endc}
            autoApply={s.auto_apply}
            disabled={disabled}
            onJob={onJob}
          />
          <BandsCard
            rat="lte"
            reading={r.lte}
            supported={r.caps.lte}
            disabled={disabled}
            onJob={onJob}
          />
          <BandsCard
            rat="nr"
            reading={r.nr}
            supported={r.caps.nr}
            disabled={disabled}
            onJob={onJob}
          />
          <CellCard cells={r.cells} disabled={disabled} onJob={onJob} />
          <ResetCard last={s.reset} disabled={disabled} onJob={onJob} />
        </div>
      ) : (
        <>
          <Empty>
            <EmptyHeader>
              <EmptyTitle>
                {s.refreshing
                  ? t("cellular.locks.reading")
                  : t("cellular.locks.notRead")}
              </EmptyTitle>
              <EmptyDescription>
                {t("cellular.locks.readHint")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
          <div className="grid gap-4 lg:grid-cols-2">
            <ResetCard last={s.reset} disabled={disabled} onJob={onJob} />
          </div>
        </>
      )}
    </div>
  );
}

export default function LocksPage() {
  const qc = useQueryClient();
  const query = useQuery({
    queryKey: lockKeys.get,
    queryFn: () => getLocks(),
    refetchInterval: (q) =>
      q.state.data && (q.state.data.refreshing || q.state.data.applying)
        ? 2000
        : false,
  });
  const sim = useQuery({ queryKey: simKeys.get, queryFn: getSim });
  const { job, start } = useJobSlot();
  const refresh = useMutation({
    mutationFn: () => getLocks(true),
    onSuccess: (d) => qc.setQueryData(lockKeys.get, d),
    onError: (e) => reportFormError(e),
  });
  const s = query.data;
  const sim2 = sim.data?.active === 2;
  const running =
    s?.applying && s.applying !== job?.id
      ? { id: s.applying, title: t("cellular.locks.radioBusy") }
      : null;
  const disabled = sim2 || !!s?.applying;

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("cellular.locks.title")}
        description={t("cellular.locks.description")}
        actions={
          <>
            {s?.lock && (
              <Badge variant="outline" title={formatTime(s.lock.ts)}>
                {t("cellular.locks.readAt", { ago: formatAgo(s.lock.ts) })}
              </Badge>
            )}
            <PendingButton
              variant="outline"
              size="sm"
              pending={refresh.isPending || !!s?.refreshing}
              icon={<RefreshCwIcon data-icon="inline-start" />}
              onClick={() => refresh.mutate()}
            >
              {t("cellular.locks.reread")}
            </PendingButton>
          </>
        }
      />
      {sim2 && (
        <Alert>
          <AlertTitle>{t("cellular.locks.sim2Title")}</AlertTitle>
          <AlertDescription>
            {t("cellular.locks.sim2Description")}
          </AlertDescription>
        </Alert>
      )}
      <JobSlot
        job={job}
        onDone={() => {
          qc.invalidateQueries({ queryKey: lockKeys.get });
          qc.invalidateQueries({ queryKey: ["cellular", "modem"] });
        }}
      />
      <JobSlot
        job={running}
        onDone={() => qc.invalidateQueries({ queryKey: lockKeys.get })}
      />
      <QueryState query={query} skeletonRows={10}>
        {(d) => <Editors s={d} disabled={disabled} onJob={start} />}
      </QueryState>
    </div>
  );
}
