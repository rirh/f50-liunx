import { t } from "@/i18n";
import { ApiError } from "@/lib/ubus";
import type { Terminal } from "@xterm/xterm";
import {
  READ_WAIT_MS,
  base64ToBytes,
  shellClose,
  shellOpen,
  shellRead,
  shellResize,
  shellWrite,
  textToBase64,
} from "./api";

export type ShellState =
  | { phase: "connecting" }
  | { phase: "live"; shell: string }
  | { phase: "retrying"; shell: string }
  | { phase: "ended"; code: number | null }
  | { phase: "failed"; error: string };

/** The session this tab is attached to: a reload (or leaving the page and coming back) attaches to it again. */
const STORE_KEY = "f50-shell-session";
const RETRY_MS = 1500;
const DIM = "\x1b[2m";
const RESET = "\x1b[0m";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const isGone = (e: unknown) => e instanceof ApiError && e.code === "not-found";
/** The login is gone: ubus.ts has already gone back to the login page. */
const isLoggedOut = (e: unknown) =>
  e instanceof ApiError &&
  (e.code === "unauthorized" || e.code === "forbidden");

/**
 * One terminal attached to one `f50.shell` session: reads its output in a long-poll loop into xterm.js, and sends
 * what is typed in order (a keystroke typed while one is on its way goes with the next request).
 */
export class ShellLink {
  private id: string | null = null;
  private offset: number | null = null;
  private shell = "";
  private disposed = false;
  private ended = false;
  /** Bumped by every (re)start, so a read loop of an earlier session stops. */
  private generation = 0;
  private queued = "";
  private sending = false;
  private resizeTimer: ReturnType<typeof setTimeout> | undefined;

  private readonly term: Terminal;
  private readonly onState: (s: ShellState) => void;

  constructor(term: Terminal, onState: (s: ShellState) => void) {
    this.term = term;
    this.onState = onState;
  }

  /** Attaches to this tab's session if it is still there, else opens one. */
  async start(fresh = false) {
    const gen = ++this.generation;
    this.ended = false;
    this.queued = "";
    this.onState({ phase: "connecting" });
    const stored = fresh ? null : sessionStorage.getItem(STORE_KEY);
    if (stored) {
      try {
        // the whole scrollback, from its start: the screen as it was
        const first = await shellRead(stored, null, 0);
        if (gen !== this.generation) return;
        this.id = stored;
        this.shell = sessionStorage.getItem(`${STORE_KEY}-shell`) ?? "";
        this.offset = null;
        this.term.reset();
        this.take(first);
        if (!first.ended) {
          this.onState({ phase: "live", shell: this.shell });
          this.resize();
          void this.readLoop(gen);
        }
        return;
      } catch (e) {
        if (isLoggedOut(e) || gen !== this.generation) return;
        sessionStorage.removeItem(STORE_KEY);
      }
    }
    try {
      const opened = await shellOpen(
        Math.max(2, this.term.cols),
        Math.max(2, this.term.rows),
      );
      if (gen !== this.generation || this.disposed) {
        void shellClose(opened.id).catch(() => undefined);
        return;
      }
      this.id = opened.id;
      this.shell = opened.shell;
      this.offset = null;
      sessionStorage.setItem(STORE_KEY, opened.id);
      sessionStorage.setItem(`${STORE_KEY}-shell`, opened.shell);
      this.term.reset();
      this.onState({ phase: "live", shell: opened.shell });
      void this.readLoop(gen);
    } catch (e) {
      if (gen !== this.generation) return;
      this.onState({
        phase: "failed",
        error: e instanceof Error ? e.message : String(e),
      });
    }
  }

  /** Writes one read's output into the terminal; true when the session has ended. */
  private take(chunk: {
    data: string;
    offset: number;
    lost: number;
    ended: boolean;
    code: number | null;
  }) {
    if (chunk.lost > 0)
      this.term.write(
        `\r\n${DIM}[${t("tools.shell.note.skipped", { count: chunk.lost })}]${RESET}\r\n`,
      );
    if (chunk.data) this.term.write(base64ToBytes(chunk.data));
    this.offset = chunk.offset;
    if (chunk.ended) this.finish(chunk.code);
    return chunk.ended;
  }

  private finish(code: number | null, note = t("tools.shell.note.ended")) {
    if (this.ended) return;
    this.ended = true;
    sessionStorage.removeItem(STORE_KEY);
    this.term.write(`\r\n${DIM}[${note}]${RESET}\r\n`);
    this.onState({ phase: "ended", code });
  }

  private async readLoop(gen: number) {
    const id = this.id;
    if (!id) return;
    let failing = false;
    while (!this.disposed && gen === this.generation && !this.ended) {
      try {
        const chunk = await shellRead(id, this.offset, READ_WAIT_MS);
        if (this.disposed || gen !== this.generation) return;
        if (failing) {
          failing = false;
          this.onState({ phase: "live", shell: this.shell });
        }
        if (this.take(chunk)) return;
      } catch (e) {
        if (this.disposed || gen !== this.generation) return;
        if (isLoggedOut(e)) return;
        if (isGone(e)) {
          this.finish(null, t("tools.shell.note.closedElsewhere"));
          return;
        }
        if (!failing) {
          failing = true;
          this.onState({ phase: "retrying", shell: this.shell });
        }
        await sleep(RETRY_MS);
      }
    }
  }

  /** What xterm.js says was typed (or pasted). After the session ended, Enter starts a new one. */
  input(data: string) {
    if (this.ended) {
      if (data === "\r") void this.start(true);
      return;
    }
    if (!this.id) return;
    this.queued += data;
    void this.flush();
  }

  private async flush() {
    if (this.sending) return;
    this.sending = true;
    try {
      while (this.queued && this.id && !this.ended && !this.disposed) {
        const data = this.queued;
        this.queued = "";
        try {
          await shellWrite(this.id, textToBase64(data));
        } catch (e) {
          if (isGone(e))
            this.finish(null, t("tools.shell.note.closedElsewhere"));
          // other failures drop that input: the read loop reports the connection
        }
      }
    } finally {
      this.sending = false;
    }
  }

  /** Tells the shell the terminal's size (after the fit settles). */
  resize() {
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => {
      if (this.id && !this.ended)
        void shellResize(this.id, this.term.cols, this.term.rows).catch(
          () => undefined,
        );
    }, 120);
  }

  /** Hangs the session up. */
  async close() {
    const id = this.id;
    if (!id || this.ended) return;
    this.generation++;
    await shellClose(id).catch(() => undefined);
    this.finish(null);
  }

  /** The page goes away: stop reading, keep the session for the next visit (f50d hangs it up when unread). */
  dispose() {
    this.disposed = true;
    this.generation++;
    clearTimeout(this.resizeTimer);
  }
}
