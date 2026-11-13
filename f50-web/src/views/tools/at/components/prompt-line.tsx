import { type KeyboardEvent, useState, useRef } from "react";
import { TerminalPrompt } from "@/components/f50/terminal";
import { Spinner } from "@/components/ui/spinner";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { CornerDownLeftIcon } from "lucide-react";
import type { useAtSend } from "./use-at-send";
import { atHistory, atKeys } from "../api";

const QUICK = [
  "ATI",
  "AT+CSQ",
  "AT+CESQ",
  "AT+COPS?",
  "AT+CEREG?",
  "AT+CGDCONT?",
];

/** The prompt line under the scrollback: ↑/↓ walk this boot's history, Enter sends; common commands under it. */
export function PromptLine({ at }: { at: ReturnType<typeof useAtSend> }) {
  const { form } = at;
  const error = form.formState.errors.command;
  const history = useQuery({ queryKey: atKeys.history, queryFn: atHistory });
  const commands = history.data?.history.map((h) => h.command) ?? [];

  // where ↑/↓ is in the history (null: the line being typed, kept in `draft`)
  const [pos, setPos] = useState<number | null>(null);
  const draft = useRef("");

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    if (commands.length === 0) return;
    e.preventDefault();
    let next: number | null;
    if (e.key === "ArrowUp") {
      if (pos === null) draft.current = form.getValues("command");
      next = pos === null ? commands.length - 1 : Math.max(0, pos - 1);
    } else {
      next = pos === null ? null : pos + 1 >= commands.length ? null : pos + 1;
    }
    setPos(next);
    form.setValue("command", next === null ? draft.current : commands[next], {
      shouldDirty: true,
    });
  };

  return (
    <>
      <form
        onSubmit={(e) => {
          setPos(null);
          void at.submit(e);
        }}
      >
        <TerminalPrompt
          id="at-command"
          prompt="AT"
          aria-label={t("tools.at.form.label")}
          autoCapitalize="characters"
          autoCorrect="off"
          enterKeyHint="send"
          placeholder="AT+CSQ"
          aria-invalid={!!error}
          aria-describedby={error ? "at-command-error" : undefined}
          className="text-base sm:text-[12.5px]"
          onKeyDown={onKeyDown}
          {...form.register("command", { onChange: () => setPos(null) })}
          aside={
            <button
              type="submit"
              disabled={at.busy}
              aria-busy={at.busy}
              aria-label={t("tools.at.form.send")}
              title={t("tools.at.form.send")}
              className="-mr-1.5 flex size-7 items-center justify-center rounded-md transition-colors hover:bg-white/10 hover:text-[#e6edf3] disabled:opacity-60"
            >
              {at.busy ? (
                <Spinner className="size-3.5" />
              ) : (
                <CornerDownLeftIcon className="size-3.5" />
              )}
            </button>
          }
        />
      </form>
      {error && (
        <div
          id="at-command-error"
          role="alert"
          className="-mt-1.5 px-3 pb-1 font-sans text-xs text-[#ff7b72]"
        >
          {error.message}
        </div>
      )}
      <div className="flex items-center gap-1.5 px-3 pb-2.5">
        <div
          className="-my-1 flex min-w-0 flex-1 gap-1.5 overflow-x-auto py-1 [scrollbar-width:none]"
          role="group"
          aria-label={t("tools.at.form.quick")}
        >
          {QUICK.map((c) => (
            <button
              key={c}
              type="button"
              disabled={at.busy}
              className="shrink-0 rounded-md bg-white/[0.06] px-2 py-0.5 text-[11.5px] text-[#8b949e] transition-colors hover:bg-white/[0.12] hover:text-[#e6edf3] disabled:opacity-50"
              onClick={() => {
                setPos(null);
                void at.run(c);
                form.setFocus("command");
              }}
            >
              {c}
            </button>
          ))}
        </div>
        <span
          className="hidden shrink-0 font-sans text-[11px] text-[#6e7681] sm:inline"
          title={t("tools.at.form.historyNote")}
        >
          ↑↓{" "}
          {history.isPending
            ? t("tools.at.form.historyLoading")
            : t("tools.at.form.history", { count: commands.length })}
        </span>
      </div>
    </>
  );
}
