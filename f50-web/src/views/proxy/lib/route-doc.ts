/**
 * A config's text (`Config.text`) as the editor sees it: groups, apps, rules and final (one-node's
 * `web/src/app/pc/lib/route-doc.ts`, its logic unchanged; messages through i18n). Old documents with only rules and
 * final read the same; keys we don't know are kept as they are.
 */
import { t } from "@/i18n";
import { APP_PACKS, type AppMatch, appPack } from "./app-packs";
import type { GroupType } from "../types";

export type { GroupType };

export type RouteGroup = {
  name: string;
  type: GroupType;
  /** Node names, in order. */
  nodes?: string[];
  /** Go RE2, matched against node names. */
  filter?: string;
  /** Seconds, urltest / fallback only. */
  interval?: number;
};

export type RouteApp = { app: string; outbound: string; match: AppMatch };

export type RouteDoc = {
  groups?: RouteGroup[];
  apps?: RouteApp[];
  rules?: unknown[];
  final?: string;
  [key: string]: unknown;
};

export const GROUP_TYPES: readonly GroupType[] = [
  "select",
  "urltest",
  "fallback",
];

export const groupTypeLabel = (type: string) =>
  type === "select" || type === "urltest" || type === "fallback"
    ? t(`proxy.groupType.${type}`)
    : type;
export const groupTypeHint = (type: GroupType) =>
  t(`proxy.groupType.${type}Hint`);

export const RESERVED_OUTBOUNDS = ["direct", "proxy", "block"];
export const DEFAULT_INTERVAL = 300;
export const MIN_INTERVAL = 60;

export type ParsedDoc =
  | { ok: true; doc: RouteDoc }
  /** Not something the cards can show: bad JSON, a whole sing-box config, a bare array. */
  | { ok: false; reason: string };

export function parseDoc(text: string): ParsedDoc {
  if (!text.trim()) return { ok: true, doc: {} };
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (e) {
    return {
      ok: false,
      reason: t("proxy.doc.syntax", { message: (e as Error).message }),
    };
  }
  if (!value || typeof value !== "object" || Array.isArray(value))
    return { ok: false, reason: t("proxy.doc.notObject") };
  const doc = value as RouteDoc;
  if ("route" in doc) return { ok: false, reason: t("proxy.doc.singBox") };
  const arrays = ["groups", "apps", "rules"] as const;
  for (const key of arrays)
    if (doc[key] !== undefined && !Array.isArray(doc[key]))
      return { ok: false, reason: t("proxy.doc.notArray", { key }) };
  if (doc.final !== undefined && typeof doc.final !== "string")
    return { ok: false, reason: t("proxy.doc.finalString") };
  return { ok: true, doc };
}

/** ads first so it blocks before anything else proxies; 国内常用 last. */
const appRank = (id: string) => {
  if (id === "ads") return -1;
  if (id === "cn") return APP_PACKS.length + 1;
  const i = APP_PACKS.findIndex((p) => p.id === id);
  return i < 0 ? APP_PACKS.length : i;
};

/** On save: apps in catalog order with the catalog's current match. */
export function refreshApps(apps: RouteApp[]): RouteApp[] {
  return apps
    .map((a) => {
      const pack = appPack(a.app);
      return pack ? { ...a, match: structuredClone(pack.match) } : a;
    })
    .sort((a, b) => appRank(a.app) - appRank(b.app));
}

/** groups, apps, rules, final first; empty lists left out, so an old document stays old-shaped. */
export function formatDoc(doc: RouteDoc): string {
  const { groups, apps, rules, final, ...rest } = doc;
  const out: RouteDoc = {};
  if (groups?.length) out.groups = groups;
  if (apps?.length) out.apps = apps;
  if (rules?.length) out.rules = rules;
  if (final) out.final = final;
  Object.assign(out, rest);
  if (Object.keys(out).length === 0) return "";
  return `${compactJson(out)}\n`;
}

/**
 * Two-space JSON with lists of plain values on one line. Only whitespace that holds a newline is touched, and JSON
 * strings never contain one.
 */
export function compactJson(value: unknown): string {
  return JSON.stringify(value, null, 2).replace(
    /\[\n\s*([^[\]{}]*?)\n\s*\]/g,
    (_, body: string) => `[${body.replace(/,\n\s*/g, ", ")}]`,
  );
}

/** What gets saved: the document with fresh app matches, or the text untouched if it isn't one. */
export function docForSave(text: string): string {
  const parsed = parseDoc(text);
  if (!parsed.ok) return text;
  const { doc } = parsed;
  return formatDoc({ ...doc, apps: refreshApps(doc.apps ?? []) });
}

/** Same document, formatting aside. */
export function sameDoc(a: string, b: string): boolean {
  if (a === b) return true;
  const x = parseDoc(a);
  const y = parseDoc(b);
  return x.ok && y.ok && formatDoc(x.doc) === formatDoc(y.doc);
}

/**
 * RE2 in the engine; here the browser's RegExp with a leading (?i) as the i flag, enough to preview which nodes a
 * filter picks. Null when invalid.
 */
export function filterRegex(filter: string): RegExp | null {
  const insensitive = filter.startsWith("(?i)");
  try {
    return new RegExp(
      insensitive ? filter.slice(4) : filter,
      insensitive ? "i" : "",
    );
  } catch {
    return null;
  }
}

/** The group's members: nodes that still exist first, then filter matches, no repeats. */
export function groupMembers(group: RouteGroup, nodeNames: string[]): string[] {
  const out = (group.nodes ?? []).filter((n) => nodeNames.includes(n));
  const re = group.filter ? filterRegex(group.filter) : null;
  if (re) for (const n of nodeNames) if (re.test(n)) out.push(n);
  return [...new Set(out)];
}

/** Why a group name can't be used, or null. */
export function groupNameProblem(
  name: string,
  others: string[],
  nodeNames: string[],
): string | null {
  const n = name.trim();
  if (!n) return t("proxy.doc.nameEmpty");
  if (RESERVED_OUTBOUNDS.includes(n))
    return t("proxy.doc.nameReserved", { name: n });
  if (others.includes(n)) return t("proxy.doc.nameTaken");
  if (nodeNames.includes(n)) return t("proxy.doc.nameNode");
  return null;
}

/** Points every outbound named from at to: apps, rules and final. */
export function renameOutbound(
  doc: RouteDoc,
  from: string,
  to: string,
): RouteDoc {
  const swap = (o: unknown) => (o === from ? to : o);
  return {
    ...doc,
    apps: doc.apps?.map((a) => ({
      ...a,
      outbound: swap(a.outbound) as string,
    })),
    rules: doc.rules?.map((r) =>
      r && typeof r === "object" && "outbound" in r
        ? { ...r, outbound: swap((r as { outbound: unknown }).outbound) }
        : r,
    ),
    final: doc.final === undefined ? undefined : (swap(doc.final) as string),
  };
}
