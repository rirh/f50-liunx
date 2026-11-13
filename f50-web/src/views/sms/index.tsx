import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { useAtom } from "jotai";
import { MessageSquareTextIcon } from "lucide-react";
import { ConversationList } from "./components/conversation-list";
import { NewMessageDialog } from "./components/new-message-dialog";
import { SmsActions } from "./components/sms-actions";
import { ThreadView } from "./components/thread-view";
import { selectedPeerAtom } from "./store/sms";

/**
 * A desktop-client messenger over `f50.sms` that fills the whole content area (the layout gives it no padding):
 * one toolbar, the conversation list on the left, the open thread on the right. On a phone the list and the thread
 * are separate views (the thread has a back button).
 */
export default function SmsPage() {
  const [peer, setPeer] = useAtom(selectedPeerAtom);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-2 border-b px-3">
        <MessageSquareTextIcon className="size-4 text-muted-foreground" />
        <h1 className="text-sm font-semibold">{t("sms.page.title")}</h1>
        <div className="ml-auto flex min-w-0 items-center gap-1 overflow-x-auto">
          <SmsActions />
        </div>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[18rem_1fr] xl:grid-cols-[20rem_1fr]">
        <div
          className={cn(
            "min-h-0 border-r bg-muted/20",
            peer ? "hidden md:block" : "block",
          )}
        >
          <ConversationList selected={peer} onSelect={setPeer} />
        </div>
        <div className={cn("min-h-0", peer ? "block" : "hidden md:block")}>
          {peer ? (
            <ThreadView key={peer} peer={peer} onBack={() => setPeer(null)} />
          ) : (
            <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
              <div className="flex size-10 items-center justify-center rounded-full bg-muted">
                <MessageSquareTextIcon className="size-5" />
              </div>
              <span className="text-sm font-medium text-foreground">
                {t("sms.page.pickTitle")}
              </span>
              <span className="text-xs">{t("sms.page.pickDescription")}</span>
            </div>
          )}
        </div>
      </div>
      <NewMessageDialog onSent={setPeer} />
    </div>
  );
}
