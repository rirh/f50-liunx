import { Badge } from "@/components/ui/badge";
import { MessageSquareIcon } from "lucide-react";
import { TextLink } from "./text-link";
import { Figure, Tile } from "./tile";
import type { Part, SmsUnread } from "../types";

export function SmsTile({ part }: { part: Part<SmsUnread> | undefined }) {
  return (
    <Tile title="短信" icon={MessageSquareIcon} to="/sms" part={part}>
      {(s) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure value={`${s.unread} 条未读`} caption={`共 ${s.total} 条`} />
            {s.unread > 0 && <Badge>新消息</Badge>}
          </div>
          <TextLink to="/sms" className="self-start text-sm text-primary">
            {s.unread > 0 ? "查看未读短信" : "打开短信"}
          </TextLink>
        </>
      )}
    </Tile>
  );
}
