import { useEffect } from "react";
import { Terminal, TerminalButton } from "@/components/f50/terminal";
import { t } from "@/i18n";
import { useAtom } from "jotai";
import { CopyIcon, EraserIcon } from "lucide-react";
import { toast } from "sonner";
import { ConfirmClassDialog } from "./confirm-class-dialog";
import { PromptLine } from "./prompt-line";
import { TimeoutSetting } from "./timeout-setting";
import { Transcript } from "./transcript";
import { useAtSend } from "./use-at-send";
import { outputAtom } from "../store/terminal";
import { asText } from "../transcript-text";

/**
 * The AT console in the shared terminal window: the title bar holds the timeout, copy and clear; the scrollback
 * shows each command and the modem's answer; the prompt sits under it. Clicking the scrollback (without selecting
 * text) puts the caret back on the prompt.
 */
export function AtTerminal() {
  const at = useAtSend();
  const [entries, setEntries] = useAtom(outputAtom);
  const focusPrompt = () => at.form.setFocus("command");
  const pending = at.sending ? at.sentAt : undefined;

  // a keyboard is there: start on the prompt (on a phone this would pop the keyboard up)
  useEffect(() => {
    if (matchMedia("(pointer: fine)").matches) at.form.setFocus("command");
  }, [at.form]);

  const copy = () =>
    navigator.clipboard.writeText(asText(entries)).then(
      () => toast.success(t("tools.at.output.copied")),
      () => toast.error(t("tools.at.output.copyFailed")),
    );

  return (
    <>
      <Terminal
        title={t("tools.at.page.title")}
        status={
          entries.length > 0
            ? t("tools.at.output.count", { count: entries.length })
            : undefined
        }
        actions={
          <>
            <TimeoutSetting at={at} />
            <TerminalButton
              disabled={entries.length === 0}
              aria-label={t("common.action.copy")}
              title={t("common.action.copy")}
              onClick={() => void copy().then(focusPrompt)}
            >
              <CopyIcon />
            </TerminalButton>
            <TerminalButton
              disabled={entries.length === 0}
              aria-label={t("common.action.clear")}
              title={t("common.action.clear")}
              onClick={() => {
                setEntries([]);
                focusPrompt();
              }}
            >
              <EraserIcon />
            </TerminalButton>
          </>
        }
        prompt={<PromptLine at={at} />}
        follow={`${entries.length}:${pending ?? ""}:${at.job?.id ?? ""}`}
        flush
        className="flex-1"
      >
        <div
          className="min-h-full break-all whitespace-pre-wrap"
          onMouseUp={() => {
            if (!window.getSelection()?.toString()) focusPrompt();
          }}
        >
          <Transcript
            entries={entries}
            sentAt={pending}
            job={at.job}
            onJobDone={at.onJobDone}
          />
        </div>
      </Terminal>
      <ConfirmClassDialog
        pending={at.confirm}
        onConfirm={at.acceptConfirm}
        onCancel={at.rejectConfirm}
        onClosed={focusPrompt}
      />
    </>
  );
}
