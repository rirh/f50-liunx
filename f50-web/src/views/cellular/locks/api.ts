import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type { LockKind, LockSetReply, LockStatus } from "./types";

export const lockKeys = { get: ["cellular", "lock"] as const };

export const getLocks = (fresh = false) =>
  f50<LockStatus>("f50.lock", "get", fresh ? { fresh: true } : {});
export const setLock = (kind: LockKind, value: string) =>
  f50<LockSetReply>("f50.lock", "set", { kind, value });
export const resetLocks = () => f50<JobStarted>("f50.lock", "reset");
