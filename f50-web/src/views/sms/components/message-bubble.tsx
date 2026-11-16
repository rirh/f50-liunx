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
import { copyText } from "@/lib/clipboard";
import { cn } from "@/lib/utils";
import { CopyIcon, EllipsisIcon, Trash2Icon } from "lucide-react";
import { toast } from "sonner";
import { linkify } from "../lib/linkify";
import { smsFullTime } from "../lib/time";
import type { SmsMessage } from "../types";

const codingLabel = (coding: SmsMessage["coding"]) =>
  coding === "gsm"
    ? "GSM-7"
    : coding === "ucs2"
      ? "UCS-2"
      : t("sms.bubble.coding8bit");

/**
 * One message in the style of Messages: incoming grey on the left, outgoing tinted on the right, 18 px corners, the
 * time in the hover title (the thread puts a time between runs). Copy and the menu appear beside it on hover.
 */
export function MessageBubble({
  message,
  spaced,
  deleting,
  onDelete,
}: {
  message: SmsMessage;
  /** The first of a run after the other side spoke: more air above. */
  spaced?: boolean;
  deleting: boolean;
  onDelete: (m: SmsMessage) => void;
}) {
  const out = message.dir === "out";
  const copy = () =>
    copyText(message.text).then(
      () => toast.success(t("common.action.copied")),
      () => toast.error(t("sms.toast.copyFailed")),
    );
  const title = [
    smsFullTime(message.time, message.utc_offset),
    message.parts > 1 &&
      `${t("sms.bubble.parts", { count: message.parts })} · ${codingLabel(message.coding)}`,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <div
      className={cn(
        "group/sms flex items-end gap-1",
        out ? "flex-row-reverse" : "flex-row",
        spaced && "mt-2",
        deleting && "opacity-50",
      )}
      aria-busy={deleting}
    >
      <Bubble
        align={out ? "end" : "start"}
        variant={out ? "default" : "muted"}
        className="max-w-[78%] gap-1 sm:max-w-[min(70%,34rem)]"
      >
        <BubbleContent
          title={title}
          className={cn(
            "rounded-[18px] px-3 py-[7px] text-[15px] leading-[1.35] whitespace-pre-wrap select-text",
            !out && "bg-foreground/[0.07]! dark:bg-foreground/[0.12]!",
          )}
        >
          {message.text ? linkify(message.text) : t("sms.list.blank")}
        </BubbleContent>
        {message.incomplete && (
          <Badge
            variant="destructive"
            className={cn("h-4 px-1.5 text-[10px]", out && "self-end")}
            title={t("sms.bubble.incompleteTitle")}
          >
            {t("sms.bubble.incomplete", {
              have: message.incomplete.have,
              total: message.incomplete.total,
            })}
          </Badge>
        )}
      </Bubble>
      <div className="flex shrink-0 items-center opacity-100 transition-opacity sm:opacity-0 sm:group-hover/sms:opacity-100 sm:focus-within:opacity-100">
        {message.text && (
          <Button
            variant="ghost"
            size="icon-xs"
            className="rounded-full text-muted-foreground"
            title={t("sms.bubble.copyText")}
            aria-label={t("sms.bubble.copyText")}
            onClick={() => void copy()}
          >
            <CopyIcon />
          </Button>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              className="rounded-full text-muted-foreground"
              aria-label={t("sms.bubble.menu")}
            >
              <EllipsisIcon />
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
    </div>
  );
}
