import { Link } from "react-router-dom";
import { PageHeader } from "@/components/f50/page-header";
import { PendingButton } from "@/components/f50/pending-button";
import { QueryState } from "@/components/f50/query-state";
import { Button } from "@/components/ui/button";
import { t } from "@/i18n";
import { PlusIcon, RefreshCwIcon } from "lucide-react";
import { ConnectHero } from "./components/connect-hero";
import { GroupsStatus } from "./components/groups-status";
import { NodesSection } from "./components/nodes-section";
import { useProfile, useProxyStatus, useUpdateSubscription } from "./queries";

/**
 * 代理: one strip on top (switch, node, live traffic, 分流 mode and config), the 策略组 of the config in effect, then
 * the nodes across the full width.
 */
export default function ProxyPage() {
  const profile = useProfile();
  const status = useProxyStatus();
  const update = useUpdateSubscription();
  const subs = profile.data?.subscriptions.length ?? 0;
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("proxy.page.title")}
        actions={
          <>
            {subs > 0 && (
              <PendingButton
                variant="outline"
                size="sm"
                pending={update.isPending}
                onClick={() => update.mutate("all")}
              >
                <RefreshCwIcon data-icon="inline-start" />
                {t("proxy.page.updateAll")}
              </PendingButton>
            )}
            <Button asChild size="sm">
              <Link to="/proxy/node">
                <PlusIcon data-icon="inline-start" />
                {t("common.action.add")}
              </Link>
            </Button>
          </>
        }
      />
      <QueryState query={profile} skeletonRows={8}>
        {(p) => (
          <div className="flex min-w-0 flex-col gap-4 md:gap-5">
            <ConnectHero profile={p} />
            <GroupsStatus
              groups={status.data?.groups ?? []}
              routing={status.data?.routing ?? p.routing}
            />
            <NodesSection profile={p} />
          </div>
        )}
      </QueryState>
    </div>
  );
}
