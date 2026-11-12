import { Badge } from "@/components/ui/badge";
import { t } from "@/i18n";
import { MessageSquareIcon } from "lucide-react";
import { TextLink } from "./text-link";
import { Figure, Tile } from "./tile";
import type { Part, SmsUnread } from "../types";

export function SmsTile({ part }: { part: Part<SmsUnread> | undefined }) {
  return (
    <Tile
      title={t("dashboard.sms.title")}
      icon={MessageSquareIcon}
      to="/sms"
      part={part}
    >
      {(s) => (
        <>
          <div className="flex items-center justify-between gap-3">
            <Figure
              value={t("dashboard.sms.unread", { count: s.unread })}
              caption={t("dashboard.sms.total", { count: s.total })}
            />
            {s.unread > 0 && <Badge>{t("dashboard.sms.newBadge")}</Badge>}
          </div>
          <TextLink to="/sms" className="self-start text-xs text-primary">
            {s.unread > 0
              ? t("dashboard.sms.viewUnread")
              : t("dashboard.sms.open")}
          </TextLink>
        </>
      )}
    </Tile>
  );
}
