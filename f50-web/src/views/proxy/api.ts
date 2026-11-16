/**
 * `f50.proxy` (f50-proxy/DESIGN.md §2): the proxy's status, its profile (subscriptions, nodes, configs, settings),
 * and the engine's connections and logs. `subscription_add` / `subscription_update` start jobs; {@link runJob}
 * follows one to its end.
 *
 * Without a device, `VITE_PROXY_MOCK=1 pnpm dev` answers every call from `./mock` (dev builds only).
 */
import {
  isJobStarted,
  JOB_STATE_LABEL,
  type JobStarted,
  jobStatus,
} from "@/lib/jobs";
import { ApiError, f50 } from "@/lib/ubus";
import type {
  Config,
  Connections,
  GroupStatus,
  InspectedNode,
  LogPage,
  ProbeMethod,
  Profile,
  ProxyNode,
  ProxySettings,
  ProxyStatus,
  RulesInfo,
  SetArgs,
  SettingsArgs,
} from "./types";

const OBJ = "f50.proxy";
const MOCK = import.meta.env.DEV && import.meta.env.VITE_PROXY_MOCK === "1";

async function call<T>(
  method: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (MOCK) return (await import("./mock")).mockCall<T>(method, args);
  return f50<T>(OBJ, method, args);
}

export const proxyKeys = {
  all: ["f50.proxy"] as const,
  status: ["f50.proxy", "status"] as const,
  profile: ["f50.proxy", "profile"] as const,
  connections: ["f50.proxy", "connections"] as const,
  logs: ["f50.proxy", "logs"] as const,
};

export const getStatus = () => call<ProxyStatus>("status");
export const getProfile = () => call<Profile>("profile");
export const setProxy = (args: SetArgs) =>
  call<ProxyStatus>("set", { ...args });
export const setSettings = (args: SettingsArgs) =>
  call<ProxySettings>("settings_set", { ...args });

export const addSubscription = (url: string, name?: string) =>
  call<JobStarted>("subscription_add", { url, ...(name ? { name } : {}) });
/** `id` or `"all"`. */
export const updateSubscription = (id: string) =>
  call<JobStarted>("subscription_update", { id });
export const editSubscription = (id: string, name?: string, url?: string) =>
  call<Profile>("subscription_set", {
    id,
    ...(name !== undefined ? { name } : {}),
    ...(url !== undefined ? { url } : {}),
  });
export const removeSubscription = (id: string) =>
  call<Profile>("subscription_remove", { id });

export const inspectNodes = (text: string) =>
  call<{ nodes: InspectedNode[] }>("node_inspect", { text });
/** No `id`: a new local node. `via` "" clears the 前置代理. */
export const saveNode = (args: {
  id?: string;
  link: string;
  name?: string;
  via?: string;
}) => call<ProxyNode>("node_save", { ...args });
export const deleteNode = (id: string) => call<Profile>("node_delete", { id });
export const probeNode = (id: string, method?: ProbeMethod) =>
  call<{ ms: number }>("probe", { id, ...(method ? { method } : {}) });

export const groupSelect = (group: string, node: string) =>
  call<GroupStatus[]>("group_select", { group, node });
export const groupTest = (group: string) =>
  call<GroupStatus[]>("group_test", { group });

export const saveConfig = (args: {
  id?: string;
  name: string;
  text: string;
  url?: string;
}) => call<Config>("config_save", { ...args });
export const deleteConfig = (id: string) =>
  call<Profile>("config_delete", { id });
export const fetchConfig = (url: string) =>
  call<{ text: string }>("config_fetch", { url });
export const checkRules = (text: string) =>
  call<RulesInfo>("rules_check", { text });

export const getConnections = async (): Promise<Connections> => {
  const c = await call<Connections>("connections");
  return { ...c, active: c.active ?? [], closed: c.closed ?? [] };
};
/** `0` closes every open connection. */
export const closeConnections = (id: number) =>
  call<{ closed: number }>("connections_close", { id });
export const getLogs = (epoch: string, after: number) =>
  call<LogPage>("logs", epoch ? { epoch, after } : {});

/** Follows a job a method started until it ends: its result, or an error with the job's reason. */
export async function runJob<T>(started: Promise<JobStarted>): Promise<T> {
  const reply = await started;
  if (!isJobStarted(reply)) throw new ApiError("failed", String(reply));
  if (MOCK) return (await import("./mock")).mockJob<T>(reply.job);
  for (;;) {
    const job = await jobStatus(reply.job);
    if (job.state === "done") return job.result as T;
    if (job.state !== "running")
      throw new ApiError(
        "failed",
        job.error?.message ?? JOB_STATE_LABEL[job.state],
        undefined,
        !!job.error?.message,
      );
    await new Promise((r) => window.setTimeout(r, 1000));
  }
}
