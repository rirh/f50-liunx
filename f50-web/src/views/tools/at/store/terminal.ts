import { atom } from "jotai";
import type { OutputEntry } from "../types";

/** The terminal's output, kept while the app is open (navigating away and back keeps it). */
export const outputAtom = atom<OutputEntry[]>([]);

let next = 1;
/** Appends one block (at most 500 kept). */
export const appendOutputAtom = atom(
  null,
  (get, set, entry: Omit<OutputEntry, "id" | "time">) => {
    const all = [
      ...get(outputAtom),
      { ...entry, id: next++, time: Date.now() },
    ];
    set(outputAtom, all.length > 500 ? all.slice(-500) : all);
  },
);
