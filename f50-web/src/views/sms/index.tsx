import { PageHeader } from "@/components/f50/page-header";
import { Card } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
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
 * A two-pane messenger over `f50.sms`: conversations on the left, the open thread on the right. On a phone the list
 * and the thread are separate views (the thread has a back button).
 */
export default function SmsPage() {
  const [peer, setPeer] = useAtom(selectedPeerAtom);

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title={t("sms.page.title")}
        description={t("sms.page.description")}
        actions={<SmsActions onDeletedAll={() => setPeer(null)} />}
      />
      <Card className="grid h-[calc(100dvh-13rem)] min-h-[28rem] grid-cols-1 gap-0 overflow-hidden py-0 md:grid-cols-[20rem_1fr]">
        <div
          className={cn("min-h-0 border-r", peer ? "hidden md:block" : "block")}
        >
          <ConversationList selected={peer} onSelect={setPeer} />
        </div>
        <div className={cn("min-h-0", peer ? "block" : "hidden md:block")}>
          {peer ? (
            <ThreadView key={peer} peer={peer} onBack={() => setPeer(null)} />
          ) : (
            <Empty className="h-full">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <MessageSquareTextIcon />
                </EmptyMedia>
                <EmptyTitle>{t("sms.page.pickTitle")}</EmptyTitle>
                <EmptyDescription>
                  {t("sms.page.pickDescription")}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </div>
      </Card>
      <NewMessageDialog onSent={setPeer} />
    </div>
  );
}
