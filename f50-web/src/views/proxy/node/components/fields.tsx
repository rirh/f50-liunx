import { type ReactNode, useId, useState } from "react";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { EyeIcon, EyeOffIcon } from "lucide-react";

/** One text field of the node form; `secret` hides it behind a show button. */
export function TextField({
  label,
  value,
  onChange,
  placeholder,
  mono,
  secret,
  invalid,
  inputMode,
  hint,
  className,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  mono?: boolean;
  secret?: boolean;
  invalid?: boolean;
  inputMode?: React.HTMLAttributes<HTMLInputElement>["inputMode"];
  hint?: ReactNode;
  className?: string;
}) {
  const id = useId();
  const [shown, setShown] = useState(false);
  const common = {
    id,
    value,
    placeholder,
    inputMode,
    spellCheck: false,
    autoCapitalize: "off",
    autoCorrect: "off",
    autoComplete: "off",
    "aria-invalid": invalid || undefined,
    onChange: (e: React.ChangeEvent<HTMLInputElement>) =>
      onChange(e.target.value),
    className: cn(mono && "font-mono text-xs"),
  } as const;
  return (
    <Field data-invalid={invalid || undefined} className={className}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      {secret ? (
        <InputGroup>
          <InputGroupInput {...common} type={shown ? "text" : "password"} />
          <InputGroupAddon align="inline-end">
            <InputGroupButton
              size="icon-xs"
              aria-label={
                shown ? t("common.action.hide") : t("common.action.show")
              }
              onClick={() => setShown((s) => !s)}
            >
              {shown ? <EyeOffIcon /> : <EyeIcon />}
            </InputGroupButton>
          </InputGroupAddon>
        </InputGroup>
      ) : (
        <Input {...common} />
      )}
      {hint && <FieldDescription className="text-xs">{hint}</FieldDescription>}
    </Field>
  );
}

export type Choice<T extends string> = {
  value: T;
  label: string;
  hint?: string;
};

/** A choice from a list; each option may carry one plain line under it. */
export function SelectField<T extends string>({
  label,
  value,
  options,
  onChange,
  hint,
  className,
}: {
  label: string;
  value: T;
  options: readonly Choice<T>[];
  onChange: (v: T) => void;
  hint?: ReactNode;
  className?: string;
}) {
  const id = useId();
  const current = options.find((o) => o.value === value);
  return (
    <Field className={className}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Select value={value} onValueChange={(v) => onChange(v as T)}>
        <SelectTrigger id={id} className="w-full">
          <SelectValue>{current?.label ?? value}</SelectValue>
        </SelectTrigger>
        <SelectContent position="popper">
          {options.map((o) => (
            <SelectItem key={o.value} value={o.value}>
              <span className="flex flex-col">
                <span>{o.label}</span>
                {o.hint && (
                  <span className="text-xs text-muted-foreground">
                    {o.hint}
                  </span>
                )}
              </span>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {(hint ?? current?.hint) && (
        <FieldDescription className="text-xs">
          {hint ?? current?.hint}
        </FieldDescription>
      )}
    </Field>
  );
}

/** An on/off setting with its name and one line, the switch on the right. */
export function SwitchField({
  label,
  checked,
  onChange,
  disabled,
  hint,
  className,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  hint?: ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <Field
      orientation="horizontal"
      data-disabled={disabled || undefined}
      className={className}
    >
      <FieldContent>
        <FieldLabel htmlFor={id}>{label}</FieldLabel>
        {hint && (
          <FieldDescription className="text-xs">{hint}</FieldDescription>
        )}
      </FieldContent>
      <Switch
        id={id}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </Field>
  );
}
