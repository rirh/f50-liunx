import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
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
          未读 {unread.data.unread} / 共 {unread.data.total}
        </Badge>
      ) : null}
      {storage && (
        <Badge
          variant="outline"
          title={`${storage.store} 存储，${formatTime(storage.at)} 读取`}
        >
          SIM 存储 {storage.used}/{storage.total} · {formatAgo(storage.at)}
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
        {sync.isPending ? <SweepShine>同步中</SweepShine> : "同步"}
      </Button>
      <DeleteDialog
        title="删除全部短信"
        description="设备短信库中的所有短信都会被删除，无法恢复。"
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
            {del.isPending ? <SweepShine>删除中</SweepShine> : "全部删除"}
          </Button>
        }
      />
      <Button size="sm" onClick={() => setCompose(true)}>
        <PlusIcon data-icon="inline-start" />
        新短信
      </Button>
    </>
  );
}
