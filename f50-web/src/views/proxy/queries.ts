/** TanStack Query over `f50.proxy`: the reads the pages share and the changes, each with its toast. */
import { useEffect } from "react";
import { usePoll } from "@/hooks/use-live";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSetAtom, useStore } from "jotai";
import { toast } from "sonner";
import {
  addSubscription,
  closeConnections,
  deleteConfig,
  deleteNode,
  editSubscription,
  getConnections,
  getLocalSocks,
  getLogs,
  getProfile,
  getStatus,
  groupSelect,
  groupTest,
  probeNode,
  proxyKeys,
  removeSubscription,
  runJob,
  saveConfig,
  saveNode,
  setLocalSocks,
  setProxy,
  setSettings,
  updateSubscription,
} from "./api";
import { recordStatus } from "./lib/rates";
import { type Latency, latencyAtom } from "./store";
import type {
  Connections,
  GroupStatus,
  LogPage,
  Profile,
  ProxyStatus,
  SetArgs,
  SettingsArgs,
} from "./types";

/** The `proxy` live topic replaces this poll while the stream is up. */
const STATUS_MS = 2000;

const failed = (title: string) => (e: unknown) =>
  toast.error(title, { description: errorMessage(e) });

/** The proxy's state; `fast` polls every second whatever the stream does (the data page's speeds). */
export function useProxyStatus({ fast = false }: { fast?: boolean } = {}) {
  const poll = usePoll(STATUS_MS, "proxy");
  const query = useQuery({
    queryKey: proxyKeys.status,
    queryFn: getStatus,
    refetchInterval: fast ? 1000 : poll,
  });
  const { data, dataUpdatedAt } = query;
  useEffect(() => {
    if (data) recordStatus(data, dataUpdatedAt);
  }, [data, dataUpdatedAt]);
  return query;
}

export function useProfile() {
  return useQuery({ queryKey: proxyKeys.profile, queryFn: getProfile });
}

function useSetProfile() {
  const qc = useQueryClient();
  return (p: Profile) => qc.setQueryData(proxyKeys.profile, p);
}

function useRefresh() {
  const qc = useQueryClient();
  return () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: proxyKeys.profile }),
      qc.invalidateQueries({ queryKey: proxyKeys.status }),
    ]);
}

/** On/off, routing, node, active config: applied at once (the engine restarts with them). */
export function useSetProxy() {
  const qc = useQueryClient();
  return useMutation({
    mutationKey: ["f50.proxy", "set"],
    mutationFn: (args: SetArgs) => setProxy(args),
    onSuccess: (s, args) => {
      qc.setQueryData(proxyKeys.status, s);
      qc.setQueryData<Profile>(proxyKeys.profile, (p) =>
        p ? { ...p, ...args } : p,
      );
    },
    onError: failed(t("proxy.toast.setFailed")),
  });
}

export function useSetSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: SettingsArgs) => setSettings(args),
    onSuccess: (settings) => {
      qc.setQueryData<Profile>(proxyKeys.profile, (p) =>
        p ? { ...p, settings } : p,
      );
      toast.success(t("proxy.toast.saved"));
    },
    onError: failed(t("proxy.toast.saveFailed")),
  });
}

