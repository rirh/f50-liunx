import { atom } from "jotai";

export const updateAvailableAtom = atom(false);
export const updatePendingAtom = atom(false);

/** 稍后提醒: no update notice before this time (Unix ms), kept across reloads. */
const SNOOZE_KEY = "f50.pageUpdate.snoozedUntil";

export function updateSnoozed(now = Date.now()): boolean {
  try {
    return now < Number(localStorage.getItem(SNOOZE_KEY) ?? 0);
  } catch {
    return false;
  }
}

/** Snoozes the notice until local midnight: later updates today stay quiet too. */
export function snoozeUpdateForToday(now = new Date()) {
  const midnight = new Date(now);
  midnight.setHours(24, 0, 0, 0);
  try {
    localStorage.setItem(SNOOZE_KEY, String(midnight.getTime()));
  } catch {
    // storage off: the notice closes for this page only
  }
}
