import { useState } from "react";
import { Button } from "@/components/ui/button";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { useAtom, useSetAtom } from "jotai";
import {
  BellRingIcon,
  ListFilterIcon,
  PenSquareIcon,
  RefreshCwIcon,
} from "lucide-react";
import { PushDialog } from "./push-dialog";
import { useSyncSms } from "../queries";
import { composeOpenAtom, filterAtom } from "../store/sms";

/**
 * The list column's header actions, icons only (their names in the tooltip): unread only, sync, Telegram push, new
 * message. There is no "delete all": messages are deleted one at a time or a conversation at once (its header).
 */
export function SmsActions() {
  const sync = useSyncSms();
  const setCompose = useSetAtom(composeOpenAtom);
  const [pushOpen, setPushOpen] = useState(false);
  const [filter, setFilter] = useAtom(filterAtom);
  const unreadOnly = filter === "unread";

  return (
    <div className="flex shrink-0 items-center">
      <Button
        variant="ghost"
        size="icon-sm"
        className={cn(
          "rounded-full",
          unreadOnly &&
            "bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary",
        )}
        title={t("sms.list.unread")}
        aria-label={t("sms.list.unread")}
        aria-pressed={unreadOnly}
        onClick={() => setFilter(unreadOnly ? "all" : "unread")}
      >
        <ListFilterIcon />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        className="rounded-full"
        title={t("sms.actions.sync")}
        aria-label={t("sms.actions.sync")}
        disabled={sync.isPending}
        aria-busy={sync.isPending}
        onClick={() => sync.mutate()}
      >
        <RefreshCwIcon className={cn(sync.isPending && "animate-spin")} />
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        className="rounded-full"
        title={t("sms.actions.push")}
        aria-label={t("sms.actions.push")}
        onClick={() => setPushOpen(true)}
      >
        <BellRingIcon />
      </Button>
      <PushDialog open={pushOpen} onOpenChange={setPushOpen} />
      <Button
        variant="ghost"
        size="icon-sm"
        className="rounded-full text-primary hover:text-primary"
        title={t("sms.actions.new")}
        aria-label={t("sms.actions.new")}
        onClick={() => setCompose(true)}
      >
        <PenSquareIcon />
      </Button>
    </div>
  );
}
