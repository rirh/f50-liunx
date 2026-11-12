import { atom } from "jotai";
import type { ActiveJob } from "../types";

/** The update job the page shows (started here, or found running on load); kept after it ends to show the outcome. */
export const activeJobAtom = atom<ActiveJob | null>(null);
