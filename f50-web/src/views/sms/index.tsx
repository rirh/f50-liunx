import { useIsMobile } from "@/hooks/use-mobile";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { useAtom } from "jotai";
import { MessageSquareTextIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { ConversationList } from "./components/conversation-list";
import { NewMessageDialog } from "./components/new-message-dialog";
import { SmsActions } from "./components/sms-actions";
import { ThreadView } from "./components/thread-view";
import { TransportAlert } from "./components/transport-alert";
import { useConversations, useSmsStatusText } from "./queries";
import { selectedPeerAtom } from "./store/sms";

/**
 * A desktop-client messenger over `f50.sms` that fills the whole content area (the layout gives it no padding):
 * one toolbar, the conversation list on the left, the open thread on the right. On a phone the list and the thread
 * are separate views (the thread has a back button).
 */
export default function SmsPage() {
  const [peer, setPeer] = useAtom(selectedPeerAtom);
  const conversations = useConversations();
  const status = useSmsStatusText();
  const isMobile = useIsMobile();
  const firstPeer = conversations.data?.pages[0]?.conversations[0]?.peer;
  const autoOpened = useRef(false);

  // Side by side, open the newest conversation instead of an empty pane — once per visit, so closing or deleting
  // the thread is not undone. A phone keeps starting from the list.
  useEffect(() => {
    if (autoOpened.current || isMobile || peer || !firstPeer) return;
    autoOpened.current = true;
    setPeer(firstPeer);
  }, [isMobile, peer, firstPeer, setPeer]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TransportAlert transport={conversations.data?.pages[0]?.transport} />
      <div className="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[19rem_1fr] xl:grid-cols-[21rem_1fr]">
        <div
          className={cn(
            "min-h-0 flex-col bg-muted/40",
            peer ? "hidden md:flex" : "flex",
          )}
        >
          <div className="flex shrink-0 items-center gap-2 ps-5 pe-3 pt-4 pb-3">
            <h1
              className="text-xl font-bold tracking-tight"
              title={status || undefined}
            >
              {t("sms.page.title")}
            </h1>
            <span className="ml-auto" />
            <SmsActions />
          </div>
          <div className="min-h-0 flex-1">
            <ConversationList selected={peer} onSelect={setPeer} />
          </div>
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
