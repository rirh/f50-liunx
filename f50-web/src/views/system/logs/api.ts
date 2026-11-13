import { f50 } from "@/lib/ubus";
import type { LogsReply } from "./types";

export const logsKey = (lines: number) =>
  ["f50.system", "logs", lines] as const;
/** Every `logs` query, whatever its line count. */
export const logsAll = ["f50.system", "logs"] as const;
/** The fingerprint `f50.live` pushes (topic `logs`). */
export const logsMarkKey = ["f50.system", "logs_mark"] as const;

/** `lines`: 10-2000 per log. */
export const getLogs = (lines: number) =>
  f50<LogsReply>("f50.system", "logs", { lines });

/** `f50.system logs_mark`: the fingerprint, as the stream pushes it. */
export const getLogsMark = () =>
  f50<{ mark: string }>("f50.system", "logs_mark");
