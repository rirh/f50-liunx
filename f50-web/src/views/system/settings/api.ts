import { ANONYMOUS_SESSION, f50, ubusCall } from "@/lib/ubus";
import type {
  ActionReply,
  HostnameReply,
  PasswordReply,
  SettingsInfo,
  TimezoneReply,
} from "./types";

/** The same key as the overview's `info`: one cache entry for both pages. */
export const infoKey = ["f50.system", "info"] as const;

export const getSettingsInfo = () => f50<SettingsInfo>("f50.system", "info");
export const setHostname = (name: string) =>
  f50<HostnameReply>("f50.system", "set_hostname", { name });
export const setTimezone = (zone: string) =>
  f50<TimezoneReply>("f50.system", "set_timezone", { zone });
export const setPassword = (password: string) =>
  f50<PasswordReply>("f50.system", "set_password", { password });
export const reboot = () => f50<ActionReply>("f50.system", "reboot");
export const poweroff = () => f50<ActionReply>("f50.system", "poweroff");

/**
 * Whether the device's ubus answers again after a reboot. Uses the anonymous session (the old login is gone with the
 * reboot) on rpcd's `session access`, which the unauthenticated ACL allows; any ubus answer means it is up, only a
 * transport failure or a timeout means it is not.
 */
export async function deviceAnswers(): Promise<boolean> {
  try {
    await ubusCall("session", "access", {}, ANONYMOUS_SESSION, { probe: true });
    return true;
  } catch (e) {
    const code = (e as { code?: string }).code;
    return code !== "transport" && code !== "timeout";
  }
}
