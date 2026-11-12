import { useEffect, useRef } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { format } from "date-fns";
import { useAtom } from "jotai";
import { CopyIcon, EraserIcon, TerminalIcon } from "lucide-react";
import { toast } from "sonner";
import { outputAtom } from "../store/terminal";
import type { OutputEntry } from "../types";

function asText(entries: OutputEntry[]) {
  return entries
    .map((e) => {
      const t = format(e.time, "HH:mm:ss");
      const body =
        e.kind === "command" ? e.lines.map((l) => `> ${l}`) : e.lines;
      return [`[${t}]`, ...body, ...(e.status ? [e.status] : [])].join("\n");
    })
    .join("\n");
}

const isOk = (status?: string) => status === "OK";

/** The terminal's scrollback: commands, replies with their final line, refusals; copy and clear. */
export function OutputPane() {
  const [entries, setEntries] = useAtom(outputAtom);
  const end = useRef<HTMLDivElement>(null);
  useEffect(
    () => end.current?.scrollIntoView({ block: "nearest" }),
    [entries.length],
  );

  const copy = () =>
    navigator.clipboard.writeText(asText(entries)).then(
      () => toast.success(t("tools.at.output.copied")),
      () => toast.error(t("tools.at.output.copyFailed")),
    );

  return (
    <Card className="min-h-0 gap-3">
      <CardHeader>
        <CardTitle>{t("tools.at.output.title")}</CardTitle>
        <CardDescription>
          {entries.length > 0
            ? t("tools.at.output.count", { count: entries.length })
            : t("tools.at.output.empty")}
        </CardDescription>
        <CardAction className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={entries.length === 0}
            onClick={() => void copy()}
          >
            <CopyIcon data-icon="inline-start" />
            {t("common.action.copy")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={entries.length === 0}
            onClick={() => setEntries([])}
          >
            <EraserIcon data-icon="inline-start" />
            {t("common.action.clear")}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent>
        <div
          role="log"
          aria-live="polite"
          className="h-[min(28rem,55dvh)] overflow-y-auto overscroll-contain rounded-md bg-muted p-3 font-mono text-xs"
        >
          {entries.length === 0 ? (
            <Empty className="h-full">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <TerminalIcon />
                </EmptyMedia>
                <EmptyTitle>{t("tools.at.output.emptyTitle")}</EmptyTitle>
                <EmptyDescription>
                  {t("tools.at.output.emptyDescription")}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          ) : (
            <div className="flex flex-col gap-2">
              {entries.map((e) => (
                <OutputBlock key={e.id} entry={e} />
              ))}
              <div ref={end} />
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function OutputBlock({ entry }: { entry: OutputEntry }) {
  const time = format(entry.time, "HH:mm:ss");
  return (
    <div className="flex gap-2">
      <time
        className="shrink-0 text-muted-foreground"
        title={format(entry.time, "yyyy-MM-dd HH:mm:ss")}
      >
        {time}
      </time>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        {entry.lines.map((l, i) => (
          <span
            key={i}
            className={cn(
              "break-all whitespace-pre-wrap",
              entry.kind === "command" && "font-semibold",
              entry.kind === "error" && "text-destructive",
              entry.kind === "info" && "text-muted-foreground",
            )}
          >
            {entry.kind === "command" ? `> ${l}` : l}
          </span>
        ))}
        {entry.status && (
          <Badge
            variant={isOk(entry.status) ? "secondary" : "destructive"}
            className="font-mono"
          >
            {entry.status}
          </Badge>
        )}
      </div>
    </div>
  );
}
