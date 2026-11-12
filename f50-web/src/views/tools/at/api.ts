import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type { AtHistoryEntry, AtReply, AtSendArgs } from "./types";

export const atKeys = { history: ["at", "history"] as const };

/** One guarded AT command: the reply, or `{job}` for a scan or a timeout over 15 s. */
export const atSend = (args: AtSendArgs) =>
  f50<AtReply | JobStarted>("f50.at", "send", { ...args });

/** The last 40 commands of this boot, oldest first. */
export const atHistory = () =>
  f50<{ history: AtHistoryEntry[] }>("f50.at", "history");
