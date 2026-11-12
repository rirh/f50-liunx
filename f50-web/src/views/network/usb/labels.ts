import type { AdbState, UsbNetBound, UsbNetMode, UsbRole } from "./types";

export const ROLE_LABEL: Record<UsbRole, string> = {
  host: "主机（接 USB 网卡）",
  device: "设备（连接电脑）",
  none: "无",
  unavailable: "不支持",
};

export const NET_MODE_LABEL: Record<UsbNetMode, string> = {
  default: "默认（NCM）",
  ncm: "NCM",
  ecm: "ECM",
  rndis: "RNDIS",
  multi: "NCM + RNDIS",
};

export const NET_MODE_HINT: Record<UsbNetMode, string> = {
  default: "NCM：Linux、macOS 与新版 Windows 免驱",
  ncm: "Linux、macOS 与 Windows 11 免驱，速度最好",
  ecm: "兼容老旧 Linux 与部分 macOS",
  rndis: "老版本 Windows",
  multi: "同时提供 NCM 与 RNDIS",
};

export const NET_BOUND_LABEL: Record<UsbNetBound, string> = {
  ncm: "NCM",
  ecm: "ECM",
  rndis: "RNDIS",
  multi: "NCM + RNDIS",
  mixed: "混合（异常）",
  none: "无",
};

export const ADB_STATE_LABEL: Record<AdbState, string> = {
  off: "已关闭",
  unavailable: "不可用",
  open: "开放（无需授权）",
  keys: "仅授权密钥",
};
