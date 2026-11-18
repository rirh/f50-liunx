import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type { CardReply, CardSlot, SimStatus } from "./types";

export const simKeys = { get: ["cellular", "sim"] as const };

export const getSim = () => f50<SimStatus>("f50.sim", "get");
export const selectSlot = (slot: 1 | 2) =>
  f50<JobStarted>("f50.sim", "select", { slot });
export const setCardRadio = (slot: CardSlot, on: boolean) =>
  f50<CardReply>("f50.sim", "card", { slot, radio: on ? "on" : "off" });
export const setImei = ({ slot, imei }: { slot: 1 | 2; imei: string }) =>
  f50<JobStarted>("f50.modem", "set_imei", { slot, imei });
