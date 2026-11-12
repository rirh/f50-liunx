import { useMemo } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import {
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
} from "@/components/ui/item";
import { Skeleton } from "@/components/ui/skeleton";
import { SweepShine } from "@/components/ui/sweep-shine";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAtom } from "jotai";
import { InboxIcon, SearchIcon, XIcon } from "lucide-react";
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
      <div className="flex flex-col gap-2 border-b p-2">
        <InputGroup>
          <InputGroupAddon>
            <SearchIcon />
          </InputGroupAddon>
          <InputGroupInput
            aria-label={t("sms.list.searchLabel")}
            placeholder={t("sms.list.searchPlaceholder")}
            className="text-base sm:text-sm"
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
        <div className="flex items-center justify-between gap-2">
          <ToggleGroup
            type="single"
            size="sm"
            variant="outline"
            value={filter}
            onValueChange={(v) => v && setFilter(v as ListFilter)}
          >
            <ToggleGroupItem value="all">{t("sms.list.all")}</ToggleGroupItem>
            <ToggleGroupItem value="unread">
              {t("sms.list.unread")}
            </ToggleGroupItem>
          </ToggleGroup>
          <span className="text-xs text-muted-foreground">
            {query.isFetching && !query.isPending ? (
              <SweepShine>{t("common.action.refreshing")}</SweepShine>
            ) : (
              t("sms.list.count", { count: totalConvs })
            )}
          </span>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-1">
        {query.isPending ? (
          <div className="flex flex-col gap-2 p-2">
            {Array.from({ length: 6 }, (_, i) => (
              <Skeleton key={i} className="h-12 w-full" />
            ))}
          </div>
        ) : query.isError ? (
          <Alert variant="destructive" className="m-1 w-auto">
            <AlertTitle>{t("common.state.readFailed")}</AlertTitle>
            <AlertDescription className="flex flex-col items-start gap-2">
              {errorMessage(query.error)}
              <Button
                size="sm"
                variant="outline"
                onClick={() => void query.refetch()}
              >
                {t("common.action.retry")}
              </Button>
            </AlertDescription>
          </Alert>
        ) : shown.length === 0 ? (
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <InboxIcon />
              </EmptyMedia>
              <EmptyTitle>
                {all.length === 0
                  ? t("sms.list.emptyTitle")
                  : t("sms.list.noMatchTitle")}
              </EmptyTitle>
              <EmptyDescription>
                {all.length === 0
                  ? t("sms.list.emptyDescription")
                  : t("sms.list.noMatchDescription")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <ItemGroup className="gap-0.5">
            {shown.map((c) => (
              <ConversationItem
                key={c.peer}
                conv={c}
                active={c.peer === selected}
                onSelect={onSelect}
              />
            ))}
          </ItemGroup>
        )}
        {query.hasNextPage && (
          <div className="flex justify-center p-2">
            <Button
              variant="outline"
              size="sm"
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
  return (
    <Item asChild size="sm" variant={active ? "muted" : "default"}>
      <button
        type="button"
        aria-current={active || undefined}
        className="cursor-pointer text-left hover:bg-muted"
        onClick={() => onSelect(conv.peer)}
      >
        <ItemContent className="min-w-0">
          <ItemTitle className="flex w-full items-center gap-2">
            <span
              className={cn("truncate", conv.unread > 0 && "font-semibold")}
            >
              {conv.peer}
            </span>
            <span
              className="ml-auto shrink-0 text-xs font-normal text-muted-foreground"
              title={conv.last_time}
            >
              {smsShortTime(conv.last_time)}
            </span>
          </ItemTitle>
          <ItemDescription className="flex w-full items-center gap-2">
            <span className="min-w-0 flex-1 truncate">
              {conv.last_dir === "out" && t("sms.list.me")}
              {conv.preview || t("sms.list.blank")}
            </span>
            {conv.unread > 0 && (
              <Badge className="shrink-0">{conv.unread}</Badge>
            )}
          </ItemDescription>
        </ItemContent>
      </button>
    </Item>
  );
}
