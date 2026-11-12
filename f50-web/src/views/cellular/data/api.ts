import { f50 } from "@/lib/ubus";
import type {
  DataPowerReply,
  DataSetReply,
  DataSettings,
  DataStatus,
} from "./types";

export const dataKeys = { get: ["cellular", "data"] as const };

export const getData = () => f50<DataStatus>("f50.data", "get");
export const setData = (s: DataSettings) =>
  f50<DataSetReply>("f50.data", "set", { ...s });
export const setDataPower = (on: boolean) =>
  f50<DataPowerReply>("f50.data", "power", { state: on ? "on" : "off" });
