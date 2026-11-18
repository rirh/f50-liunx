import { Link, useNavigate } from "react-router-dom";
import { UserAvatar } from "@/components/f50/user-avatar";
import { t } from "@/i18n";
import { listUnread } from "@/views/sms/api";
import { smsFullTime, smsShortTime, useMinuteTick } from "@/views/sms/lib/time";
import { selectedPeerAtom } from "@/views/sms/store/sms";
import { useQuery } from "@tanstack/react-query";
import { useSetAtom } from "jotai";
import { ChevronRightIcon, MessageSquareIcon } from "lucide-react";

const SHOWN = 3;

/**
 * The dashboard's unread messages: shown only while there are any (`unread`, from the live status), refetched when
 * that count changes. A message opens its conversation on the SMS page.
 */
export function UnreadSms({ unread }: { unread: number }) {
  const q = useQuery({
    // under ["sms"], so the SMS page's invalidations (read, delete, sync) refresh it too
    queryKey: ["sms", "dashboard-unread", unread],
    queryFn: ({ signal }) => listUnread(1, SHOWN, signal),
    enabled: unread > 0,
  });
  const now = useMinuteTick();
  const open = useSetAtom(selectedPeerAtom);
  const navigate = useNavigate();

  const messages = q.data?.messages ?? [];
  if (unread === 0) return null;

  return (
    <section className="flex flex-col gap-1 rounded-2xl bg-card p-4 pb-2 text-card-foreground">
      <header className="flex items-center gap-2 pb-1">
        <span className="flex size-6 items-center justify-center rounded-md bg-[#30B94D] text-white">
          <MessageSquareIcon aria-hidden className="size-3.5" />
        </span>
        <h2 className="text-sm font-semibold">{t("dashboard.sms.unread")}</h2>
        <span className="rounded-full bg-[#FF3B30] px-2 py-0.5 text-xs font-semibold text-white tabular-nums">
          {unread}
        </span>
        <Link
          to="/sms"
          className="group ms-auto flex items-center gap-0.5 text-xs text-muted-foreground hover:text-primary"
        >
          {t("dashboard.sms.all")}
          <ChevronRightIcon
            aria-hidden
            className="size-3.5 transition-transform group-hover:translate-x-0.5"
          />
        </Link>
      </header>
      <ul
        hidden={messages.length === 0}
        className="-mx-2 flex flex-col divide-y divide-border/60"
      >
        {messages.map((m) => (
          <li key={m.id}>
            <button
              type="button"
              className="flex w-full cursor-pointer items-start gap-3 rounded-[10px] px-2 py-2.5 text-left transition-colors outline-none hover:bg-foreground/[0.04] focus-visible:bg-foreground/[0.06]"
              onClick={() => {
                open(m.peer);
                navigate("/sms");
              }}
            >
              <UserAvatar name={m.peer} className="size-9" />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="flex w-full items-baseline gap-2">
                  <span className="min-w-0 truncate text-sm font-semibold tabular-nums">
                    {m.peer}
                  </span>
                  <span
                    className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums"
                    title={smsFullTime(m.time, m.utc_offset)}
                  >
                    {smsShortTime(m.time, m.utc_offset, now)}
                  </span>
                </span>
                <span className="line-clamp-2 text-[13px] leading-snug wrap-anywhere">
                  {m.text}
                </span>
              </span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
