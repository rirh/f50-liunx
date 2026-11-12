import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type {
  ClientStatus,
  JoinArgs,
  LeaveArgs,
  ScanNetwork,
  ScanReply,
} from "./types";

export const clientKeys = { status: ["f50.wificlient", "status"] as const };

export const getClientStatus = () =>
  f50<ClientStatus>("f50.wificlient", "status");
export const scanNetworks = () => f50<ScanReply>("f50.wificlient", "scan");
export const joinNetwork = (args: JoinArgs) =>
  f50<JobStarted>("f50.wificlient", "join", { ...args });
export const leaveNetwork = (args: LeaveArgs) =>
  f50<JobStarted>("f50.wificlient", "leave", { ...args });

/** The networks in a scan job's result (`{ networks }`), empty when it has none. */
export function networksOf(result: unknown): ScanNetwork[] {
  const n = (result as { networks?: unknown } | null)?.networks;
  return Array.isArray(n) ? (n as ScanNetwork[]) : [];
}
