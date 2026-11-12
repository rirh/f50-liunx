import {
  type BaseSyntheticEvent,
  type KeyboardEvent,
  useRef,
  useState,
} from "react";
import { JobProgress } from "@/components/f50/job-progress";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group";
import { Kbd } from "@/components/ui/kbd";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { CornerDownLeftIcon, XIcon } from "lucide-react";
import { ConfirmClassDialog } from "./confirm-class-dialog";
import { useAtSend } from "./use-at-send";
import { atHistory, atKeys } from "../api";

const QUICK = [
  "ATI",
  "AT+CSQ",
  "AT+CESQ",
  "AT+COPS?",
  "AT+CEREG?",
  "AT+CGDCONT?",
];

/** The command line: ↑/↓ walk this boot's history, Enter sends, an optional answer timeout. */
export function CommandForm() {
  const at = useAtSend();
  const { form } = at;
  const { errors } = form.formState;
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

  const submit = (e?: BaseSyntheticEvent) => {
    setPos(null);
    return at.submit(e);
  };

  return (
    <Card className="gap-3">
      <CardHeader>
        <CardTitle>{t("tools.at.form.title")}</CardTitle>
        <CardDescription>{t("tools.at.form.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <form onSubmit={submit}>
          <FieldGroup className="gap-3">
            <Field data-invalid={!!errors.command}>
              <FieldLabel htmlFor="at-command" className="sr-only">
                {t("tools.at.form.label")}
              </FieldLabel>
              <InputGroup>
                <InputGroupAddon>
                  <InputGroupText className="font-mono">&gt;</InputGroupText>
                </InputGroupAddon>
                <InputGroupInput
                  id="at-command"
                  autoComplete="off"
                  autoCapitalize="characters"
                  spellCheck={false}
                  placeholder="AT+CSQ"
                  className="font-mono text-base sm:text-sm"
                  aria-invalid={!!errors.command}
                  onKeyDown={onKeyDown}
                  {...form.register("command", {
                    onChange: () => setPos(null),
                  })}
                />
                <InputGroupAddon align="inline-end">
                  <InputGroupButton
                    type="submit"
                    variant="default"
                    size="sm"
                    disabled={at.busy}
                    aria-busy={at.busy}
                  >
                    <CornerDownLeftIcon data-icon="inline-start" />
                    {at.sending ? (
                      <SweepShine>{t("tools.at.form.sending")}</SweepShine>
                    ) : at.busy ? (
                      <SweepShine>{t("common.state.running")}</SweepShine>
                    ) : (
                      t("tools.at.form.send")
                    )}
                  </InputGroupButton>
                </InputGroupAddon>
              </InputGroup>
              <FieldError errors={[errors.command]} />
              <FieldDescription>
                <Kbd>↑</Kbd> <Kbd>↓</Kbd>{" "}
                {history.isPending
                  ? t("tools.at.form.historyLoading")
                  : t("tools.at.form.history", { count: commands.length })}
              </FieldDescription>
            </Field>
            <Field data-invalid={!!errors.timeout} className="max-w-56">
              <FieldLabel htmlFor="at-timeout">
                {t("tools.at.form.timeout")}
              </FieldLabel>
              <InputGroup>
                <InputGroupInput
                  id="at-timeout"
                  inputMode="numeric"
                  placeholder="8"
                  className="text-base sm:text-sm"
                  aria-invalid={!!errors.timeout}
                  {...form.register("timeout")}
                />
                <InputGroupAddon align="inline-end">
                  <InputGroupText>{t("tools.at.form.seconds")}</InputGroupText>
                </InputGroupAddon>
              </InputGroup>
              <FieldError errors={[errors.timeout]} />
            </Field>
          </FieldGroup>
        </form>

        <div
          className="flex flex-wrap gap-1.5"
          aria-label={t("tools.at.form.quick")}
        >
          {QUICK.map((c) => (
            <Button
              key={c}
              type="button"
              variant="outline"
              size="xs"
              className="font-mono"
              onClick={() => {
                setPos(null);
                form.setValue("command", c, { shouldValidate: false });
                form.setFocus("command");
              }}
            >
              {c}
            </Button>
          ))}
        </div>

        {at.job && (
          <div className="flex items-start gap-2">
            <div className="min-w-0 flex-1">
              <JobProgress
                id={at.job.id}
                title={t("tools.at.form.jobTitle", { command: at.job.command })}
                onDone={at.onJobDone}
              />
            </div>
            {!at.jobRunning && (
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={t("common.action.close")}
                onClick={at.dismissJob}
              >
                <XIcon />
              </Button>
            )}
          </div>
        )}
      </CardContent>
      <ConfirmClassDialog
        pending={at.confirm}
        onConfirm={at.acceptConfirm}
        onCancel={at.rejectConfirm}
      />
    </Card>
  );
}
