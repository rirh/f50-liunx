import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type {
  ModemCells,
  ModemIdentity,
  ModemStatus,
  RadioReply,
} from "./types";

export const signalKeys = {
  all: ["cellular", "modem"] as const,
  status: ["cellular", "modem", "status"] as const,
  cells: ["cellular", "modem", "cells"] as const,
};

export const getStatus = () => f50<ModemStatus>("f50.modem", "status");
export const getCells = () => f50<ModemCells>("f50.modem", "cells");
export const getIdentity = () => f50<ModemIdentity>("f50.modem", "identity");
export const setRadio = (on: boolean) =>
  f50<RadioReply>("f50.modem", "radio", { state: on ? "on" : "off" });
export const restartModem = () => f50<JobStarted>("f50.modem", "restart");
