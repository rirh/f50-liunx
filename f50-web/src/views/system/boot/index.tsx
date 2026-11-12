import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon, SearchIcon } from "lucide-react";
import { bootKey, getBoot } from "./api";
import { BootSummary } from "./components/boot-summary";
import { MiscCard } from "./components/misc-card";
import { StageCard } from "./components/stage-card";
import type { Stage } from "./types";

const STAGES: Stage[] = ["early", "vendor", "hardware", "post"];

export default function Page() {
  const query = useQuery({ queryKey: bootKey, queryFn: getBoot });
  const [filter, setFilter] = useState("");
  return (
    <>
      <PageHeader
        title={t("maintenance.boot.title")}
        description={t("maintenance.boot.description")}
        actions={
          <PendingButton
            size="sm"
            variant="outline"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {t("common.action.refresh")}
          </PendingButton>
        }
      />
      <QueryState query={query} skeletonRows={8}>
        {(info) => (
          <div className="flex flex-col gap-6">
            <div className="grid gap-6 lg:grid-cols-2">
              <BootSummary info={info} />
              <MiscCard misc={info.misc} slot={info.slot} />
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h2 className="text-base font-semibold">
                {t("maintenance.boot.stages")}
              </h2>
              <InputGroup className="w-full sm:w-64">
                <InputGroupInput
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder={t("maintenance.boot.filter.placeholder")}
                  aria-label={t("maintenance.boot.filter.aria")}
                />
                <InputGroupAddon>
                  <SearchIcon />
                </InputGroupAddon>
              </InputGroup>
            </div>
            {STAGES.map((s) => (
              <StageCard
                key={s}
                stage={s}
                report={info.stages.find((r) => r.stage === s)}
                filter={filter}
              />
            ))}
          </div>
        )}
      </QueryState>
    </>
  );
}
