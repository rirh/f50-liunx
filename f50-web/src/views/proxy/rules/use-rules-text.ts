/**
 * 自定义规则: the document's rules[] on their own, as Clash-style lines (`lib/rules-text`) or one sing-box rule per
 * line of JSON (one-node's `custom-rules.tsx` hook, unchanged in behaviour).
 */
import { useState } from "react";
import { t } from "@/i18n";
import { format as formatText, parse as parseText } from "../lib/rules-text";

export type RulesMode = "text" | "json";

const MODE_KEY = "f50.proxy.rules.mode";

/** One rule per line, so a short list reads like a list. */
export const formatRules = (rules: unknown[]) =>
  rules.length
    ? `[\n${rules.map((r) => `  ${JSON.stringify(r)}`).join(",\n")}\n]`
    : "";

/** The rules array, or why the text isn't one. */
export function parseRules(text: string): unknown[] | string {
  if (!text.trim()) return [];
  try {
    const value: unknown = JSON.parse(text);
    if (!Array.isArray(value)) return t("proxy.rules.custom.notArray");
    if (value.some((r) => !r || typeof r !== "object" || Array.isArray(r)))
      return t("proxy.rules.custom.notObject");
    return value;
  } catch (e) {
    return t("proxy.doc.syntax", { message: (e as Error).message });
  }
}

const asRule = (rule: unknown) => ({
  rule: rule as Record<string, unknown>,
  enabled: true,
});

export const FORMATS: Record<RulesMode, (rules: unknown[]) => string> = {
  json: formatRules,
  text: (rules) => formatText(rules.map(asRule)),
};

export const PARSERS: Record<RulesMode, (text: string) => unknown[] | string> =
  {
    json: parseRules,
    text: (text) => {
      const { rules, errors } = parseText(text, { allowFinal: false });
      if (errors.length)
        return errors
          .map((e) =>
            t("proxy.rules.custom.lineError", {
              line: e.line,
              message: e.message,
            }),
          )
          .join(t("proxy.listSep"));
      return rules.map((r) => r.rule);
    },
  };

function storedMode(): RulesMode {
  try {
    return localStorage.getItem(MODE_KEY) === "json" ? "json" : "text";
  } catch {
    return "text";
  }
}

/**
 * The editor's text over the document's rules. Typing keeps the text as typed (valid edits go into the document at
 * once); a change from elsewhere (the JSON view, a renamed 策略组) starts it over.
 */
export function useRulesText(
  rules: unknown[],
  onChange: (rules: unknown[]) => void,
) {
  const [mode, setModeState] = useState<RulesMode>(storedMode);
  const base = JSON.stringify(rules);
  const [draft, setDraft] = useState<{
    base: string;
    mode: RulesMode;
    text: string;
  } | null>(null);
  const text =
    draft?.base === base && draft.mode === mode
      ? draft.text
      : FORMATS[mode](rules);
  const parsed = PARSERS[mode](text);
  return {
    mode,
    text,
    error: typeof parsed === "string" ? parsed : null,
    /** Switching starts the text over from the document, so it needs no errors. */
    setMode: (next: RulesMode) => {
      if (typeof parsed === "string") return false;
      try {
        localStorage.setItem(MODE_KEY, next);
      } catch {
        // not stored: the choice lasts for this page
      }
      setModeState(next);
      return true;
    },
    setText: (next: string) => {
      const value = PARSERS[mode](next);
      if (typeof value === "string")
        return setDraft({ base, mode, text: next });
      setDraft({ base: JSON.stringify(value), mode, text: next });
      onChange(value);
    },
  };
}

export type RulesText = ReturnType<typeof useRulesText>;
