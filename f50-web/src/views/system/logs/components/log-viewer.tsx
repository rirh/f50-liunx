import { useLayoutEffect, useRef } from "react";
import { cn } from "@/lib/utils";

/** Monospace lines, newest at the bottom; follows the end while the reader is there. */
export function LogViewer({
  lines,
  empty,
}: {
  lines: string[];
  empty: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const atEnd = useRef(true);
  useLayoutEffect(() => {
    const el = box.current;
    if (el && atEnd.current) el.scrollTop = el.scrollHeight;
  }, [lines]);
  return (
    <div
      ref={box}
      onScroll={(e) => {
        const el = e.currentTarget;
        atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className="h-[65dvh] min-h-64 overflow-auto rounded-md border bg-muted/40 p-3"
      role="log"
      aria-live="off"
      tabIndex={0}
    >
      {lines.length === 0 ? (
        <p className="text-sm text-muted-foreground">{empty}</p>
      ) : (
        <pre className="font-mono text-xs leading-5 whitespace-pre-wrap break-all">
          {lines.map((l, i) => (
            <span
              key={i}
              className={cn(
                "block",
                l.startsWith("--- ") &&
                  "mt-2 font-semibold text-foreground first:mt-0",
              )}
            >
              {l || " "}
            </span>
          ))}
        </pre>
      )}
    </div>
  );
}
