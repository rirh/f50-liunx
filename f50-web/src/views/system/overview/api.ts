import { f50 } from "@/lib/ubus";
import type { Clients, OwnNumber, SystemInfo, SystemStatus } from "./types";

export const systemKeys = {
  info: ["f50.system", "info"] as const,
  status: ["f50.system", "status"] as const,
  clients: ["f50.system", "clients"] as const,
};

export const getInfo = () => f50<SystemInfo>("f50.system", "info");
export const getStatus = () => f50<SystemStatus>("f50.system", "status");
export const getClients = () => f50<Clients>("f50.system", "clients");
/** The SIM's own number: private (write access), fetched only when asked for. */
export const getOwnNumber = () => f50<OwnNumber>("f50.modem", "identity");
