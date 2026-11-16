import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { BUILTIN_OUTBOUNDS, outboundKind, outboundLabel } from "../outbound";

/** 去向: 当前节点 / 直连 / 拦截, then the 策略组, then the nodes. A name that is gone shows as missing. */
export function OutboundSelect({
  value,
  onChange,
  groups,
  nodes,
  block = true,
  label,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  /** 策略组 names in this config. */
  groups: string[];
  nodes: string[];
  /** 拦截 (outbound block); not for final. */
  block?: boolean;
  label: string;
  className?: string;
}) {
  const missing = outboundKind(value, groups, nodes) === "missing";
  const builtins = BUILTIN_OUTBOUNDS.filter((o) => block || o !== "block");
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger
        size="sm"
        aria-label={label}
        aria-invalid={missing || undefined}
        className={cn(
          "max-w-40 min-w-0",
          missing && "text-destructive",
          className,
        )}
      >
        <SelectValue>
          {missing
            ? t("proxy.outbound.missing", { name: value })
            : outboundLabel(value)}
        </SelectValue>
      </SelectTrigger>
      <SelectContent position="popper" align="end">
        <SelectGroup>
          {builtins.map((o) => (
            <SelectItem key={o} value={o}>
              {outboundLabel(o)}
            </SelectItem>
          ))}
          {missing && (
            <SelectItem value={value} className="text-destructive">
              {t("proxy.outbound.missing", { name: value })}
            </SelectItem>
          )}
        </SelectGroup>
        {groups.length > 0 && (
          <>
            <SelectSeparator />
            <SelectGroup>
              <SelectLabel>{t("proxy.groups.title")}</SelectLabel>
              {groups.map((g) => (
                <SelectItem key={g} value={g}>
                  {g}
                </SelectItem>
              ))}
            </SelectGroup>
          </>
        )}
        {nodes.length > 0 && (
          <>
            <SelectSeparator />
            <SelectGroup>
              <SelectLabel>{t("proxy.nodes.title")}</SelectLabel>
              {nodes.map((n) => (
                <SelectItem key={n} value={n}>
                  {n}
                </SelectItem>
              ))}
            </SelectGroup>
          </>
        )}
      </SelectContent>
    </Select>
  );
}
