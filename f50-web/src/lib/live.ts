/**
 * `f50.live` over uhttpd's `/ubus/subscribe/<object>`: an event stream that f50d fills while somebody listens.
 *
 * `EventSource` cannot send headers and uhttpd takes the rpcd session only as `Authorization: Bearer <session>`, so
 * the stream is read with `fetch`. Each event is named after a topic (`status`, `system`, `modem`, `cells`, `sms`)
 * and carries the envelope of the read method behind it, the same `{ ok, data }` a call returns.
 *
 * uhttpd answers a refused subscription (an unknown session, or one whose login predates the `:subscribe` grant)
 * with a JSON body instead of a stream: that is `denied`, and the caller falls back to polling.
 */
import { t } from "@/i18n";

const BASE = import.meta.env.VITE_API_BASE_URL ?? "/ubus";

export type LiveTopic = "status" | "system" | "modem" | "cells" | "sms";

export interface LiveEvent {
  topic: string;
  /** The envelope: `{ ok: true, data }` or `{ ok: false, code, error }`. */
  payload: unknown;
}

/** Whole events in `buffer` and what is left of an unfinished one (`text/event-stream`: blank-line separated). */
export function parseEvents(buffer: string): {
  events: LiveEvent[];
  rest: string;
} {
  const text = buffer.replace(/\r\n?/g, "\n");
  const blocks = text.split("\n\n");
  const rest = blocks.pop() ?? "";
  const events: LiveEvent[] = [];
  for (const block of blocks) {
    let topic = "message";
    const data: string[] = [];
    for (const line of block.split("\n")) {
      if (!line || line.startsWith(":")) continue;
      const i = line.indexOf(":");
      const field = i < 0 ? line : line.slice(0, i);
      const value = i < 0 ? "" : line.slice(i + 1).replace(/^ /, "");
      if (field === "event") topic = value;
      else if (field === "data") data.push(value);
    }
    if (data.length === 0) continue;
    try {
      events.push({ topic, payload: JSON.parse(data.join("\n")) });
    } catch {
      // a malformed event is skipped; the next one carries newer figures
    }
  }
  return { events, rest };
}

export type StreamEnd =
  { kind: "closed" } | { kind: "denied" } | { kind: "error"; message: string };

/**
 * Reads the stream until it ends or `signal` aborts, calling `onEvent` per event and `onOpen` once it is a stream.
 * Never throws: how it ended is the result (an abort is `closed`).
 */
export async function readLive(
  session: string,
  signal: AbortSignal,
  onOpen: () => void,
  onEvent: (e: LiveEvent) => void,
): Promise<StreamEnd> {
  let res: Response;
  try {
    res = await fetch(`${BASE}/subscribe/f50.live`, {
      headers: {
        Authorization: `Bearer ${session}`,
        Accept: "text/event-stream",
      },
      signal,
      cache: "no-store",
    });
  } catch (e) {
    return signal.aborted
      ? { kind: "closed" }
      : {
          kind: "error",
          message: e instanceof Error ? e.message : t("app.live.network"),
        };
  }
  const type = res.headers.get("content-type") ?? "";
  if (!type.includes("text/event-stream")) {
    // uhttpd's refusal: {"code":-13,"message":"Permission denied"} (or a 4xx from an older uhttpd without it)
    await res.body?.cancel().catch(() => undefined);
    return res.ok || res.status === 403 || res.status === 404
      ? { kind: "denied" }
      : { kind: "error", message: `HTTP ${res.status}` };
  }
  if (!res.body) return { kind: "error", message: t("app.live.noStream") };
  onOpen();
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) return { kind: "closed" };
      buffer += decoder.decode(value, { stream: true });
      const { events, rest } = parseEvents(buffer);
      buffer = rest;
      for (const e of events) onEvent(e);
    }
  } catch (e) {
    return signal.aborted
      ? { kind: "closed" }
      : {
          kind: "error",
          message: e instanceof Error ? e.message : t("app.live.broken"),
        };
  } finally {
    reader.releaseLock();
  }
}
