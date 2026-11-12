import { f50 } from "@/lib/ubus";
import type { LogsReply } from "./types";

export const logsKey = (lines: number) =>
  ["f50.system", "logs", lines] as const;

/** `lines`: 10-2000 per log. */
export const getLogs = (lines: number) =>
  f50<LogsReply>("f50.system", "logs", { lines });
