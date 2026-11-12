import { f50 } from "@/lib/ubus";
import type { LanInfo, LanSetArgs, LanSetResult } from "./types";

export const lanKeys = { info: ["f50.lan", "get"] as const };

export const getLan = () => f50<LanInfo>("f50.lan", "get");
export const setLan = (args: LanSetArgs) =>
  f50<LanSetResult>("f50.lan", "set", { ...args });
