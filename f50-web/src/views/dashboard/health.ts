/** What on the device needs a look, as short phrases for the page header; empty when all is well. */
import { t } from "@/i18n";
import type { DashboardStatus } from "./types";

export interface Issue {
  text: string;
  /** Red (broken) rather than amber (worth a look). */
  bad: boolean;
}

export function issuesOf(s: DashboardStatus): Issue[] {
  const out: Issue[] = [];
  const add = (text: string, bad = false) => out.push({ text, bad });
  const modem = s.modem?.ok ? s.modem.data : null;
  const data = s.data?.ok ? s.data.data : null;
  const traffic = s.traffic?.ok ? s.traffic.data : null;
  const system = s.system?.ok ? s.system.data : null;
  const update = s.update?.ok ? s.update.data : null;

  if (modem) {
    if (!modem.radio_allowed) add(t("dashboard.hero.radioOff"), true);
    else if (!modem.registered) add(t("dashboard.hero.unregistered"), true);
    if (modem.roaming) add(t("dashboard.hero.roaming"));
  } else if (s.modem) add(t("dashboard.health.modem"), true);
  if (data?.state === "failed") add(t("dashboard.health.dataFailed"), true);
  else if (data?.state === "down") add(t("dashboard.health.dataOff"));
  if (traffic?.cap.cut_active) add(t("dashboard.hero.cutActive"), true);
  else if (traffic?.cap.level === "over")
    add(t("dashboard.hero.level.over"), true);
  else if (traffic?.cap.level === "warn") add(t("dashboard.hero.level.warn"));
  if (system?.heat_alarm) add(t("dashboard.hero.hot"), true);
  else {
    const temp = system?.temperatures.hottest ?? system?.temperatures.cpu;
    if (temp != null && temp >= 70) add(t("dashboard.health.warm"));
  }
  if (update?.restart_pending) add(t("dashboard.update.restartPending"));
  else if (update?.notice) add(t("dashboard.update.available"));
  return out;
}
