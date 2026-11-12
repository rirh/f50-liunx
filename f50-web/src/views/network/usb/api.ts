import { f50 } from "@/lib/ubus";
import type {
  AdapterAddResult,
  AdbArgs,
  AdbKeyResult,
  AdbResult,
  UsbAdapters,
  UsbInfo,
  UsbSetArgs,
  UsbSetResult,
} from "./types";

export const usbKeys = {
  all: ["f50.usb"] as const,
  info: ["f50.usb", "get"] as const,
  adapters: ["f50.usb", "adapters"] as const,
};

export const getUsb = () => f50<UsbInfo>("f50.usb", "get");
export const setUsb = (args: UsbSetArgs) =>
  f50<UsbSetResult>("f50.usb", "set", { ...args });
export const getAdapters = () => f50<UsbAdapters>("f50.usb", "adapters");
export const addAdapter = (iface: string) =>
  f50<AdapterAddResult>("f50.usb", "adapter_add", { iface });
export const setAdb = (args: AdbArgs) =>
  f50<AdbResult>("f50.usb", "adb", { ...args });
export const addAdbKey = (key: string) =>
  f50<AdbKeyResult>("f50.usb", "adb_key_add", { key });
