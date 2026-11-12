import { f50 } from "@/lib/ubus";
import type { BootInfo } from "./types";

export const bootKey = ["f50.boot", "get"] as const;

export const getBoot = () => f50<BootInfo>("f50.boot", "get");
