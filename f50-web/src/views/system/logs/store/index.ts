import { atom } from "jotai";
import type { LineCount } from "../types";

/** The log viewer's choices, kept while the user moves between pages. */
export const linesAtom = atom<LineCount>(300);
export const autoRefreshAtom = atom(false);
export const filterAtom = atom("");
