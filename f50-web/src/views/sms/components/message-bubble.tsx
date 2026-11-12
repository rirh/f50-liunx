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
import { cn } from "@/lib/utils";
import { CopyIcon, EllipsisVerticalIcon, Trash2Icon } from "lucide-react";
import { toast } from "sonner";
import { smsFullTime, smsShortTime } from "../lib/time";
import type { SmsMessage } from "../types";

const CODING_LABEL: Record<SmsMessage["coding"], string> = {
  gsm: "GSM-7",
  "8bit": "8 位",
  ucs2: "UCS-2",
};

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
      () => toast.success("已复制"),
      () => toast.error("复制失败"),
    );
  return (
    <Bubble
      align={out ? "end" : "start"}
      variant={out ? "default" : "muted"}
      className={cn("group/sms", deleting && "opacity-50")}
      aria-busy={deleting}
    >
      <BubbleContent className="whitespace-pre-wrap select-text">
        {message.text || "（空）"}
      </BubbleContent>
      <div
        className={cn(
          "flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground",
          out && "justify-end",
        )}
      >
        <time title={smsFullTime(message.time, message.utc_offset)}>
          {smsShortTime(message.time, message.utc_offset)}
        </time>
        {message.parts > 1 && (
          <span title={CODING_LABEL[message.coding]}>
            {message.parts} 条拼接
          </span>
        )}
        {message.incomplete && (
          <Badge variant="destructive" title="长短信的部分分段尚未收到">
            不完整 {message.incomplete.have}/{message.incomplete.total}
          </Badge>
        )}
        {message.status === "unread" && <Badge variant="secondary">未读</Badge>}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              aria-label="短信操作"
              className="opacity-100 sm:opacity-0 sm:group-hover/sms:opacity-100 sm:focus-visible:opacity-100"
            >
              <EllipsisVerticalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align={out ? "end" : "start"}>
            <DropdownMenuGroup>
              <DropdownMenuItem onSelect={() => void copy()}>
                <CopyIcon />
                复制内容
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                disabled={deleting}
                onSelect={() => onDelete(message)}
              >
                <Trash2Icon />
                删除
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </Bubble>
  );
}
