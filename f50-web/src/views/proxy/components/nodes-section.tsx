import { useCallback } from "react";
import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { t } from "@/i18n";
import { formatAgo } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAtom, useAtomValue } from "jotai";
import {
  CircleAlertIcon,
  PlusIcon,
  RefreshCwIcon,
  ZapIcon,
} from "lucide-react";
import { NodeGroup } from "./node-group";
import { NodeRow } from "./node-row";
import { SubscriptionDialog } from "./subscription-dialog";
import { allNodes, subscriptionTitle, usageParts } from "../lib/format";
import {
  useProbe,
  useProxyStatus,
  useSetProxy,
  useUpdateSubscription,
} from "../queries";
import { foldedAtom, latencyAtom } from "../store";
import type { Profile, ProxyNode, Subscription } from "../types";

const ACTION = "text-muted-foreground hover:text-primary";

/** The subscriptions (one card each: usage, expiry, update, edit) and the local nodes; a click picks a node. */
export function NodesSection({ profile }: { profile: Profile }) {
  const [folded, setFolded] = useAtom(foldedAtom);
  const latency = useAtomValue(latencyAtom);
  const { data: status } = useProxyStatus();
  const set = useSetProxy();
  const { probe, probeAll } = useProbe();
  const running = status?.state === "running";
  const names = new Map(allNodes(profile).map((n) => [n.id, n.name]));
  const { mutate } = set;
  const onSelect = useCallback(
    (id: string) => mutate({ selected: id }),
    [mutate],
  );
  const onProbe = useCallback((id: string) => void probe(id), [probe]);

  if (!profile.subscriptions.length && !profile.local_nodes.length) {
    return (
      <Empty className="rounded-2xl bg-card">
        <EmptyHeader>
          <EmptyTitle>{t("proxy.nodes.emptyTitle")}</EmptyTitle>
          <EmptyDescription>{t("proxy.nodes.emptyBody")}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent className="flex-row justify-center">
          <Button asChild>
            <Link to="/proxy/node?type=subscription">
              <PlusIcon data-icon="inline-start" />
              {t("proxy.nodes.addSubscription")}
            </Link>
          </Button>
          <Button asChild variant="outline">
            <Link to="/proxy/node">{t("proxy.nodes.addNode")}</Link>
          </Button>
        </EmptyContent>
      </Empty>
    );
  }

  const fold = (key: string) => ({
    open: !folded[key],
    onOpenChange: (open: boolean) => setFolded((f) => ({ ...f, [key]: !open })),
  });

  const rows = (nodes: ProxyNode[]) => (
    <div className="divide-y md:grid md:grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] md:gap-2.5 md:divide-y-0">
      {nodes.map((n) => (
        <NodeRow
          key={n.id}
          node={n}
          selected={n.id === profile.selected}
          connected={running && n.id === status?.node?.id}
          latency={latency[n.id]}
          via={n.via ? names.get(n.via) : undefined}
          disabled={set.isPending}
          onSelect={onSelect}
          onProbe={onProbe}
        />
      ))}
    </div>
  );
  const testAll = (nodes: ProxyNode[]) =>
    void probeAll(nodes.filter((n) => !n.problem).map((n) => n.id));
  const testing = (nodes: ProxyNode[]) =>
    nodes.some((n) => latency[n.id] === "pending");

  return (
    <section
      aria-label={t("proxy.nodes.title")}
      className="flex flex-col gap-3 md:gap-6"
    >
      {profile.subscriptions.map((s) => (
        <NodeGroup
          key={s.id}
          title={subscriptionTitle(s)}
          description={<SubscriptionLine sub={s} />}
          {...fold(`sub:${s.id}`)}
          bare
          actions={
            <>
              <TestButton
                busy={testing(s.nodes)}
                onClick={() => testAll(s.nodes)}
                label={t("proxy.nodes.testAll")}
              />
              <UpdateButton sub={s} />
              <SubscriptionDialog sub={s} />
            </>
          }
        >
          {s.error && (
            <p className="flex items-start gap-1.5 border-b px-4 py-2 text-xs text-destructive md:mb-2 md:border-b-0 md:py-0">
              <CircleAlertIcon className="mt-px size-3.5 shrink-0" />
              <span className="min-w-0 break-words">
                {t("proxy.sub.lastError", { error: s.error })}
              </span>
            </p>
          )}
          {s.nodes.length ? (
            rows(s.nodes)
          ) : (
            <p className="px-4 py-3 text-sm text-muted-foreground">
              {t("proxy.sub.noNodes")}
            </p>
          )}
        </NodeGroup>
      ))}
      {profile.local_nodes.length > 0 && (
        <NodeGroup
          title={t("proxy.nodes.local")}
          description={t("proxy.nodes.localHint", {
            count: profile.local_nodes.length,
          })}
          {...fold("local")}
          bare
          actions={
            <TestButton
              busy={testing(profile.local_nodes)}
              onClick={() => testAll(profile.local_nodes)}
              label={t("proxy.nodes.testAll")}
            />
          }
        >
          {rows(profile.local_nodes)}
        </NodeGroup>
      )}
    </section>
  );
}

function SubscriptionLine({ sub }: { sub: Subscription }) {
  const usage = sub.usage && usageParts(sub.usage);
  const parts = [
    t("proxy.sub.count", { count: sub.nodes.length }),
    usage && t("proxy.sub.usage", { used: usage.used, total: usage.total }),
    usage &&
      (usage.expire
        ? t("proxy.sub.expires", { date: usage.expire })
        : t("proxy.sub.noExpiry")),
    sub.updated_at &&
      t("proxy.sub.updated", { ago: formatAgo(sub.updated_at) }),
  ].filter(Boolean);
  return <>{parts.join(" · ")}</>;
}

function TestButton({
  busy,
  onClick,
  label,
}: {
  busy: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      title={label}
      disabled={busy}
      aria-busy={busy}
      onClick={onClick}
      className={cn(ACTION, busy && "text-primary")}
    >
      <ZapIcon className={cn(busy && "animate-pulse")} />
    </Button>
  );
}

function UpdateButton({ sub }: { sub: Subscription }) {
  const update = useUpdateSubscription();
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={t("proxy.sub.update", { name: subscriptionTitle(sub) })}
      title={t("proxy.sub.updateShort")}
      disabled={update.isPending}
      aria-busy={update.isPending}
      onClick={() => update.mutate(sub.id)}
      className={cn(ACTION, update.isPending && "text-primary")}
    >
      <RefreshCwIcon className={cn(update.isPending && "animate-spin")} />
    </Button>
  );
}
