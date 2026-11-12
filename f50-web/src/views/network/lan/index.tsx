import { useState } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { ExternalLinkIcon, RefreshCwIcon } from "lucide-react";
import { getLan, lanKeys } from "./api";
import { LanSettingsForm } from "./components/settings-form";
import { LanStatusCard } from "./components/status-card";
import { lanDefaults } from "./schema";

export default function LanPage() {
  const query = useQuery({ queryKey: lanKeys.info, queryFn: getLan });
  /** The console's new address after the LAN moved: this page's own address no longer answers. */
  const [moved, setMoved] = useState<string | null>(null);
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("network.lan.title")}
        description={t("network.lan.description")}
        actions={
          <PendingButton
            variant="outline"
            size="sm"
            pending={query.isFetching}
            onClick={() => query.refetch()}
          >
            <RefreshCwIcon data-icon="inline-start" />
            {query.isFetching
              ? t("common.action.refreshing")
              : t("common.action.refresh")}
          </PendingButton>
        }
      />
      {moved && (
        <Alert>
          <AlertTitle>{t("network.lan.moved.title")}</AlertTitle>
          <AlertDescription className="flex flex-col items-start gap-2">
            <span>{t("network.lan.moved.description")}</span>
            <Button asChild size="sm" variant="outline">
              <a href={moved}>
                <ExternalLinkIcon data-icon="inline-start" />
                <span className="font-mono break-all">{moved}</span>
              </a>
            </Button>
          </AlertDescription>
        </Alert>
      )}
      <QueryState query={query} skeletonRows={6}>
        {(info) => (
          <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
            <LanStatusCard info={info} />
            <LanSettingsForm
              key={JSON.stringify(lanDefaults(info))}
              info={info}
              onMoved={setMoved}
            />
          </div>
        )}
      </QueryState>
    </div>
  );
}
