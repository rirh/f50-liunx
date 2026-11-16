import { f50 } from "@/lib/ubus";

/**
 * `f50.shell`: a root login shell on a pseudo-terminal in f50d. Bytes cross as base64 both ways; `read` is a long
 * poll that answers as soon as there is output past `offset` (or after `wait` ms with none).
 */
export interface ShellOpened {
  id: string;
  shell: string;
  pid: number;
}

export interface ShellChunk {
  /** base64 of what the terminal printed from the offset asked for */
  data: string;
  /** the offset to ask for next */
  offset: number;
  /** bytes that scrolled out of f50d's scrollback before they were read */
  lost: number;
  ended: boolean;
  code: number | null;
}

/** The longest a read waits on the device (f50d allows 20 s; the HTTP request must stay well inside 30 s). */
export const READ_WAIT_MS = 15_000;

export const shellOpen = (cols: number, rows: number) =>
  f50<ShellOpened>("f50.shell", "open", { cols, rows });

export const shellRead = (id: string, offset: number | null, wait: number) =>
  f50<ShellChunk>("f50.shell", "read", {
    id,
    wait,
    ...(offset === null ? {} : { offset }),
  });

export const shellWrite = (id: string, data: string) =>
  f50<Record<string, never>>("f50.shell", "write", { id, data });

export const shellResize = (id: string, cols: number, rows: number) =>
  f50<Record<string, never>>("f50.shell", "resize", { id, cols, rows });

export const shellClose = (id: string) =>
  f50<Record<string, never>>("f50.shell", "close", { id });

const encoder = new TextEncoder();

/** UTF-8 text (what xterm.js types) as base64. */
export function textToBase64(text: string): string {
  return bytesToBase64(encoder.encode(text));
}

export function bytesToBase64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000)
    s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

export function base64ToBytes(b64: string): Uint8Array {
  const s = atob(b64);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}
