/** f50d's AT terminal speaks English; these are the messages it answers with, in Chinese. */
import type { Key } from "@/i18n";
import { t } from "@/i18n";
import type { AtClass } from "./types";

export const CLASS_LABEL = {
  scan: {
    title: "tools.at.class.scan.title",
    detail: "tools.at.class.scan.detail",
  },
  nv: { title: "tools.at.class.nv.title", detail: "tools.at.class.nv.detail" },
  radio: {
    title: "tools.at.class.radio.title",
    detail: "tools.at.class.radio.detail",
  },
  apn: {
    title: "tools.at.class.apn.title",
    detail: "tools.at.class.apn.detail",
  },
  sim: {
    title: "tools.at.class.sim.title",
    detail: "tools.at.class.sim.detail",
  },
} satisfies Record<AtClass, { title: Key; detail: Key }>;

/** The class named in `this command needs confirmation: nv`. */
export function classOf(message: string): AtClass | null {
  const m = /confirmation:\s*([a-z]+)/i.exec(message);
  const c = m?.[1]?.toLowerCase();
  return c && c in CLASS_LABEL ? (c as AtClass) : null;
}

const KNOWN: [RegExp, Key][] = [
  [/printable ASCII/i, "tools.at.error.ascii"],
  [/chaining/i, "tools.at.error.chaining"],
  [/wedges the AT port/i, "tools.at.error.wedges"],
  [/moves the SIM selection/i, "tools.at.error.simSelection"],
  [/did not answer in time/i, "tools.at.error.timeout"],
];

/** f50d's message in Chinese where it is one the terminal knows, else as it came. */
export function translate(message: string): string {
  const busy = /still running in the background \(([^)]+)\)/i.exec(message);
  if (busy) return t("tools.at.error.busy", { job: busy[1] });
  for (const [re, key] of KNOWN) if (re.test(message)) return t(key);
  return message;
}
