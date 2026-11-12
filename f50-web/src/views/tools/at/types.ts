/** `f50.at` reply shapes (crates/f50-at/src/terminal.rs). */

/** A command answered within the request, or the result of its job. */
export interface AtReply {
  reply_lines: string[];
  /** The final result line: `OK`, `ERROR`, `+CME ERROR: 3`, … */
  status: string;
}

export interface AtHistoryEntry {
  /** Unix seconds. */
  time: number;
  /** PIN and password arguments cut off (`AT+CPIN=...`). */
  command: string;
}

export interface AtSendArgs {
  command: string;
  confirm?: boolean;
  timeout?: number;
}

/** What a command needs confirming for (`guard::Class`). */
export type AtClass = "scan" | "nv" | "radio" | "apn" | "sim";

export type OutputKind = "command" | "reply" | "error" | "info";

/** One block of the output pane. */
export interface OutputEntry {
  id: number;
  /** Milliseconds. */
  time: number;
  kind: OutputKind;
  lines: string[];
  /** The final result line of a reply. */
  status?: string;
}
