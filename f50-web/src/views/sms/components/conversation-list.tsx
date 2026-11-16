import { useMemo } from "react";
import { UserAvatar } from "@/components/f50/user-avatar";
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
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAtom } from "jotai";
import { InboxIcon, SearchIcon, XIcon } from "lucide-react";
import { smsFullTime, smsShortTime, useMinuteTick } from "../lib/time";
import { useConversations } from "../queries";
import { filterAtom, searchAtom } from "../store/sms";
import type { SmsConversation } from "../types";

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
  const [filter] = useAtom(filterAtom);

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
  const now = useMinuteTick();

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 px-3 pb-2">
        <InputGroup className="h-8 rounded-lg border-transparent bg-foreground/[0.06] has-[[data-slot=input-group-control]:focus-visible]:border-transparent has-[[data-slot=input-group-control]:focus-visible]:ring-2 has-[[data-slot=input-group-control]:focus-visible]:ring-primary/30 dark:bg-foreground/10">
          <InputGroupAddon>
            <SearchIcon className="size-3.5" />
          </InputGroupAddon>
          <InputGroupInput
            aria-label={t("sms.list.searchLabel")}
            placeholder={t("sms.list.searchPlaceholder")}
            className="text-base sm:text-[13px]"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          {search && (
            <InputGroupAddon align="inline-end">
              <InputGroupButton
                size="icon-xs"
                className="rounded-full"
                aria-label={t("sms.list.clearSearch")}
                onClick={() => setSearch("")}
              >
                <XIcon />
              </InputGroupButton>
            </InputGroupAddon>
          )}
        </InputGroup>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        {query.isPending ? (
          <div className="flex flex-col gap-1 p-2">
            {Array.from({ length: 8 }, (_, i) => (
              <Skeleton key={i} className="h-14 w-full rounded-[10px]" />
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
          <ul className="flex flex-col gap-px px-2 pb-2">
            {shown.map((c) => (
              <li key={c.peer}>
                <ConversationItem
                  conv={c}
                  active={c.peer === selected}
                  onSelect={onSelect}
                  now={now}
                />
              </li>
            ))}
          </ul>
        )}
        {query.hasNextPage && (
          <div className="flex justify-center p-1.5">
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
    </div>
  );
}

function ConversationItem({
  conv,
  active,
  onSelect,
  now,
}: {
  conv: SmsConversation;
  active: boolean;
  onSelect: (peer: string) => void;
  now: Date;
}) {
  const unread = conv.unread > 0;
  return (
    <button
      type="button"
      aria-current={active || undefined}
      className={cn(
        "relative flex w-full cursor-pointer items-start gap-3 rounded-[10px] py-2.5 ps-4 pe-3 text-left transition-colors outline-none hover:bg-foreground/[0.04] focus-visible:bg-foreground/[0.06]",
        active &&
          "bg-primary text-primary-foreground hover:bg-primary focus-visible:bg-primary",
      )}
      onClick={() => onSelect(conv.peer)}
    >
      {unread && (
        <span
          aria-hidden
          className={cn(
            "absolute top-1/2 left-1 size-2 -translate-y-1/2 rounded-full",
            active ? "bg-primary-foreground" : "bg-primary",
          )}
        />
      )}
      <UserAvatar name={conv.peer} className="size-10" />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="flex w-full items-baseline gap-2">
          <span className="min-w-0 truncate text-[14px] font-semibold tabular-nums">
            {conv.peer}
          </span>
          <span
            className={cn(
              "ml-auto shrink-0 text-xs tabular-nums",
              active ? "text-primary-foreground/75" : "text-muted-foreground",
            )}
            title={smsFullTime(conv.last_time, conv.last_utc_offset)}
          >
            {smsShortTime(conv.last_time, conv.last_utc_offset, now)}
          </span>
        </span>
        <span
          className={cn(
            "line-clamp-2 text-[13px] leading-snug wrap-anywhere",
            active ? "text-primary-foreground/80" : "text-muted-foreground",
            unread && !active && "text-foreground",
          )}
        >
          {conv.last_dir === "out" && t("sms.list.me")}
          {conv.preview || t("sms.list.blank")}
        </span>
      </span>
    </button>
  );
}
