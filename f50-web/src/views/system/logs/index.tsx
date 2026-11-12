import { useMemo } from "react";
import { PageHeader } from "@/components/f50/page-header";
import { QueryState } from "@/components/f50/query-state";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { useAtomValue } from "jotai";
import { toast } from "sonner";
import { getLogs, logsKey } from "./api";
import { LogToolbar } from "./components/log-toolbar";
import { LogViewer } from "./components/log-viewer";
import { autoRefreshAtom, filterAtom, linesAtom } from "./store";

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

export default function Page() {
  const lines = useAtomValue(linesAtom);
  const auto = useAtomValue(autoRefreshAtom);
  const filter = useAtomValue(filterAtom);
  const query = useQuery({
    queryKey: logsKey(lines),
    queryFn: () => getLogs(lines),
    refetchInterval: auto ? 5000 : false,
    placeholderData: (prev) => prev,
  });
  const all = useMemo(
    () => (query.data?.text ?? "").replace(/\n$/, "").split("\n"),
    [query.data],
  );
  const shown = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return f
      ? all.filter((l) => l.startsWith("--- ") || l.toLowerCase().includes(f))
      : all;
  }, [all, filter]);
  const text = shown.join("\n");

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(t("maintenance.logs.copied", { count: shown.length }));
    } catch {
      toast.error(t("maintenance.logs.copyFailed"));
    }
  };

  return (
    <>
      <PageHeader
        title={t("maintenance.logs.title")}
        description={t("maintenance.logs.description")}
      />
      <Card>
        <CardHeader>
          <CardTitle>{t("maintenance.logs.card.title")}</CardTitle>
          <CardDescription>
            {t("maintenance.logs.card.description", { lines })}
            {filter.trim() &&
              t("maintenance.logs.card.filtered", { count: shown.length })}
          </CardDescription>
          <CardAction>
            {query.isFetching ? (
              <Badge variant="secondary">
                {t("maintenance.logs.card.reading")}
              </Badge>
            ) : (
              auto && (
                <Badge variant="outline">
                  {t("maintenance.logs.card.auto")}
                </Badge>
              )
            )}
          </CardAction>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <LogToolbar
            fetching={query.isFetching}
            hasText={!!query.data && shown.length > 0}
            onRefresh={() => query.refetch()}
            onCopy={copy}
            onDownload={() => download(text)}
          />
          <QueryState query={query} skeletonRows={12}>
            {() => (
              <LogViewer
                lines={shown}
                empty={
                  filter.trim()
                    ? t("maintenance.logs.noMatch")
                    : t("maintenance.logs.empty")
                }
              />
            )}
          </QueryState>
        </CardContent>
      </Card>
    </>
  );
}
