import {
  type ComponentProps,
  type ReactNode,
  forwardRef,
  useLayoutEffect,
  useRef,
} from "react";
import { cn } from "@/lib/utils";

/**
 * A command-line window, dark in both themes, without window decoration (it is part of the app, not a window in it): a title bar (title, a short status, actions), a scrollback that keeps
 * to the newest line while the reader is at the bottom, and an optional prompt line under it. Used by the system log
 * and meant for the AT terminal.
 */
export function Terminal({
  title,
  status,
  actions,
  prompt,
  follow,
  flush,
  children,
  className,
}: {
  title: ReactNode;
  /** A few words after the title (line count, "reading…"). */
  status?: ReactNode;
  /** Small controls on the right of the title bar ({@link TerminalButton}, {@link TerminalSelect}). */
  actions?: ReactNode;
  /** The line under the scrollback ({@link TerminalPrompt}). */
  prompt?: ReactNode;
  /** Changes whenever the content does, so the scrollback can keep to the end. */
  follow?: unknown;
  /** Edge to edge in the content area: no frame, no rounded corners, so it reads as part of the app. */
  flush?: boolean;
  children: ReactNode;
  className?: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const atEnd = useRef(true);
  useLayoutEffect(() => {
    const el = box.current;
    if (el && atEnd.current) el.scrollTop = el.scrollHeight;
  }, [follow]);
  return (
    <section
      className={cn(
        "flex min-h-0 flex-col overflow-hidden bg-[#0d1117] font-mono text-[12.5px] text-[#c9d1d9]",
        !flush &&
          "rounded-xl border border-black/20 shadow-sm dark:border-white/10",
        className,
      )}
    >
      <header className="flex min-h-10 shrink-0 flex-wrap items-center gap-x-3 gap-y-1.5 border-b border-white/10 bg-[#161b22] px-3 py-1.5">
        <span className="font-sans text-[13px] font-medium text-[#e6edf3]">
          {title}
        </span>
        {status && (
          <span className="truncate text-[11.5px] text-[#8b949e]">
            {status}
          </span>
        )}
        {actions && (
          <div className="ms-auto flex flex-wrap items-center gap-1">
            {actions}
          </div>
        )}
      </header>
      <div
        ref={box}
        onScroll={(e) => {
          const el = e.currentTarget;
          atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        role="log"
        aria-live="off"
        tabIndex={0}
        className="min-h-0 flex-1 overflow-auto px-3 py-2 leading-5 outline-none selection:bg-[#264f78] focus-visible:ring-1 focus-visible:ring-[#58a6ff]/50 focus-visible:ring-inset"
      >
        {children}
      </div>
      {prompt && (
        <div className="shrink-0 border-t border-white/10 bg-[#0d1117]">
          {prompt}
        </div>
      )}
    </section>
  );
}

/** A quiet button for the title bar: an icon, optionally a word; `pressed` for a toggle. */
export const TerminalButton = forwardRef<
  HTMLButtonElement,
  ComponentProps<"button"> & { pressed?: boolean }
>(function TerminalButton({ pressed, className, ...props }, ref) {
  return (
    <button
      ref={ref}
      type="button"
      aria-pressed={pressed}
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-md px-2 font-sans text-xs text-[#8b949e] transition-colors outline-none hover:bg-white/10 hover:text-[#e6edf3] focus-visible:ring-1 focus-visible:ring-[#58a6ff]/60 disabled:pointer-events-none disabled:opacity-40 [&_svg]:size-3.5 [&_svg]:shrink-0",
        pressed && "bg-[#1f6feb]/25 text-[#79c0ff] hover:bg-[#1f6feb]/35",
        className,
      )}
      {...props}
    />
  );
});

/** A native select styled for the title bar. */
export function TerminalSelect({
  className,
  ...props
}: ComponentProps<"select">) {
  return (
    <select
      className={cn(
        "h-7 rounded-md border border-white/10 bg-[#0d1117] px-1.5 font-sans text-xs text-[#c9d1d9] outline-none hover:border-white/20 focus-visible:ring-1 focus-visible:ring-[#58a6ff]/60",
        className,
      )}
      {...props}
    />
  );
}

/** The input line under the scrollback: a coloured prompt, then the text field. */
export const TerminalPrompt = forwardRef<
  HTMLInputElement,
  ComponentProps<"input"> & { prompt: string; aside?: ReactNode }
>(function TerminalPrompt({ prompt, aside, className, ...props }, ref) {
  return (
    <label className="flex h-10 items-center gap-2 px-3">
      <span className="shrink-0 text-[#3fb950]">{prompt}</span>
      <span aria-hidden className="shrink-0 text-[#8b949e]">
        ›
      </span>
      <input
        ref={ref}
        spellCheck={false}
        autoComplete="off"
        className={cn(
          "min-w-0 flex-1 bg-transparent text-[#e6edf3] caret-[#58a6ff] outline-none placeholder:text-[#6e7681]",
          className,
        )}
        {...props}
      />
      {aside && (
        <span className="shrink-0 font-sans text-[11.5px] text-[#8b949e]">
          {aside}
        </span>
      )}
    </label>
  );
});
