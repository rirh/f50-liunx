import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { formatAgo, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useSetAtom } from "jotai";
import { PlusIcon, RefreshCwIcon } from "lucide-react";
import { useConversations, useSyncSms, useUnread } from "../queries";
import { composeOpenAtom } from "../store/sms";

/**
 * The toolbar's right side: counts and the SIM store's use as plain text, then sync and new message. There is no
 * "delete all": messages are deleted one at a time (each message's menu) or a conversation at once (its header).
 */
export function SmsActions() {
  const unread = useUnread();
  const storage = useConversations().data?.pages[0]?.sim_storage ?? null;
  const sync = useSyncSms();
  const setCompose = useSetAtom(composeOpenAtom);

  return (
    <>
      <span className="hidden shrink-0 items-center gap-2 text-xs text-muted-foreground tabular-nums sm:flex">
        {unread.isPending ? (
          <Spinner className="size-3" />
        ) : unread.data ? (
          <span
            className={cn(unread.data.unread > 0 && "font-medium text-primary")}
          >
            {t("sms.actions.unread", {
              unread: unread.data.unread,
              total: unread.data.total,
            })}
          </span>
        ) : null}
        {storage && (
          <span
            title={t("sms.actions.storageTitle", {
              store: storage.store,
              time: formatTime(storage.at),
            })}
          >
            {t("sms.actions.storage", {
              used: storage.used,
              total: storage.total,
              ago: formatAgo(storage.at),
            })}
          </span>
        )}
      </span>
      <Separator orientation="vertical" className="mx-1 hidden h-4 sm:block" />
      <Button
        variant="ghost"
        size="xs"
        title={t("sms.actions.sync")}
        disabled={sync.isPending}
        aria-busy={sync.isPending}
        onClick={() => sync.mutate()}
      >
        <RefreshCwIcon data-icon="inline-start" />
        <span className="hidden sm:inline">
          {sync.isPending ? (
            <SweepShine>{t("sms.actions.syncing")}</SweepShine>
          ) : (
            t("sms.actions.sync")
          )}
        </span>
      </Button>
      <Button size="xs" onClick={() => setCompose(true)}>
        <PlusIcon data-icon="inline-start" />
        {t("sms.actions.new")}
      </Button>
    </>
  );
}
