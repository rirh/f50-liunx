/** `f50.system` write replies (f50-system/src/admin.rs). */

/** `set_hostname`: the name now set. */
export interface HostnameReply {
  hostname: string;
}

/** `set_timezone`: the zone now set (OpenWrt also gets the zone's POSIX TZ string; it is not echoed). */
export interface TimezoneReply {
  timezone: string;
}

/** `set_password`: whose password changed (always root). */
export interface PasswordReply {
  user: "root";
}

/** `reboot` / `poweroff`: an empty object; the action follows two seconds later. */
export type ActionReply = Record<string, never>;

/** The part of `f50.system info` this page shows. */
export interface SettingsInfo {
  hostname: string;
  timezone: string | null;
  time: number;
}