/** Delay tests: one real request through each node; results go to `latencyAtom`. */
export function useProbe() {
  const store = useStore();
  const setLatency = useSetAtom(latencyAtom);
  const measure = async (id: string, previous: Latency | undefined) => {
    try {
      const { ms } = await probeNode(id);
      setLatency((l) => ({ ...l, [id]: ms }));
    } catch (error) {
      if (error instanceof ApiError && error.code === "busy") {
        setLatency((l) => {
          if (l[id] !== "pending") return l;
          const next = { ...l };
          if (previous === undefined) delete next[id];
          else next[id] = previous;
          return next;
        });
        toast.error(errorMessage(error), { id: "proxy-probe-busy" });
        return;
      }
      setLatency((l) => ({ ...l, [id]: "fail" }));
    }
  };
  const probe = async (id: string) => {
    const previous = store.get(latencyAtom)[id];
    if (previous === "pending") return;
    setLatency((l) => ({ ...l, [id]: "pending" }));
    await measure(id, previous);
  };
  // a few at a time: the router answers each with a real request through the node
  const probeAll = async (ids: string[]) => {
    const previous = store.get(latencyAtom);
    const queue = ids.filter((id) => previous[id] !== "pending");
    setLatency((l) => ({
      ...l,
      ...Object.fromEntries(queue.map((id) => [id, "pending" as const])),
    }));
    const worker = async () => {
      for (let id = queue.shift(); id; id = queue.shift())
        await measure(id, previous[id]);
    };
    await Promise.all(Array.from({ length: 4 }, worker));
  };
  return { probe, probeAll };
}

/** group_select / group_test answer with the whole list; shown before the next status. */
function useSetGroups() {
  const qc = useQueryClient();
  return (groups: GroupStatus[]) =>
    qc.setQueryData<ProxyStatus>(proxyKeys.status, (s) =>
      s ? { ...s, groups } : s,
    );
}

export function useGroupSelect() {
  const setGroups = useSetGroups();
  return useMutation({
    mutationFn: ({ group, node }: { group: string; node: string }) =>
      groupSelect(group, node),
    onSuccess: setGroups,
    onError: failed(t("proxy.toast.switchFailed")),
  });
}

export function useGroupTest() {
  const setGroups = useSetGroups();
  return useMutation({
    mutationFn: (group: string) => groupTest(group),
    onSuccess: setGroups,
    onError: failed(t("proxy.toast.testFailed")),
  });
}

export function useAddSubscription() {
  const refresh = useRefresh();
  const setLatency = useSetAtom(latencyAtom);
  return useMutation({
    mutationFn: ({ url, name }: { url: string; name?: string }) =>
      runJob<{ id: string; nodes: number }>(addSubscription(url, name)),
    onSuccess: (r) => {
      setLatency({});
      toast.success(t("proxy.toast.subscriptionAdded", { count: r.nodes }));
      return refresh();
    },
    onError: failed(t("proxy.toast.subscriptionFailed")),
  });
}

export function useUpdateSubscription() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) =>
      runJob<{ updated: number; failed: number }>(updateSubscription(id)),
    onSuccess: (r) => {
      if (r.failed)
        toast.warning(t("proxy.toast.subscriptionPartly", { ...r }));
      else toast.success(t("proxy.toast.subscriptionUpdated"));
    },
    onError: failed(t("proxy.toast.subscriptionFailed")),
    onSettled: refresh,
  });
}

export function useEditSubscription() {
  const setProfile = useSetProfile();
  return useMutation({
    mutationFn: ({
      id,
      name,
      url,
    }: {
      id: string;
      name: string;
      url: string;
    }) => editSubscription(id, name, url),
    onSuccess: (p) => {
      setProfile(p);
      toast.success(t("proxy.toast.saved"));
    },
    onError: failed(t("proxy.toast.saveFailed")),
  });
}

export function useRemoveSubscription() {
  const setProfile = useSetProfile();
  return useMutation({
    mutationFn: (id: string) => removeSubscription(id),
    onSuccess: (p) => {
      setProfile(p);
      toast.success(t("proxy.toast.deleted"));
    },
    onError: failed(t("proxy.toast.deleteFailed")),
  });
}

export function useSaveNode() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: saveNode,
    onSuccess: () => refresh(),
    onError: failed(t("proxy.toast.saveFailed")),
  });
}

export function useDeleteNode() {
  const setProfile = useSetProfile();
  return useMutation({
    mutationFn: (id: string) => deleteNode(id),
    onSuccess: (p) => {
      setProfile(p);
      toast.success(t("proxy.toast.deleted"));
    },
    onError: failed(t("proxy.toast.deleteFailed")),
  });
}

