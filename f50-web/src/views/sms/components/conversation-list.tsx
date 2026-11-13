import { useMemo } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import { SweepShine } from "@/components/ui/sweep-shine";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAtom } from "jotai";
import { InboxIcon, SearchIcon, UserRoundIcon, XIcon } from "lucide-react";
import { smsShortTime } from "../lib/time";
import { useConversations } from "../queries";
import { filterAtom, searchAtom } from "../store/sms";
import type { ListFilter, SmsConversation } from "../types";

/** Conversations newest first, searchable by number or preview, with an unread-only filter. */
export function ConversationList({
  selected,
  onSelect,
}: {
  selected: string | null;
  onSelect: (peer: string) => void;
}) {
  const query = useConversations();
  const [search, setSearch] = useAtom(searchAtom);
  const [filter, setFilter] = useAtom(filterAtom);

  const all = useMemo(
    () => query.data?.pages.flatMap((p) => p.conversations) ?? [],
    [query.data],
  );
  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return all.filter(
      (c) =>
        (filter === "all" || c.unread > 0) &&
        (!q ||
          c.peer.toLowerCase().includes(q) ||
          c.preview.toLowerCase().includes(q)),
    );
  }, [all, search, filter]);
  const totalConvs = query.data?.pages[0]?.conversations_total ?? 0;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-1.5 border-b px-2">
        <InputGroup className="h-7 min-w-0 flex-1">
          <InputGroupAddon>
            <SearchIcon className="size-3.5" />
          </InputGroupAddon>
          <InputGroupInput
            aria-label={t("sms.list.searchLabel")}
            placeholder={t("sms.list.searchPlaceholder")}
            className="text-base sm:text-xs"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search && (
            <InputGroupAddon align="inline-end">
              <InputGroupButton
                size="icon-xs"
                aria-label={t("sms.list.clearSearch")}
                onClick={() => setSearch("")}
              >
                <XIcon />
              </InputGroupButton>
            </InputGroupAddon>
          )}
        </InputGroup>
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          className="shrink-0"
          value={filter}
          onValueChange={(v) => v && setFilter(v as ListFilter)}
        >
          <ToggleGroupItem value="all" className="h-7 px-2 text-xs">
            {t("sms.list.all")}
          </ToggleGroupItem>
          <ToggleGroupItem value="unread" className="h-7 px-2 text-xs">
            {t("sms.list.unread")}
          </ToggleGroupItem>
        </ToggleGroup>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        {query.isPending ? (
          <div className="flex flex-col gap-1 p-2">
            {Array.from({ length: 8 }, (_, i) => (
              <Skeleton key={i} className="h-9 w-full" />
            ))}
          </div>
        ) : query.isError ? (
          <Alert variant="destructive" className="m-2 w-auto">
            <AlertTitle>{t("common.state.readFailed")}</AlertTitle>
            <AlertDescription className="flex flex-col items-start gap-2">
              {errorMessage(query.error)}
              <Button
                size="xs"
                variant="outline"
                onClick={() => void query.refetch()}
              >
                {t("common.action.retry")}
              </Button>
            </AlertDescription>
          </Alert>
        ) : shown.length === 0 ? (
          <div className="flex flex-col items-center gap-1 px-4 py-10 text-center text-muted-foreground">
            <InboxIcon className="size-5" />
            <span className="text-sm">
              {all.length === 0
                ? t("sms.list.emptyTitle")
                : t("sms.list.noMatchTitle")}
            </span>
            <span className="text-xs">
              {all.length === 0
                ? t("sms.list.emptyDescription")
                : t("sms.list.noMatchDescription")}
            </span>
          </div>
        ) : (
          <ul className="divide-y">
            {shown.map((c) => (
              <li key={c.peer}>
                <ConversationItem
                  conv={c}
                  active={c.peer === selected}
                  onSelect={onSelect}
                />
              </li>
            ))}
          </ul>
        )}
        {query.hasNextPage && (
          <div className="flex justify-center border-t p-1.5">
            <Button
              variant="ghost"
              size="xs"
              disabled={query.isFetchingNextPage}
              aria-busy={query.isFetchingNextPage}
              onClick={() => void query.fetchNextPage()}
            >
              {query.isFetchingNextPage ? (
                <SweepShine>{t("common.action.loading")}</SweepShine>
              ) : (
                t("sms.list.loadMore")
              )}
            </Button>
          </div>
        )}
      </div>

      <div className="flex h-6 shrink-0 items-center border-t px-2 text-[11px] text-muted-foreground">
        {query.isFetching && !query.isPending ? (
          <SweepShine>{t("common.action.refreshing")}</SweepShine>
        ) : (
          t("sms.list.count", { count: totalConvs })
        )}
      </div>
    </div>
  );
}

function ConversationItem({
  conv,
  active,
  onSelect,
}: {
  conv: SmsConversation;
  active: boolean;
  onSelect: (peer: string) => void;
}) {
  const unread = conv.unread > 0;
  return (
    <button
      type="button"
      aria-current={active || undefined}
      className={cn(
        "relative flex w-full cursor-pointer items-center gap-2.5 px-3 py-2 text-left outline-none hover:bg-muted/60 focus-visible:bg-muted",
        active && "bg-primary/10 hover:bg-primary/15",
      )}
      onClick={() => onSelect(conv.peer)}
    >
      {active && (
        <span className="absolute inset-y-0 left-0 w-0.5 bg-primary" />
      )}
      <span
        className={cn(
          "relative flex size-8 shrink-0 items-center justify-center rounded-full text-xs font-medium",
          active
            ? "bg-primary text-primary-foreground"
            : "bg-muted text-muted-foreground",
        )}
        aria-hidden
      >
        {/^\+?\d/.test(conv.peer) ? (
          <UserRoundIcon className="size-4" />
        ) : (
          conv.peer.slice(0, 1).toUpperCase()
        )}
        {unread && (
          <span className="absolute -top-0.5 -right-0.5 size-2.5 rounded-full border-2 border-background bg-primary" />
        )}
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex w-full items-center gap-2">
          <span
            className={cn(
              "min-w-0 truncate text-sm tabular-nums",
              unread ? "font-semibold" : "font-medium",
            )}
          >
            {conv.peer}
          </span>
          <span
            className="ml-auto shrink-0 text-[11px] text-muted-foreground tabular-nums"
            title={conv.last_time}
          >
            {smsShortTime(conv.last_time)}
          </span>
        </span>
        <span className="flex w-full items-center gap-2 text-xs text-muted-foreground">
          <span className="min-w-0 flex-1 truncate">
            {conv.last_dir === "out" && t("sms.list.me")}
            {conv.preview || t("sms.list.blank")}
          </span>
          {unread && (
            <span className="shrink-0 rounded-full bg-primary px-1.5 text-[10px] leading-4 font-medium text-primary-foreground tabular-nums">
              {conv.unread}
            </span>
          )}
        </span>
      </span>
    </button>
  );
}
