import { useState } from "react";
import { AnimatedSegmentedTabs } from "@/components/ui/animated-segmented-tabs";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { cn } from "@/lib/utils";
import { PlusIcon } from "lucide-react";
import { OutboundSelect } from "./outbound-select";
import { parse as parseText } from "../../lib/rules-text";
import {
  FORMATS,
  PARSERS,
  type RulesMode,
  type RulesText,
} from "../use-rules-text";

const EXAMPLE: unknown[] = [
  { domain_suffix: ["corp.example.com"], outbound: "direct" },
  { domain_keyword: ["adservice"], action: "reject" },
];

const KINDS = ["domain_suffix", "domain", "domain_keyword", "ip_cidr"] as const;
type Kind = (typeof KINDS)[number];

const TEXT_PLACEHOLDER = `DOMAIN-SUFFIX,corp.example.com,DIRECT
DOMAIN-KEYWORD,adservice,REJECT
GEOSITE,openai,PROXY`;

/** 自定义规则: matched first; a quick add row, then the whole list as text (Clash-style) or JSON. */
export function CustomRules({
  rules,
  groups,
  nodes,
}: {
  rules: RulesText;
  groups: string[];
  nodes: string[];
}) {
  const [adding, setAdding] = useState(false);
  const [kind, setKind] = useState<Kind>("domain_suffix");
  const [value, setValue] = useState("");
  const [outbound, setOutbound] = useState("proxy");
  const text = rules.mode === "text";
  const lineErrors = text
    ? parseText(rules.text, { allowFinal: false }).errors
    : [];

  const add = () => {
    const parsed = PARSERS[rules.mode](rules.text);
    if (typeof parsed === "string" || !value.trim()) return;
    const target = outbound === "block" ? { action: "reject" } : { outbound };
    rules.setText(
      FORMATS[rules.mode]([...parsed, { [kind]: [value.trim()], ...target }]),
    );
    setValue("");
    setAdding(false);
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("proxy.rules.custom.title")}</CardTitle>
        <CardDescription>{t("proxy.rules.custom.description")}</CardDescription>
        <CardAction>
          <AnimatedSegmentedTabs
            label={t("proxy.rules.custom.mode")}
            value={rules.mode}
            onValueChange={(next: RulesMode) => rules.setMode(next)}
            options={[
              { value: "text", label: t("proxy.rules.custom.text") },
              { value: "json", label: "JSON" },
            ]}
            triggerClassName="px-2"
          />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={!!rules.error}
            onClick={() => setAdding((v) => !v)}
          >
            {adding ? (
              t("common.action.cancel")
            ) : (
              <>
                <PlusIcon data-icon="inline-start" />
                {t("proxy.rules.custom.add")}
              </>
            )}
          </Button>
          {!rules.text.trim() && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => rules.setText(FORMATS[rules.mode](EXAMPLE))}
            >
              {t("proxy.rules.custom.example")}
            </Button>
          )}
        </div>
        {adding && (
          <div className="flex flex-wrap items-center gap-2 rounded-lg bg-muted/40 p-2">
            <Select value={kind} onValueChange={(v) => setKind(v as Kind)}>
              <SelectTrigger
                size="sm"
                aria-label={t("proxy.rules.custom.kind")}
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent position="popper">
                {KINDS.map((k) => (
                  <SelectItem key={k} value={k}>
                    {t(`proxy.rules.custom.kinds.${k}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input
              aria-label={t("proxy.rules.custom.value")}
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  add();
                }
              }}
              placeholder={kind === "ip_cidr" ? "192.0.2.0/24" : "example.com"}
              spellCheck={false}
              autoCapitalize="off"
              className="h-7 min-w-32 flex-1 font-mono text-xs"
            />
            <OutboundSelect
              label={t("proxy.rules.custom.outbound")}
              value={outbound}
              onChange={setOutbound}
              groups={groups}
              nodes={nodes}
            />
            <Button
              size="sm"
              disabled={!value.trim() || !!rules.error}
              onClick={add}
            >
              {t("common.action.add")}
            </Button>
          </div>
        )}
        <Textarea
          aria-label={t("proxy.rules.custom.title")}
          value={rules.text}
          onChange={(e) => rules.setText(e.target.value)}
          spellCheck={false}
          autoCapitalize="off"
          placeholder={
            text
              ? TEXT_PLACEHOLDER
              : '[ { "domain_suffix": ["example.com"], "outbound": "direct" } ]'
          }
          aria-invalid={!!rules.error || undefined}
          className={cn(
            "max-h-96 min-h-28 font-mono text-xs leading-relaxed whitespace-pre",
          )}
        />
        {rules.error ? (
          <ul className="flex flex-col gap-0.5 text-xs text-destructive">
            {text && lineErrors.length
              ? lineErrors.map((e) => (
                  <li key={`${e.line}-${e.message}`}>
                    {t("proxy.rules.custom.lineError", {
                      line: e.line,
                      message: e.message,
                    })}
                  </li>
                ))
              : [<li key="e">{rules.error}</li>]}
          </ul>
        ) : (
          <p className="text-xs text-muted-foreground">
            {text
              ? t("proxy.rules.custom.textHelp")
              : t("proxy.rules.custom.jsonHelp")}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
