import { useEffect, useRef, useState } from "react";
import {
  Terminal as TerminalFrame,
  TerminalButton,
} from "@/components/f50/terminal";
import { t } from "@/i18n";
import { copyText } from "@/lib/clipboard";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { PlusIcon, PowerOffIcon } from "lucide-react";
import { ShellLink, type ShellState } from "./link";

/** The terminal's colours: the same dark window as the AT terminal and the system log. */
const THEME = {
  background: "#0d1117",
  foreground: "#c9d1d9",
  cursor: "#58a6ff",
  cursorAccent: "#0d1117",
  selectionBackground: "#264f78",
  black: "#484f58",
  red: "#ff7b72",
  green: "#3fb950",
  yellow: "#d29922",
  blue: "#58a6ff",
  magenta: "#bc8cff",
  cyan: "#39c5cf",
  white: "#b1bac4",
  brightBlack: "#6e7681",
  brightRed: "#ffa198",
  brightGreen: "#56d364",
  brightYellow: "#e3b341",
  brightBlue: "#79c0ff",
  brightMagenta: "#d2a8ff",
  brightCyan: "#56d4dd",
  brightWhite: "#f0f6fc",
};

function statusText(s: ShellState): string {
  switch (s.phase) {
    case "connecting":
      return t("tools.shell.status.connecting");
    case "live":
      return t("tools.shell.status.live", { shell: s.shell });
    case "retrying":
      return t("tools.shell.status.retrying");
    case "ended":
      return s.code === null
        ? t("tools.shell.status.endedPlain")
        : t("tools.shell.status.ended", { code: s.code });
    case "failed":
      return t("tools.shell.status.failed", { error: s.error });
  }
}

/**
 * A root shell on the device (`f50.shell`) in xterm.js, filling the content area like the AT terminal. The session
 * outlives the page: coming back (or reloading) in the same tab attaches to it again with its scrollback.
 */
export default function ShellPage() {
  const host = useRef<HTMLDivElement>(null);
  const link = useRef<ShellLink | null>(null);
  const [state, setState] = useState<ShellState>({ phase: "connecting" });

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const term = new Terminal({
      theme: THEME,
      fontFamily:
        getComputedStyle(document.documentElement)
          .getPropertyValue("--font-mono")
          .trim() || "monospace",
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 5000,
      macOptionIsMeta: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(el);
    fit.fit();

    const shell = new ShellLink(term, setState);
    link.current = shell;
    // Ctrl+Shift+C / Ctrl+Shift+V, as in a Linux terminal (Ctrl+C stays the interrupt). Copying goes through
    // copyText: over plain http there is no navigator.clipboard. Pasting is left to the browser: its paste event
    // works over http too, and xterm.js types what it carries (Cmd+V on a Mac is that already).
    term.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown" || !e.ctrlKey || !e.shiftKey || e.altKey)
        return true;
      const key = e.code || `Key${e.key.toUpperCase()}`;
      if (key === "KeyC") {
        const sel = term.getSelection();
        if (sel) void copyText(sel).catch(() => undefined);
        e.preventDefault();
        return false;
      }
      if (key === "KeyV") return false;
      return true;
    });
    const typed = term.onData((d) => shell.input(d));
    const binary = term.onBinary((d) => shell.input(d));
    const resized = term.onResize(() => shell.resize());
    const observer = new ResizeObserver(() => {
      if (el.clientWidth > 0 && el.clientHeight > 0) fit.fit();
    });
    observer.observe(el);
    void shell.start();
    // a keyboard is there: start typing at once (on a phone this would pop the keyboard up)
    if (matchMedia("(pointer: fine)").matches) term.focus();

    return () => {
      observer.disconnect();
      typed.dispose();
      binary.dispose();
      resized.dispose();
      shell.dispose();
      link.current = null;
      term.dispose();
    };
  }, []);

  const ended = state.phase === "ended" || state.phase === "failed";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TerminalFrame
        title={t("tools.shell.title")}
        status={statusText(state)}
        actions={
          ended ? (
            <TerminalButton onClick={() => void link.current?.start(true)}>
              <PlusIcon />
              {t("tools.shell.action.restart")}
            </TerminalButton>
          ) : (
            <TerminalButton
              disabled={state.phase === "connecting"}
              onClick={() => void link.current?.close()}
            >
              <PowerOffIcon />
              {t("tools.shell.action.close")}
            </TerminalButton>
          )
        }
        flush
        bare
        className="flex-1"
      >
        <div ref={host} className="h-full min-h-0 w-full" />
      </TerminalFrame>
    </div>
  );
}
