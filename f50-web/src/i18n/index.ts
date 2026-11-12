/**
 * The console's languages: 简体中文 (the source) and English.
 *
 * - Messages live in `src/i18n/locales/<locale>/<namespace>.ts`, one namespace per area of the app. The Chinese
 *   files are the source of truth: their shape defines {@link Key}, and every English file is typed against it, so a
 *   key missing in English fails the type check.
 * - `t("dashboard.rate.title")` reads the current locale; `{name}` placeholders take `vars`. In English a key
 *   `<key>_one` is used instead of `<key>` when `vars.count === 1`.
 * - Switching the language remounts the routed app (`LocaleGate` in main.tsx), so `t()` may be called anywhere
 *   during render, including in label helpers outside components; never at module load (a constant's value would
 *   keep the first language).
 * - The choice is stored in localStorage; without one the browser's language decides (Chinese for `zh*`).
 */
import { useSyncExternalStore } from "react";
import { enUS, zhCN } from "date-fns/locale";
import en from "./locales/en";
import zh, { type Messages } from "./locales/zh";

export type Locale = "zh" | "en";
export const LOCALES: { id: Locale; label: string }[] = [
  { id: "zh", label: "简体中文" },
  { id: "en", label: "English" },
];

const STORE_KEY = "f50-locale";
const dictionaries: Record<Locale, Messages> = { zh, en };

type Join<K, P> = K extends string
  ? P extends string
    ? `${K}.${P}`
    : never
  : never;
type Paths<T> = {
  [K in keyof T & string]: T[K] extends string ? K : Join<K, Paths<T[K]>>;
}[keyof T & string];

/** Every message key, from the Chinese messages. */
export type Key = Paths<Messages>;
export type Vars = Record<string, string | number | null | undefined>;

function detect(): Locale {
  try {
    const saved = localStorage.getItem(STORE_KEY);
    if (saved === "zh" || saved === "en") return saved;
  } catch {
    // storage unavailable: fall through to the browser's language
  }
  const langs =
    typeof navigator === "undefined"
      ? []
      : (navigator.languages ?? [navigator.language]);
  return langs.some((l) => l?.toLowerCase().startsWith("zh")) ||
    langs.length === 0
    ? "zh"
    : "en";
}

let locale: Locale = detect();
const listeners = new Set<() => void>();

function applyDocument() {
  if (typeof document !== "undefined")
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
}
applyDocument();

export function getLocale(): Locale {
  return locale;
}

export function setLocale(next: Locale) {
  if (next === locale) return;
  locale = next;
  try {
    localStorage.setItem(STORE_KEY, next);
  } catch {
    // not stored: the choice lasts for this page
  }
  applyDocument();
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** The current locale; components re-render when it changes. */
export function useLocale(): Locale {
  return useSyncExternalStore(subscribe, getLocale, getLocale);
}

function lookup(dict: Messages, key: string): string | undefined {
  let node: unknown = dict;
  for (const part of key.split(".")) {
    if (node == null || typeof node !== "object") return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

/** The message for `key` in the current locale, with `{name}` placeholders filled from `vars`. */
export function t(key: Key, vars?: Vars): string {
  const dict = dictionaries[locale];
  let text: string | undefined;
  if (vars?.count === 1) text = lookup(dict, `${key}_one`);
  text ??= lookup(dict, key) ?? lookup(zh, key) ?? key;
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => {
    const v = vars[name];
    return v == null ? m : String(v);
  });
}

/** date-fns' locale for the current language. */
export function dateLocale() {
  return locale === "zh" ? zhCN : enUS;
}

/** A list joined the way the language does ("、" or ", "). */
export function joinList(items: string[]): string {
  return items.join(locale === "zh" ? "、" : ", ");
}
