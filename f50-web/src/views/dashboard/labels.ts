import type { DataHold, DataPhase, Rat, SimState } from "./types";

export const RAT_LABEL: Record<Rat, string> = {
  gsm: "2G",
  umts: "3G",
  lte: "4G LTE",
  nsa: "5G NSA",
  sa: "5G SA",
};

export const SIM_STATE_LABEL: Record<SimState, string> = {
  absent: "未插卡",
  ready: "就绪",
  locked: "需要 PIN/PUK",
};

export const DATA_PHASE_LABEL: Record<DataPhase, string> = {
  down: "未连接",
  connecting: "连接中",
  connected: "已连接",
  held: "已暂停",
  failed: "连接失败",
};

export const DATA_HOLD_LABEL: Record<DataHold, string> = {
  roaming: "漫游中且未允许数据漫游",
  "sim-missing": "未插 SIM 卡",
  "radio-off": "射频已关闭",
};

export const SECURITY_LABEL: Record<string, string> = {
  wpa2: "WPA2",
  "wpa2-wpa3": "WPA2/WPA3",
  wpa3: "WPA3",
  open: "开放",
  other: "其他",
};

export const UPLINK_MODE_LABEL: Record<string, string> = {
  off: "未使用",
  client: "客户端",
  hotspot: "热点模式",
};

export const USB_ROLE_LABEL: Record<string, string> = {
  host: "主机",
  device: "设备",
  unavailable: "不可用",
};
