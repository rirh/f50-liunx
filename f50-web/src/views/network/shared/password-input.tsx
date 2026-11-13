import { useState, type ComponentProps } from "react";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { t } from "@/i18n";
import { EyeIcon, EyeOffIcon } from "lucide-react";

export function PasswordInput({
  disabled,
  defaultVisible = false,
  ...props
}: Omit<ComponentProps<typeof InputGroupInput>, "type"> & {
  defaultVisible?: boolean;
}) {
  const [visible, setVisible] = useState(defaultVisible);
  const label = t(
    visible ? "network.shared.hidePassword" : "network.shared.showPassword",
  );
  return (
    <InputGroup>
      <InputGroupInput
        {...props}
        type={visible ? "text" : "password"}
        disabled={disabled}
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
      />
      <InputGroupAddon align="inline-end">
        <InputGroupButton
          size="icon-xs"
          disabled={disabled}
          aria-label={label}
          aria-pressed={visible}
          title={label}
          onClick={() => setVisible((v) => !v)}
        >
          {visible ? <EyeOffIcon /> : <EyeIcon />}
        </InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
  );
}
