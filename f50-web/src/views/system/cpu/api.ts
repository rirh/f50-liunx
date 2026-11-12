import { f50 } from "@/lib/ubus";
import type { CpuProfile, CpuStatus } from "./types";

export const cpuKey = ["f50.cpu", "get"] as const;

export const getCpu = () => f50<CpuStatus>("f50.cpu", "get");
/** Applies and saves a profile; the reply is the new `get`. */
export const setProfile = (name: CpuProfile) =>
  f50<CpuStatus>("f50.cpu", "profile", { name });
