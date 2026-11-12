import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { formatAgo, formatTime } from "@/lib/format";
import { useSetAtom } from "jotai";
import { PlusIcon, RefreshCwIcon, Trash2Icon } from "lucide-react";
import { DeleteDialog } from "./delete-dialog";
import {
  useConversations,
  useDeleteSms,
  useSyncSms,
  useUnread,
} from "../queries";
import { composeOpenAtom } from "../store/sms";

/** The page's actions: counts, the SIM store's use, sync, new message, delete all. */
export function SmsActions({ onDeletedAll }: { onDeletedAll: () => void }) {
  const unread = useUnread();
  const storage = useConversations().data?.pages[0]?.sim_storage ?? null;
  const sync = useSyncSms();
  const del = useDeleteSms();
  const setCompose = useSetAtom(composeOpenAtom);

  return (
    <>
      {unread.isPending ? (
        <Spinner />
      ) : unread.data ? (
        <Badge variant={unread.data.unread > 0 ? "default" : "secondary"}>
          {t("sms.actions.unread", {
            unread: unread.data.unread,
            total: unread.data.total,
          })}
        </Badge>
      ) : null}
      {storage && (
        <Badge
          variant="outline"
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
        </Badge>
      )}
      <Button
        variant="outline"
        size="sm"
        disabled={sync.isPending}
        aria-busy={sync.isPending}
        onClick={() => sync.mutate()}
      >
        <RefreshCwIcon data-icon="inline-start" />
        {sync.isPending ? (
          <SweepShine>{t("sms.actions.syncing")}</SweepShine>
        ) : (
          t("sms.actions.sync")
        )}
      </Button>
      <DeleteDialog
        title={t("sms.actions.deleteAllTitle")}
        description={t("sms.actions.deleteAllDescription")}
        onConfirm={(sim) =>
          del.mutate({ id: "all", sim }, { onSuccess: onDeletedAll })
        }
        trigger={
          <Button
            variant="outline"
            size="sm"
            disabled={del.isPending || unread.data?.total === 0}
            aria-busy={del.isPending}
          >
            <Trash2Icon data-icon="inline-start" />
            {del.isPending ? (
              <SweepShine>{t("sms.actions.deleting")}</SweepShine>
            ) : (
              t("sms.actions.deleteAll")
            )}
          </Button>
        }
      />
      <Button size="sm" onClick={() => setCompose(true)}>
        <PlusIcon data-icon="inline-start" />
        {t("sms.actions.new")}
      </Button>
    </>
  );
}
