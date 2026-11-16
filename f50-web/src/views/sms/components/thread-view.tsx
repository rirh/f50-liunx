import { Fragment, useEffect, useRef, useState } from "react";
import { UserAvatar } from "@/components/f50/user-avatar";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { BubbleGroup } from "@/components/ui/bubble";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { ArrowLeftIcon, MessagesSquareIcon, Trash2Icon } from "lucide-react";
import { Composer } from "./composer";
import { DeleteDialog } from "./delete-dialog";
import { MessageBubble } from "./message-bubble";
import { smsDate, smsFullTime, smsShortTime, useMinuteTick } from "../lib/time";
import { useDeleteConversation, useDeleteSms, useThread } from "../queries";
import type { SmsMessage } from "../types";

/** A time separator goes between two messages more than 30 minutes apart (as Messages does). */
const SEPARATE_MS = 30 * 60_000;

function separated(a: SmsMessage, b: SmsMessage): boolean {
  const da = smsDate(a.time, a.utc_offset);
  const db = smsDate(b.time, b.utc_offset);
  if (!da || !db) return a.time.slice(0, 10) !== b.time.slice(0, 10);
  return Math.abs(db.getTime() - da.getTime()) > SEPARATE_MS;
}

/** The open conversation: its messages oldest first (older pages on demand), delete, and the reply box. */
export function ThreadView({
  peer,
  onBack,
}: {
  peer: string;
  onBack: () => void;
}) {
  const thread = useThread(peer);
  const { messages } = thread;
  const del = useDeleteSms();
  const delConv = useDeleteConversation();
  const [toDelete, setToDelete] = useState<SmsMessage | null>(null);
  const now = useMinuteTick();
  const total = thread.data?.pages[0]?.total ?? 0;

  // keep the newest message in view when one arrives (not when older pages load above)
  const scroller = useRef<HTMLDivElement>(null);
  const newest = messages.at(-1)?.id;
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [newest, peer]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="grid shrink-0 grid-cols-[1fr_auto_1fr] items-center px-3 pt-3 pb-2">
        <div className="flex">
          <Button
            variant="ghost"
            size="icon-sm"
            className="rounded-full md:hidden"
            aria-label={t("sms.thread.back")}
            onClick={onBack}
          >
            <ArrowLeftIcon />
          </Button>
        </div>
        <div
          className="flex min-w-0 flex-col items-center gap-1"
          title={
            thread.isPending
              ? undefined
              : t("sms.thread.count", { count: total })
          }
        >
          <UserAvatar name={peer} className="size-10" />
          <span className="max-w-60 truncate text-xs font-medium tabular-nums">
            {peer}
          </span>
        </div>
        <div className="flex justify-end">
          <DeleteDialog
            title={t("sms.thread.deleteConversationTitle")}
            description={t("sms.thread.deleteConversationDescription", {
              peer,
              count: total,
            })}
            onConfirm={(sim) =>
              delConv.mutate({ peer, sim }, { onSuccess: onBack })
            }
            trigger={
              <Button
                variant="ghost"
                size="icon-sm"
                className="rounded-full text-muted-foreground hover:text-destructive"
                title={t("sms.thread.deleteConversation")}
                aria-label={t("sms.thread.deleteConversation")}
                disabled={delConv.isPending || total === 0}
                aria-busy={delConv.isPending}
              >
                <Trash2Icon />
              </Button>
            }
          />
        </div>
      </div>

      <div
        ref={scroller}
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 pt-2 pb-4 sm:px-6"
      >
        {thread.isPending ? (
          <div className="flex flex-col gap-1.5">
            <Skeleton className="h-8 w-2/3" />
            <Skeleton className="h-8 w-1/2 self-end" />
            <Skeleton className="h-10 w-3/5" />
          </div>
        ) : thread.isError ? (
          <Alert variant="destructive">
            <AlertTitle>{t("common.state.readFailed")}</AlertTitle>
            <AlertDescription className="flex flex-col items-start gap-2">
              {errorMessage(thread.error)}
              <Button
                size="sm"
                variant="outline"
                onClick={() => void thread.refetch()}
              >
                {t("common.action.retry")}
              </Button>
            </AlertDescription>
          </Alert>
        ) : messages.length === 0 ? (
          <div className="flex flex-col items-center gap-1 py-10 text-center text-muted-foreground">
            <MessagesSquareIcon className="size-5" />
            <span className="text-sm">{t("sms.thread.emptyTitle")}</span>
            <span className="text-xs">{t("sms.thread.emptyDescription")}</span>
          </div>
        ) : (
          <BubbleGroup className="mx-auto w-full max-w-3xl gap-0.5">
            {thread.hasNextPage && (
              <Button
                variant="ghost"
                size="xs"
                className="self-center"
                disabled={thread.isFetchingNextPage}
                aria-busy={thread.isFetchingNextPage}
                onClick={() => void thread.fetchNextPage()}
              >
                {thread.isFetchingNextPage ? (
                  <Spinner data-icon="inline-start" />
                ) : null}
                {thread.isFetchingNextPage ? (
                  <SweepShine>{t("common.action.loading")}</SweepShine>
                ) : (
                  t("sms.thread.older")
                )}
              </Button>
            )}
            {messages.map((m, i) => {
              const prev = messages[i - 1];
              const gap = !prev || separated(prev, m);
              return (
                <Fragment key={m.id}>
                  {gap && (
                    <div
                      className="self-center pt-4 pb-1.5 text-[11px] font-medium text-muted-foreground tabular-nums"
                      title={smsFullTime(m.time, m.utc_offset)}
                    >
                      {smsShortTime(m.time, m.utc_offset, now)}
                    </div>
                  )}
                  <MessageBubble
                    message={m}
                    // a new sender or a pause starts a new run, with air above it
                    spaced={!gap && prev.dir !== m.dir}
                    deleting={del.isPending && del.variables?.id === m.id}
                    onDelete={setToDelete}
                  />
                </Fragment>
              );
            })}
          </BubbleGroup>
        )}
      </div>

      <Composer key={peer} peer={peer} />

      <DeleteDialog
        open={toDelete !== null}
        onOpenChange={(o) => !o && setToDelete(null)}
        title={t("sms.thread.deleteOneTitle")}
        description={
          toDelete
            ? t("sms.thread.quote", {
                text: `${toDelete.text.slice(0, 40)}${toDelete.text.length > 40 ? "…" : ""}`,
              })
            : ""
        }
        onConfirm={(sim) => {
          if (toDelete) del.mutate({ id: toDelete.id, sim });
          setToDelete(null);
        }}
      />
    </div>
  );
}
