import { Badge } from "@/components/ui/badge";
import { Bubble, BubbleContent } from "@/components/ui/bubble";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { CopyIcon, EllipsisVerticalIcon, Trash2Icon } from "lucide-react";
import { toast } from "sonner";
import { smsFullTime, smsShortTime } from "../lib/time";
import type { SmsMessage } from "../types";

const codingLabel = (coding: SmsMessage["coding"]) =>
  coding === "gsm"
    ? "GSM-7"
    : coding === "ucs2"
      ? "UCS-2"
      : t("sms.bubble.coding8bit");

/** One message: incoming on the left, outgoing on the right, its time, parts and an incomplete long message marked. */
export function MessageBubble({
  message,
  deleting,
  onDelete,
}: {
  message: SmsMessage;
  deleting: boolean;
  onDelete: (m: SmsMessage) => void;
}) {
  const out = message.dir === "out";
  const copy = () =>
    navigator.clipboard.writeText(message.text).then(
      () => toast.success(t("common.action.copied")),
      () => toast.error(t("sms.toast.copyFailed")),
    );
  return (
    <Bubble
      align={out ? "end" : "start"}
      variant={out ? "default" : "muted"}
      className={cn(
        "group/sms max-w-[85%] gap-0.5 md:max-w-[70%]",
        deleting && "opacity-50",
      )}
      aria-busy={deleting}
    >
      <BubbleContent className="rounded-lg px-2.5 py-1.5 leading-snug whitespace-pre-wrap select-text">
        {message.text || t("sms.list.blank")}
      </BubbleContent>
      <div
        className={cn(
          "flex h-5 flex-wrap items-center gap-1.5 px-0.5 text-[11px] text-muted-foreground tabular-nums",
          out && "justify-end",
        )}
      >
        <time title={smsFullTime(message.time, message.utc_offset)}>
          {smsShortTime(message.time, message.utc_offset)}
        </time>
        {message.parts > 1 && (
          <span title={codingLabel(message.coding)}>
            {t("sms.bubble.parts", { count: message.parts })}
          </span>
        )}
        {message.incomplete && (
          <Badge
            variant="destructive"
            className="h-4 px-1 text-[10px]"
            title={t("sms.bubble.incompleteTitle")}
          >
            {t("sms.bubble.incomplete", {
              have: message.incomplete.have,
              total: message.incomplete.total,
            })}
          </Badge>
        )}
        {message.status === "unread" && (
          <Badge variant="secondary" className="h-4 px-1 text-[10px]">
            {t("sms.bubble.unread")}
          </Badge>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label={t("sms.bubble.menu")}
              className="size-5 opacity-100 sm:opacity-0 sm:group-hover/sms:opacity-100 sm:focus-visible:opacity-100"
            >
              <EllipsisVerticalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align={out ? "end" : "start"}>
            <DropdownMenuGroup>
              <DropdownMenuItem onSelect={() => void copy()}>
                <CopyIcon />
                {t("sms.bubble.copyText")}
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                disabled={deleting}
                onSelect={() => onDelete(message)}
              >
                <Trash2Icon />
                {t("common.action.delete")}
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </Bubble>
  );
}
