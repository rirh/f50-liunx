import { format } from "date-fns";
import type { OutputEntry } from "./types";

/** The transcript as plain text, for the clipboard. */
export function asText(entries: OutputEntry[]) {
  return entries
    .flatMap((e) => {
      if (e.kind === "command")
        return e.lines.map((l) => `[${format(e.time, "HH:mm:ss")}] > ${l}`);
      const tail = e.status ? [e.status] : [];
      return [...e.lines, ...tail];
    })
    .join("\n");
}
