import { useMemo, useState } from "react";
import {
  Terminal,
  TerminalButton,
  TerminalPrompt,
} from "@/components/f50/terminal";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Skeleton } from "@/components/ui/skeleton";
import { t } from "@/i18n";
import { copyText } from "@/lib/clipboard";
import { cn } from "@/lib/utils";
import {
  CheckIcon,
  CopyIcon,
  FilterIcon,
  PauseIcon,
  PlayIcon,
  Trash2Icon,
} from "lucide-react";
import { toast } from "sonner";
import { useLogs } from "../../queries";
import type { LogLevel, LogLine } from "../../types";

const LEVELS: LogLevel[] = ["error", "warning", "info", "debug"];
const LEVEL_TAG: Record<LogLevel, string> = {
  error: "ERROR",
  warning: "WARN ",
  info: "INFO ",
  debug: "DEBUG",
};
/** The terminal's palette (the system log's): errors red, warnings amber, info blue, debug dimmed. */
const LEVEL_TONE: Record<LogLevel, string> = {
  error: "text-[#ff7b72]",
  warning: "text-[#d29922]",
  info: "text-[#79c0ff]",
  debug: "text-[#6e7681]",
};

/** Seconds since the connection started, 4 digits (sing-box's INFO[0013] clock). */
const runClock = (ms: number) =>
  String(Math.max(0, Math.floor(ms / 1000))).padStart(4, "0");

/** Xray lines read "[session] module: message"; the engine's own "one-node-client: message". */
function parse(text: string) {
  const m = /^(?:\[(\d+)\] )?([\w./-]+): ([\s\S]*)$/.exec(text);
  return m ? { session: m[1], module: m[2], message: m[3] } : { message: text };
}

const levelOf = (l: LogLine): LogLevel =>
  LEVELS.includes(l.level) ? l.level : "info";

/**
 * 日志: the engine's log in the console's terminal (as the system log): level filter, pause, copy and clear in the
 * title bar, a `grep` prompt under the scrollback (a session number follows one connection).
 */
