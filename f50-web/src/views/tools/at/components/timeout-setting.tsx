import { useState } from "react";
import { TerminalButton } from "@/components/f50/terminal";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { TimerIcon } from "lucide-react";
import type { useAtSend } from "./use-at-send";

/** The answer timeout, in the title bar: shows the value, opens to change it (and opens itself on a bad value). */
export function TimeoutSetting({ at }: { at: ReturnType<typeof useAtSend> }) {
  const { form } = at;
  const error = form.formState.errors.timeout;
  const value = form.watch("timeout").trim();
  // a bad value keeps it open until it is fixed or dismissed
  const [open, setOpen] = useState(false);

  return (
    <Popover
      open={open || !!error}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) {
          form.clearErrors("timeout");
          form.setFocus("command");
        }
      }}
    >
      <PopoverTrigger asChild>
        <TerminalButton
          className={cn(
            "tabular-nums aria-expanded:bg-white/10",
            error && "text-[#ff7b72]",
          )}
          title={t("tools.at.form.timeout")}
          aria-label={t("tools.at.form.timeout")}
        >
          <TimerIcon />
          {value || "8"} {t("tools.at.form.seconds")}
        </TerminalButton>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64">
        <Field data-invalid={!!error}>
          <FieldLabel htmlFor="at-timeout">
            {t("tools.at.form.timeout")}
          </FieldLabel>
          <InputGroup>
            <InputGroupInput
              id="at-timeout"
              inputMode="numeric"
              placeholder="8"
              className="text-base sm:text-sm"
              aria-invalid={!!error}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  setOpen(false);
                  form.clearErrors("timeout");
                  form.setFocus("command");
                }
              }}
              {...form.register("timeout", {
                onChange: () => form.clearErrors("timeout"),
              })}
            />
            <InputGroupAddon align="inline-end">
              <InputGroupText>{t("tools.at.form.seconds")}</InputGroupText>
            </InputGroupAddon>
          </InputGroup>
          {error ? (
            <FieldError errors={[error]} />
          ) : (
            <FieldDescription>
              {t("tools.at.form.timeoutHint")}
            </FieldDescription>
          )}
        </Field>
      </PopoverContent>
    </Popover>
  );
}
