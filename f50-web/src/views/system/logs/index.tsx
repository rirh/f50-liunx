import { useMemo } from "react";
import {
  Terminal,
  TerminalButton,
  TerminalPrompt,
  TerminalSelect,
} from "@/components/f50/terminal";
import { Skeleton } from "@/components/ui/skeleton";
import { useLiveState } from "@/hooks/use-live";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { useAtom } from "jotai";
import {
  CopyIcon,
  DownloadIcon,
  RefreshCwIcon,
  TimerResetIcon,
} from "lucide-react";
import { toast } from "sonner";
import { getLogs, getLogsMark, logsKey, logsMarkKey } from "./api";
import { LogLines } from "./components/log-viewer";
import { autoRefreshAtom, filterAtom, linesAtom } from "./store";
import { LINE_OPTIONS, type LineCount } from "./types";

function download(text: string) {
  const url = URL.createObjectURL(
    new Blob([text], { type: "text/plain;charset=utf-8" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = `f50-logs-${format(new Date(), "yyyyMMdd-HHmmss")}.txt`;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/**
 * dmesg and logread in one terminal that fills the page: line count, live state (or auto-refresh while the stream is
 * down), refresh, copy and download in its title bar, a `grep` prompt under the scrollback to filter (case-insensitive; section headers always stay).
 */
export default function Page() {
  const [lines, setLines] = useAtom(linesAtom);
  const [auto, setAuto] = useAtom(autoRefreshAtom);
  const [filter, setFilter] = useAtom(filterAtom);
  // the stream only counts once it has pushed a log fingerprint (an older f50d has no `logs` topic)
  const mark = useQuery({
    queryKey: logsMarkKey,
    queryFn: getLogsMark,
    enabled: false,
  });
  const live = useLiveState() === "live" && !!mark.data;
  const query = useQuery({
    queryKey: logsKey(lines),
    queryFn: () => getLogs(lines),
    // live: refetched when f50.live pushes a new log fingerprint (live-provider); else every 5 s when asked
    refetchInterval: !live && auto ? 5000 : false,
    placeholderData: (prev) => prev,
  });
  const all = useMemo(
    () => (query.data?.text ?? "").replace(/\n$/, "").split("\n"),
    [query.data],
  );
  const needle = filter.trim().toLowerCase();
  const shown = useMemo(
    () =>
      needle
        ? all.filter(
            (l) => l.startsWith("--- ") || l.toLowerCase().includes(needle),
          )
        : all,
    [all, needle],
  );
  const text = shown.join("\n");
  const hasText = !!query.data && shown.length > 0;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(t("maintenance.logs.copied", { count: shown.length }));
    } catch {
      toast.error(t("maintenance.logs.copyFailed"));
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Terminal
        flush
        className="flex-1"
        title={t("maintenance.logs.title")}
        status={
          query.isFetching
            ? t("maintenance.logs.card.reading")
            : t("maintenance.logs.card.description", { lines })
        }
        follow={text}
        actions={
          <>
            <TerminalSelect
              value={String(lines)}
              onChange={(e) => setLines(Number(e.target.value) as LineCount)}
              aria-label={t("maintenance.logs.toolbar.linesAria")}
            >
              {LINE_OPTIONS.map((n) => (
                <option key={n} value={n}>
                  {t("maintenance.logs.toolbar.lines", { count: n })}
                </option>
              ))}
            </TerminalSelect>
            {live ? (
              <span
                className="inline-flex h-7 items-center gap-1.5 px-2 font-sans text-xs text-[#3fb950]"
                title={t("maintenance.logs.liveHint")}
              >
                <span className="relative flex size-1.5">
                  <span className="absolute size-full animate-ping rounded-full bg-[#3fb950]/70 motion-reduce:animate-none" />
                  <span className="relative size-1.5 rounded-full bg-[#3fb950]" />
                </span>
                {t("app.live.live")}
              </span>
            ) : (
              <TerminalButton
                pressed={auto}
                onClick={() => setAuto(!auto)}
                title={t("maintenance.logs.toolbar.auto")}
              >
                <TimerResetIcon />
                {t("maintenance.logs.toolbar.auto")}
              </TerminalButton>
            )}
            <TerminalButton
              onClick={() => void query.refetch()}
              disabled={query.isFetching}
              aria-label={t("common.action.refresh")}
              title={t("common.action.refresh")}
            >
              <RefreshCwIcon
                className={cn(query.isFetching && "animate-spin")}
              />
            </TerminalButton>
            <TerminalButton
              onClick={() => void copy()}
              disabled={!hasText}
              aria-label={t("common.action.copy")}
              title={t("common.action.copy")}
            >
              <CopyIcon />
            </TerminalButton>
            <TerminalButton
              onClick={() => download(text)}
              disabled={!hasText}
              aria-label={t("common.action.download")}
              title={t("common.action.download")}
            >
              <DownloadIcon />
            </TerminalButton>
          </>
        }
        prompt={
          <TerminalPrompt
            prompt="grep"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder={t("maintenance.logs.toolbar.filterPlaceholder")}
            aria-label={t("maintenance.logs.toolbar.filterAria")}
            aside={
              needle
                ? t("maintenance.logs.matched", { count: shown.length })
                : undefined
            }
          />
        }
      >
        {query.isPending ? (
          <div className="flex flex-col gap-2">
            {Array.from({ length: 12 }, (_, i) => (
              <Skeleton key={i} className="h-4 bg-white/10" />
            ))}
          </div>
        ) : query.isError && !query.data ? (
          <p className="text-[#ff7b72]">{errorMessage(query.error)}</p>
        ) : (
          <LogLines
            lines={shown}
            empty={
              needle
                ? t("maintenance.logs.noMatch")
                : t("maintenance.logs.empty")
            }
          />
        )}
      </Terminal>
    </div>
  );
}