export function LogsView() {
  const { data, isPending } = useLogs();
  const [floor, setFloor] = useState({ epoch: "", seq: 0 });
  const [levels, setLevels] = useState<LogLevel[]>(LEVELS);
  const [query, setQuery] = useState("");
  // paused: the list stays as it was; polling goes on, so nothing is missed
  const [frozen, setFrozen] = useState<LogLine[] | null>(null);
  const [copied, setCopied] = useState(false);

  const epoch = data?.epoch ?? "";
  const fresh = useMemo(() => {
    const min = floor.epoch === epoch ? floor.seq : 0;
    return (data?.lines ?? []).filter((l) => l.seq > min);
  }, [data, floor, epoch]);
  const live = frozen ?? fresh;
  const lastFrozen = frozen?.at(-1)?.seq ?? 0;
  const waiting = frozen ? fresh.filter((l) => l.seq > lastFrozen).length : 0;

  const counts = useMemo(() => {
    const c: Record<LogLevel, number> = {
      error: 0,
      warning: 0,
      info: 0,
      debug: 0,
    };
    for (const l of live) c[levelOf(l)]++;
    return c;
  }, [live]);
  const q = query.trim().toLowerCase();
  const shown = useMemo(
    () =>
      live.filter(
        (l) =>
          levels.includes(levelOf(l)) &&
          (!q || l.text.toLowerCase().includes(q)),
      ),
    [live, levels, q],
  );
  const filtered = levels.length < LEVELS.length;

  const copy = async () => {
    const text = shown
      .map(
        (l) => `${LEVEL_TAG[levelOf(l)].trim()}[${runClock(l.run)}] ${l.text}`,
      )
      .join("\n");
    try {
      await copyText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
      toast.success(t("proxy.data.logs.copied", { count: shown.length }));
    } catch {
      toast.error(t("proxy.node.copyFailed"));
    }
  };

  return (
    <Terminal
      className="min-h-80 flex-1"
      title={t("proxy.data.view.logs")}
      status={frozen ? t("proxy.data.paused") : t("proxy.data.logs.hint")}
      follow={shown.at(-1)?.seq}
      actions={
        <>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <TerminalButton pressed={filtered}>
                <FilterIcon />
                {t("proxy.data.logs.level")}
                {counts.error > 0 && (
                  <span className="size-1.5 rounded-full bg-[#ff7b72]" />
                )}
              </TerminalButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
              {LEVELS.map((level) => (
                <DropdownMenuCheckboxItem
                  key={level}
                  checked={levels.includes(level)}
                  onCheckedChange={(on) =>
                    setLevels(
                      on
                        ? LEVELS.filter(
                            (l) => l === level || levels.includes(l),
                          )
                        : levels.filter((l) => l !== level),
                    )
                  }
                  // stays open, so several levels can be ticked in one go
                  onSelect={(e) => e.preventDefault()}
                >
                  <span className="flex-1">
                    {t(`proxy.data.logs.levels.${level}`)}
                  </span>
                  <span className="text-xs tabular-nums opacity-60">
                    {counts[level]}
                  </span>
                </DropdownMenuCheckboxItem>
              ))}
              <DropdownMenuSeparator />
              <DropdownMenuItem
                disabled={!filtered}
                onSelect={() => setLevels(LEVELS)}
              >
                {t("proxy.data.conns.showAll")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <TerminalButton
            pressed={frozen !== null}
            onClick={() => setFrozen(frozen ? null : fresh)}
          >
            {frozen ? <PlayIcon /> : <PauseIcon />}
            {frozen ? t("proxy.data.resume") : t("proxy.data.pause")}
            {waiting > 0 && (
              <span className="rounded-full bg-[#1f6feb] px-1.5 text-[10px] leading-4 font-semibold text-white tabular-nums">
                {waiting > 99 ? "99+" : waiting}
              </span>
            )}
          </TerminalButton>
          <TerminalButton
            aria-label={t("common.action.copy")}
            title={t("common.action.copy")}
            disabled={!shown.length}
            onClick={() => void copy()}
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
          </TerminalButton>
          <TerminalButton
            aria-label={t("common.action.clear")}
            title={t("common.action.clear")}
            disabled={!fresh.length}
            onClick={() => {
              setFloor({ epoch, seq: data?.next ?? 0 });
              setFrozen(null);
            }}
          >
            <Trash2Icon />
          </TerminalButton>
        </>
      }
      prompt={
        <TerminalPrompt
          prompt="grep"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("proxy.data.logs.search")}
          aria-label={t("common.action.search")}
          aside={
            q || filtered
              ? t("maintenance.logs.matched", { count: shown.length })
              : undefined
          }
        />
      }
    >
      {isPending ? (
        <div className="flex flex-col gap-2">
          {Array.from({ length: 12 }, (_, i) => (
            <Skeleton key={i} className="h-4 bg-white/10" />
          ))}
        </div>
      ) : shown.length === 0 ? (
        <p className="font-sans text-[#8b949e]">
          {q || filtered
            ? t("proxy.data.logs.noMatch")
            : t("proxy.data.logs.none")}
        </p>
      ) : (
        <pre className="font-mono whitespace-pre-wrap break-all">
          {shown.map((l) => {
            const { session, module, message } = parse(l.text);
            const level = levelOf(l);
            const loud = level === "error" || level === "warning";
            return (
              <span key={l.seq} className="block">
                <span className={LEVEL_TONE[level]}>{LEVEL_TAG[level]}</span>
                <span className="text-[#6e7681]">[{runClock(l.run)}]</span>{" "}
                {session && (
                  <>
                    <button
                      type="button"
                      onClick={() => setQuery(session)}
                      className="text-[#58a6ff] outline-none hover:underline focus-visible:underline"
                    >
                      [{session}]
                    </button>{" "}
                  </>
                )}
                {module && <span className="text-[#8b949e]">{module}: </span>}
                <span
                  className={cn(loud ? LEVEL_TONE[level] : "text-[#c9d1d9]")}
                >
                  {message}
                </span>
              </span>
            );
          })}
        </pre>
      )}
    </Terminal>
  );
}
