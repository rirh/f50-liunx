import { useEffect, useRef, useState } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { BubbleGroup } from "@/components/ui/bubble";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { ArrowLeftIcon, MessagesSquareIcon, Trash2Icon } from "lucide-react";
import { Composer } from "./composer";
import { DeleteDialog } from "./delete-dialog";
import { MessageBubble } from "./message-bubble";
import { useDeleteConversation, useDeleteSms, useThread } from "../queries";
import type { SmsMessage } from "../types";

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
      <div className="flex items-center gap-2 border-b p-2">
        <Button
          variant="ghost"
          size="icon-sm"
          className="md:hidden"
          aria-label={t("sms.thread.back")}
          onClick={onBack}
        >
          <ArrowLeftIcon />
        </Button>
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate font-medium">{peer}</span>
          <span className="text-xs text-muted-foreground">
            {thread.isPending
              ? t("sms.thread.loading")
              : t("sms.thread.count", { count: total })}
            {thread.isFetching &&
              !thread.isPending &&
              ` · ${t("common.action.refreshing")}`}
          </span>
        </div>
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
              size="sm"
              disabled={delConv.isPending || total === 0}
              aria-busy={delConv.isPending}
            >
              <Trash2Icon data-icon="inline-start" />
              {delConv.isPending ? (
                <SweepShine>{t("sms.actions.deleting")}</SweepShine>
              ) : (
                <span className="hidden sm:inline">
                  {t("sms.thread.deleteConversation")}
                </span>
              )}
            </Button>
          }
        />
      </div>

      <div
        ref={scroller}
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-3"
      >
        {thread.isPending ? (
          <div className="flex flex-col gap-3">
            <Skeleton className="h-12 w-2/3" />
            <Skeleton className="h-12 w-1/2 self-end" />
            <Skeleton className="h-16 w-3/5" />
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
          <Empty>
            <EmptyHeader>
              <EmptyMedia variant="icon">
                <MessagesSquareIcon />
              </EmptyMedia>
              <EmptyTitle>{t("sms.thread.emptyTitle")}</EmptyTitle>
              <EmptyDescription>
                {t("sms.thread.emptyDescription")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <BubbleGroup className="gap-3">
            {thread.hasNextPage && (
              <Button
                variant="outline"
                size="sm"
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
            {messages.map((m) => (
              <MessageBubble
                key={m.id}
                message={m}
                deleting={del.isPending && del.variables?.id === m.id}
                onDelete={setToDelete}
              />
            ))}
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
