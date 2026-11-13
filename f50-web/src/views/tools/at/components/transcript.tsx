import { type ReactNode, useEffect, useState } from "react";
import { Spinner } from "@/components/ui/spinner";
import { useJob } from "@/hooks/use-job";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { type JobStatus, jobCancel } from "@/lib/jobs";
import { cn } from "@/lib/utils";
import { useMutation } from "@tanstack/react-query";
import { format } from "date-fns";
import { toast } from "sonner";
import type { RunningJob } from "./use-at-send";
import type { OutputEntry } from "../types";

const FAIL = /^(\+CM[ES] ERROR\b|ERROR\b|NO CARRIER\b|COMMAND NOT SUPPORT)/;
/** `+CSQ: 23,99`, `^ZCELLINFO: …`, `$MYNETINFO: …`: the response prefix is coloured. */
const PREFIX = /^([+^$*#][A-Z0-9_]+:)(.*)$/is;

const ms = (n: number) =>
  n < 1000 ? `${n} ms` : `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)} s`;

function Muted({ children }: { children: ReactNode }) {
  return (
    <span className="shrink-0 font-sans text-[11px] text-[#6e7681] tabular-nums select-none">
      {children}
    </span>
  );
}

function ReplyLine({ line }: { line: string }) {
  if (FAIL.test(line)) return <div className="text-[#ff7b72]">{line}</div>;
  const m = PREFIX.exec(line);
  if (!m) return <div>{line || " "}</div>;
  return (
    <div>
      <span className="text-[#79c0ff]">{m[1]}</span>
      {m[2]}
    </div>
  );
}

function Entry({ entry, first }: { entry: OutputEntry; first: boolean }) {
  if (entry.kind === "command") {
    return (
      <div className={cn("flex items-baseline gap-2", !first && "mt-3")}>
        <span className="text-[#3fb950] select-none">&gt;</span>
        <span className="min-w-0 flex-1 font-semibold text-[#e6edf3]">
          {entry.lines.join(" ")}
        </span>
        <Muted>
          <time title={format(entry.time, "yyyy-MM-dd HH:mm:ss")}>
            {format(entry.time, "HH:mm:ss")}
          </time>
        </Muted>
      </div>
    );
  }
  const duration = entry.ms !== undefined && <Muted>{ms(entry.ms)}</Muted>;
  if (entry.kind === "reply") {
    const ok = entry.status === "OK";
    return (
      <div>
        {entry.lines.map((l, i) => (
          <ReplyLine key={i} line={l} />
        ))}
        <div className="flex items-baseline gap-2">
          <span
            className={cn(
              "min-w-0 flex-1 font-semibold",
              entry.status
                ? ok
                  ? "text-[#3fb950]"
                  : "text-[#ff7b72]"
                : "text-[#8b949e]",
            )}
          >
            {entry.status ?? "—"}
          </span>
          {duration}
        </div>
      </div>
    );
  }
  return (
    <div className="flex items-baseline gap-2">
      <span
        className={cn(
          "min-w-0 flex-1",
          entry.kind === "error" ? "text-[#ff7b72]" : "text-[#8b949e]",
        )}
      >
        {entry.lines.join("\n")}
      </span>
      {duration}
    </div>
  );
}

/** Seconds since `since`, ticking. */
function Elapsed({ since }: { since: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  return <Muted>{Math.max(0, Math.floor((now - since) / 1000))} s</Muted>;
}

/** A command running as a job: polls it, offers to stop it, hands its result back when it ends. */
function JobLine({
  job,
  onDone,
}: {
  job: RunningJob;
  onDone: (j: JobStatus) => void;
}) {
  const { data, error } = useJob(job.id, onDone);
  const cancel = useMutation({
    mutationFn: () => jobCancel(job.id),
    onSuccess: () => toast.success(t("app.job.stopRequested")),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const last = data?.log.trimEnd().split("\n").at(-1);
  return (
    <div className="flex items-baseline gap-2 text-[#d29922]">
      <Spinner className="size-3 shrink-0 self-center" />
      <span className="min-w-0 flex-1">
        {t("tools.at.output.jobRunning", { job: job.id })}
        {error ? (
          <span className="text-[#ff7b72]"> · {errorMessage(error)}</span>
        ) : (
          last && <span className="text-[#8b949e]"> · {last}</span>
        )}
        {data?.cancellable && (
          <button
            type="button"
            className="ml-2 text-[#8b949e] underline underline-offset-2 hover:text-[#e6edf3] disabled:opacity-50"
            disabled={cancel.isPending}
            onClick={() => cancel.mutate()}
          >
            {cancel.isPending
              ? t("common.action.stopping")
              : t("common.action.stop")}
          </button>
        )}
      </span>
      <Elapsed since={job.start} />
    </div>
  );
}

/** The scrollback: each command after its prompt, then the modem's lines, the result line and how long it took. */
export function Transcript({
  entries,
  sentAt,
  job,
  onJobDone,
}: {
  entries: OutputEntry[];
  /** Set while a request waits for its answer. */
  sentAt?: number;
  job: RunningJob | null;
  onJobDone: (j: JobStatus) => void;
}) {
  return (
    <>
      {entries.length === 0 && (
        <div className="text-[#8b949e]"># {t("tools.at.output.empty")}</div>
      )}
      {entries.map((e, i) => (
        <Entry key={e.id} entry={e} first={i === 0} />
      ))}
      {sentAt !== undefined && (
        <div className="flex items-baseline gap-2 text-[#8b949e]">
          <Spinner className="size-3 shrink-0 self-center" />
          <span className="flex-1">{t("tools.at.output.waiting")}</span>
          <Elapsed since={sentAt} />
        </div>
      )}
      {job && <JobLine job={job} onDone={onJobDone} />}
    </>
  );
}
