import { atom } from "jotai";
import type { ListFilter } from "../types";

/** The open conversation (its peer); null shows the list alone on a phone. */
export const selectedPeerAtom = atom<string | null>(null);
export const searchAtom = atom("");
export const filterAtom = atom<ListFilter>("all");
/** The new-message dialog. */
export const composeOpenAtom = atom(false);