/** 本地 SOCKS5: how the ports run, while `enabled` (a node has one). */
export function useLocalSocks(enabled: boolean) {
  return useQuery({
    queryKey: proxyKeys.localSocks,
    queryFn: getLocalSocks,
    enabled,
    refetchInterval: 10_000,
  });
}

/** Applied at once, apart from the node form's save. */
export function useSetLocalSocks() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (a: { id: string; enabled: boolean; port?: number }) =>
      setLocalSocks(a.id, a.enabled, a.port),
    onSuccess: (r) => {
      qc.setQueryData(proxyKeys.localSocks, r.local_socks);
      return qc.invalidateQueries({ queryKey: proxyKeys.profile });
    },
    onError: failed(t("proxy.socks.failed")),
  });
}

export function useSaveConfig() {
  const refresh = useRefresh();
  return useMutation({
    mutationFn: saveConfig,
    onSuccess: () => refresh(),
    onError: failed(t("proxy.toast.saveFailed")),
  });
}

export function useDeleteConfig() {
  const setProfile = useSetProfile();
  const refresh = useRefresh();
  return useMutation({
    mutationFn: (id: string) => deleteConfig(id),
    onSuccess: (p) => {
      setProfile(p);
      toast.success(t("proxy.toast.deleted"));
      return refresh();
    },
    onError: failed(t("proxy.toast.deleteFailed")),
  });
}

/** A poll of the connections: when it was taken and each open connection's rate. */
export type ConnectionsSnapshot = Connections & {
  at: number;
  rates: Map<number, { up: number; down: number }>;
};

function connectionRates(
  prev: ConnectionsSnapshot | undefined,
  data: Connections,
  at: number,
) {
  const rates = new Map<number, { up: number; down: number }>();
  if (!prev || prev.epoch !== data.epoch || at <= prev.at) return rates;
  const dt = (at - prev.at) / 1000;
  const before = new Map(prev.active.map((c) => [c.id, c]));
  for (const c of data.active) {
    const p = before.get(c.id);
    if (p)
      rates.set(c.id, {
        up: Math.max(0, (c.up - p.up) / dt),
        down: Math.max(0, (c.down - p.down) / dt),
      });
  }
  return rates;
}

/** Open and recently closed connections, every second unless paused (and only while the page is visible). */
export function useConnections({ paused = false } = {}) {
  const qc = useQueryClient();
  return useQuery({
    queryKey: proxyKeys.connections,
    queryFn: async (): Promise<ConnectionsSnapshot> => {
      const prev = qc.getQueryData<ConnectionsSnapshot>(proxyKeys.connections);
      const data = await getConnections();
      const at = Date.now();
      return { ...data, at, rates: connectionRates(prev, data, at) };
    },
    refetchInterval: paused ? false : 1000,
  });
}

export function useCloseConnections() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => closeConnections(id),
    onSuccess: ({ closed }, id) => {
      if (id === 0)
        toast.success(
          closed
            ? t("proxy.data.conns.closedAll", { count: closed })
            : t("proxy.data.conns.noneToClose"),
        );
      return qc.invalidateQueries({ queryKey: proxyKeys.connections });
    },
    onError: failed(t("proxy.data.conns.closeFailed")),
  });
}

export const LOG_LINES_KEPT = 1000;

/** The engine log, fetched incrementally: each poll asks for the lines after the last one; a new epoch starts over. */
export function useLogs() {
  const qc = useQueryClient();
  return useQuery({
    queryKey: proxyKeys.logs,
    queryFn: async (): Promise<LogPage> => {
      const prev = qc.getQueryData<LogPage>(proxyKeys.logs);
      const page = await getLogs(prev?.epoch ?? "", prev?.next ?? 0);
      const lines =
        prev && prev.epoch === page.epoch
          ? [...prev.lines, ...page.lines]
          : page.lines;
      return { ...page, lines: lines.slice(-LOG_LINES_KEPT) };
    },
    refetchInterval: 1000,
  });
}
