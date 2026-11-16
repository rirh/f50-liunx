/**
 * Ported from one-node's `web/src/lib/rules-text` (logic unchanged; error messages through i18n).
 *
 * Clash-style rule text ↔ One Node rules (sing-box route rules, backend
 * pkg/rulespec). One line per condition: `TYPE,VALUE,OUTBOUND[,no-resolve]`,
 * logic as `AND,((…),(…)),OUTBOUND`, `MATCH,OUTBOUND` for the final
 * outbound. Matching keeps sing-box semantics, as the backend's Clash
 * renderer does (routing-rules.md):
 *
 * - DOMAIN-SUFFIX,x.com matches x.com and its subdomains; the subdomains-only
 *   suffix `.x.com` reads and writes as AND((DOMAIN-SUFFIX,x.com),(NOT,((DOMAIN,x.com)))).
 * - IP rules never resolve a domain, with or without `no-resolve`.
 * - GEOSITE,x / GEOIP,X are the rule sets geosite-x / geoip-x;
 *   GEOSITE,x@cn is kept as geosite-x@cn (our text, not mihomo's).
 *
 * A rule the text cannot express (protocol, ip_is_private, a regex with
 * `,()`) is written as `JSON,{match},OUTBOUND`, so nothing is ever dropped.
 *
 * Blocks: a `# remark` line names the rule below it (`# [停用] remark` keeps
 * it disabled), a blank line ends a rule, and adjacent single address lines
 * with the same outbound form one rule (they are alternatives either way).
 */
import { t } from "@/i18n";

/** A sing-box route rule: match fields plus `action` / `outbound`. */
export type RuleJSON = Record<string, unknown>;

export interface TextRule {
  rule: RuleJSON;
  remark?: string;
  enabled: boolean;
}

export interface TextError {
  /** 1-based line number. */
  line: number;
  message: string;
}

export interface ParseResult {
  rules: TextRule[];
  /** Outbound of `MATCH,…`, when present. */
  final?: string;
  errors: TextError[];
}

export const DISABLED_MARK = "[停用]";

type Match = Record<string, unknown>;

const ADDRESS_FIELDS = [
  "domain",
  "domain_suffix",
  "domain_keyword",
  "domain_regex",
  "ip_cidr",
  "ip_is_private",
  "rule_set",
];
const PORT_FIELDS = ["port", "port_range"];

const SIMPLE_TYPES: Record<string, string> = {
  DOMAIN: "domain",
  "DOMAIN-SUFFIX": "domain_suffix",
  "DOMAIN-KEYWORD": "domain_keyword",
  "DOMAIN-REGEX": "domain_regex",
  "IP-CIDR": "ip_cidr",
  "IP-CIDR6": "ip_cidr",
};

// ---------------------------------------------------------------- parse

class LineError extends Error {}

/** Splits `a,(b,c),d` on top-level commas. */
function splitTop(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "(") depth++;
    else if (c === ")") {
      depth--;
      if (depth < 0) throw new LineError(t("proxy.rulesText.parens"));
    } else if (c === "," && depth === 0) {
      parts.push(text.slice(start, i).trim());
      start = i + 1;
    }
  }
  if (depth !== 0) throw new LineError(t("proxy.rulesText.parens"));
  parts.push(text.slice(start).trim());
  return parts;
}

function unwrap(item: string): string {
  if (!item.startsWith("(") || !item.endsWith(")")) {
    throw new LineError(t("proxy.rulesText.wrap", { item }));
  }
  return item.slice(1, -1).trim();
}

function fieldsOf(match: Match) {
  return Object.keys(match).filter((key) => key !== "invert");
}

function isDefault(match: Match) {
  return match.type !== "logical";
}

function groupOf(key: string) {
  if (ADDRESS_FIELDS.includes(key)) return "address";
  if (PORT_FIELDS.includes(key)) return "port";
  return key;
}

function union(a: unknown, b: unknown): unknown[] {
  const list = [...(a as unknown[]), ...(b as unknown[])];
  return list.filter((value, i) => list.indexOf(value) === i);
}

function logical(mode: "and" | "or", rules: Match[]): Match {
  return { type: "logical", mode, rules };
}

/** AND of plain default rules whose groups do not overlap is one default rule. */
function and(subs: Match[]): Match {
  if (subs.length === 2) {
    const [a, b] = subs;
    const suffix = a.domain_suffix as string[] | undefined;
    const domain = b.domain as string[] | undefined;
    if (
      fieldsOf(a).join() === "domain_suffix" &&
      fieldsOf(b).join() === "domain" &&
      !a.invert &&
      b.invert === true &&
      suffix?.length === 1 &&
      domain?.length === 1 &&
      suffix[0] === domain[0] &&
      !suffix[0].startsWith(".")
    ) {
      return { domain_suffix: [`.${suffix[0]}`] };
    }
  }
  const groups = new Set<string>();
  const merged: Match = {};
  for (const sub of subs) {
    if (!isDefault(sub) || sub.invert) return logical("and", subs);
    const own = new Set(fieldsOf(sub).map(groupOf));
    for (const group of own) {
      if (groups.has(group)) return logical("and", subs);
    }
    own.forEach((group) => groups.add(group));
    Object.assign(merged, sub);
  }
  return merged;
}

/** OR of plain default rules in one field group is one default rule. */
function or(subs: Match[]): Match {
  const groups = new Set<string>();
  for (const sub of subs) {
    if (!isDefault(sub) || sub.invert) return logical("or", subs);
    fieldsOf(sub).forEach((key) => groups.add(groupOf(key)));
  }
  if (groups.size !== 1) return logical("or", subs);
  const merged: Match = {};
  for (const sub of subs) {
    for (const key of fieldsOf(sub)) {
      merged[key] = key in merged ? union(merged[key], sub[key]) : sub[key];
    }
  }
  return merged;
}

function not(match: Match): Match {
  return { ...match, invert: !match.invert };
}

function simpleCondition(type: string, value: string): Match {
  if (!value) throw new LineError(t("proxy.rulesText.noValue", { type }));
  const field = SIMPLE_TYPES[type];
  if (field) return { [field]: [value] };
  switch (type) {
    case "GEOSITE":
      return { rule_set: [`geosite-${value.toLowerCase()}`] };
    case "GEOIP":
      return { rule_set: [`geoip-${value.toLowerCase()}`] };
    case "DST-PORT": {
      const range = /^(\d+)-(\d+)$/.exec(value);
      if (range) {
        const [first, last] = [Number(range[1]), Number(range[2])];
        if (first < 1 || last > 65535 || first > last) {
          throw new LineError(t("proxy.rulesText.badRange", { value }));
        }
        return { port_range: [`${first}:${last}`] };
      }
      const port = Number(value);
      if (!/^\d+$/.test(value) || port < 1 || port > 65535) {
        throw new LineError(t("proxy.rulesText.badPort", { value }));
      }
      return { port: [port] };
    }
    case "NETWORK": {
      const network = value.toLowerCase();
      if (network !== "tcp" && network !== "udp") {
        throw new LineError(t("proxy.rulesText.badNetwork", { value }));
      }
      return { network: [network] };
    }
    default:
      throw new LineError(t("proxy.rulesText.unknownType", { type }));
  }
}

/**
 * Parses a condition. `parts` are its top-level comma parts; the leftover
 * parts (outbound, no-resolve) are returned for the caller.
 */
function condition(parts: string[]): { match: Match; rest: string[] } {
  const type = parts[0]?.toUpperCase() ?? "";
  if (type === "AND" || type === "OR" || type === "NOT") {
    const body = parts[1];
    if (!body) throw new LineError(t("proxy.rulesText.noSubs", { type }));
    const subs = splitTop(unwrap(body)).map((item) => {
      const sub = condition(splitTop(unwrap(item)));
      const extra = sub.rest.filter((part) => !isNoResolve(part));
      if (extra.length) {
        throw new LineError(t("proxy.rulesText.subOutbound", { item }));
      }
      return sub.match;
    });
    let match: Match;
    if (type === "NOT") {
      if (subs.length !== 1) throw new LineError(t("proxy.rulesText.notOne"));
      match = not(subs[0]);
    } else {
      if (subs.length < 2)
        throw new LineError(t("proxy.rulesText.twoSubs", { type }));
      match = type === "AND" ? and(subs) : or(subs);
    }
    return { match, rest: parts.slice(2) };
  }
  return { match: simpleCondition(type, parts[1] ?? ""), rest: parts.slice(2) };
}

function isNoResolve(part: string) {
  return part.toLowerCase() === "no-resolve";
}

/** Outbound text → rule action fields. */
function target(name: string): RuleJSON {
  const upper = name.toUpperCase();
  if (upper === "DIRECT") return { outbound: "direct" };
  if (upper === "REJECT" || upper === "REJECT-DROP")
    return { action: "reject" };
  if (upper === "PROXY" || name === "代理") return { outbound: "proxy" };
  return { outbound: name };
}

function finalOutbound(name: string): string {
  const fields = target(name);
  if (fields.action === "reject")
    throw new LineError(t("proxy.rulesText.matchReject"));
  return fields.outbound as string;
}

function jsonLine(rest: string): RuleJSON {
  const end = rest.lastIndexOf("}");
  if (end < 0) throw new LineError(t("proxy.rulesText.jsonShape"));
  let value: unknown;
  try {
    value = JSON.parse(rest.slice(0, end + 1));
  } catch {
    throw new LineError(t("proxy.rulesText.jsonSyntax"));
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new LineError(t("proxy.rulesText.jsonObject"));
  }
  const rule = value as RuleJSON;
  const tail = rest.slice(end + 1).trim();
  if (!tail) {
    if (rule.action === undefined && rule.outbound === undefined) {
      throw new LineError(t("proxy.rulesText.noOutbound"));
    }
    return rule;
  }
  if (!tail.startsWith(","))
    throw new LineError(t("proxy.rulesText.jsonComma"));
  const { action: _action, outbound: _outbound, ...match } = rule;
  return { ...match, ...target(tail.slice(1).trim()) };
}

function parseLine(line: string): RuleJSON {
  if (/^json\s*,/i.test(line)) return jsonLine(line.replace(/^json\s*,/i, ""));
  const { match, rest } = condition(splitTop(line));
  const names = rest.filter((part) => !isNoResolve(part));
  if (names.length === 0 || !names[0])
    throw new LineError(t("proxy.rulesText.noOutbound"));
  if (names.length > 1)
    throw new LineError(
      t("proxy.rulesText.extra", { extra: names.slice(1).join(",") }),
    );
  return { ...match, ...target(names[0]) };
}

/** A rule of one address field with one value and no extras: lines that may merge. */
function singleAddress(rule: RuleJSON) {
  const keys = Object.keys(rule).filter(
    (key) => key !== "outbound" && key !== "action",
  );
  return (
    keys.length === 1 &&
    ADDRESS_FIELDS.includes(keys[0]) &&
    Array.isArray(rule[keys[0]])
  );
}

function sameTarget(a: RuleJSON, b: RuleJSON) {
  return a.action === b.action && a.outbound === b.outbound;
}

export interface ParseOptions {
  /** Accept `MATCH,…`; otherwise it is an error. Default true. */
  allowFinal?: boolean;
}

export function parse(text: string, options: ParseOptions = {}): ParseResult {
  const allowFinal = options.allowFinal ?? true;
  const rules: TextRule[] = [];
  const errors: TextError[] = [];
  let final: string | undefined;
  let remark: string | undefined;
  let enabled = true;
  // The rule adjacent lines may still merge into.
  let open: TextRule | undefined;

  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim();
    if (!line) {
      open = undefined;
      remark = undefined;
      enabled = true;
      return;
    }
    if (line.startsWith("#")) {
      let note = line.slice(1).trim();
      if (open) {
        open = undefined;
        remark = undefined;
        enabled = true;
      }
      if (note.startsWith(DISABLED_MARK)) {
        enabled = false;
        note = note.slice(DISABLED_MARK.length).trim();
      }
      if (note) remark = remark ? `${remark} ${note}` : note;
      return;
    }
    try {
      if (/^match\s*,/i.test(line)) {
        if (!allowFinal) throw new LineError(t("proxy.rulesText.noMatch"));
        final = finalOutbound(line.replace(/^match\s*,/i, "").trim());
        open = undefined;
        return;
      }
      const rule = parseLine(line);
      if (
        open &&
        singleAddress(open.rule) &&
        singleAddress(rule) &&
        sameTarget(open.rule, rule)
      ) {
        const { action: _action, outbound: _outbound, ...match } = rule;
        open.rule = {
          ...or([stripTarget(open.rule), match]),
          ...targetOf(open.rule),
        };
        return;
      }
      open = { rule, enabled, ...(remark && { remark }) };
      rules.push(open);
      remark = undefined;
      enabled = true;
    } catch (error) {
      if (!(error instanceof LineError)) throw error;
      errors.push({ line: index + 1, message: error.message });
      open = undefined;
    }
  });
  return { rules, final, errors };
}

function stripTarget(rule: RuleJSON): Match {
  const { action: _action, outbound: _outbound, ...match } = rule;
  return match;
}

function targetOf(rule: RuleJSON): RuleJSON {
  if (rule.action === "reject") return { action: "reject" };
  return { outbound: rule.outbound };
}

// ---------------------------------------------------------------- format

type Cond =
  | { kind: string; payload: string; noResolve?: boolean }
  | { kind: "AND" | "OR" | "NOT"; subs: Cond[] };

const list = (value: unknown): unknown[] =>
  value === undefined ? [] : Array.isArray(value) ? value : [value];

const strings = (value: unknown): string[] | undefined => {
  const values = list(value);
  return values.every((item) => typeof item === "string")
    ? (values as string[])
    : undefined;
};

function join(kind: "AND" | "OR", subs: Cond[]): Cond {
  return subs.length === 1 ? subs[0] : { kind, subs };
}

const FORMAT_FIELDS = new Set([
  "type",
  "mode",
  "rules",
  "invert",
  "domain",
  "domain_suffix",
  "domain_keyword",
  "domain_regex",
  "ip_cidr",
  "rule_set",
  "port",
  "port_range",
  "network",
]);

/** The text condition of a match, or undefined when the text cannot hold it. */
function toCond(match: Match): Cond | undefined {
  if (Object.keys(match).some((key) => !FORMAT_FIELDS.has(key)))
    return undefined;
  let cond: Cond;
  if (match.type === "logical") {
    const mode = match.mode;
    const nested = match.rules;
    if (
      (mode !== "and" && mode !== "or") ||
      !Array.isArray(nested) ||
      nested.length < 2
    ) {
      return undefined;
    }
    const subs: Cond[] = [];
    for (const sub of nested) {
      if (!sub || typeof sub !== "object") return undefined;
      const c = toCond(sub as Match);
      if (!c) return undefined;
      subs.push(c);
    }
    cond = { kind: mode === "and" ? "AND" : "OR", subs };
  } else {
    if (
      match.type !== undefined ||
      match.mode !== undefined ||
      match.rules !== undefined
    ) {
      return undefined;
    }
    const address: Cond[] = [];
    const simple = (field: string, kind: string, noResolve?: boolean) => {
      const values = strings(match[field]);
      if (!values) return false;
      for (const value of values) {
        if (!value || /[,()]/.test(value)) return false;
        address.push({ kind, payload: value, ...(noResolve && { noResolve }) });
      }
      return true;
    };
    if (!simple("domain", "DOMAIN")) return undefined;
    const suffixes = strings(match.domain_suffix);
    if (!suffixes) return undefined;
    for (const value of suffixes) {
      if (!value || /[,()]/.test(value)) return undefined;
      if (value.startsWith(".")) {
        const name = value.slice(1);
        address.push({
          kind: "AND",
          subs: [
            { kind: "DOMAIN-SUFFIX", payload: name },
            { kind: "NOT", subs: [{ kind: "DOMAIN", payload: name }] },
          ],
        });
      } else {
        address.push({ kind: "DOMAIN-SUFFIX", payload: value });
      }
    }
    if (!simple("domain_keyword", "DOMAIN-KEYWORD")) return undefined;
    if (!simple("domain_regex", "DOMAIN-REGEX")) return undefined;
    const cidrs = strings(match.ip_cidr);
    if (!cidrs) return undefined;
    for (const value of cidrs) {
      if (!value || /[,()]/.test(value)) return undefined;
      address.push({
        kind: value.includes(":") ? "IP-CIDR6" : "IP-CIDR",
        payload: value,
        noResolve: true,
      });
    }
    const tags = strings(match.rule_set);
    if (!tags) return undefined;
    for (const tag of tags) {
      if (/[,()]/.test(tag)) return undefined;
      if (tag.startsWith("geosite-")) {
        address.push({ kind: "GEOSITE", payload: tag.slice(8) });
      } else if (tag.startsWith("geoip-") && !tag.includes("@")) {
        address.push({
          kind: "GEOIP",
          payload: tag.slice(6).toUpperCase(),
          noResolve: true,
        });
      } else {
        return undefined;
      }
    }
    const ports: Cond[] = [];
    for (const port of list(match.port)) {
      if (typeof port !== "number") return undefined;
      ports.push({ kind: "DST-PORT", payload: String(port) });
    }
    const ranges = strings(match.port_range);
    if (!ranges) return undefined;
    for (const range of ranges) {
      const [first, last] = range.split(":");
      if (last === undefined) return undefined;
      ports.push({
        kind: "DST-PORT",
        payload: `${first || "1"}-${last || "65535"}`,
      });
    }
    const networks = strings(match.network);
    if (!networks) return undefined;
    const network = networks.map((value) => ({
      kind: "NETWORK",
      payload: value.toUpperCase(),
    }));
    const groups = [address, ports, network]
      .filter((group) => group.length > 0)
      .map((group) => join("OR", group));
    if (groups.length === 0) return undefined;
    cond = join("AND", groups);
  }
  if (match.invert === true) cond = { kind: "NOT", subs: [cond] };
  else if (match.invert !== undefined && match.invert !== false)
    return undefined;
  return cond;
}

function body(cond: Cond): string {
  if ("subs" in cond) {
    return `${cond.kind},(${cond.subs.map((sub) => `(${body(sub)})`).join(",")})`;
  }
  return cond.noResolve
    ? `${cond.kind},${cond.payload},no-resolve`
    : `${cond.kind},${cond.payload}`;
}

function line(cond: Cond, outbound: string): string {
  if (!("subs" in cond) && cond.noResolve) {
    return `${cond.kind},${cond.payload},${outbound},no-resolve`;
  }
  return `${body(cond)},${outbound}`;
}

function outboundText(rule: RuleJSON): string | undefined {
  if (rule.action === "reject")
    return rule.outbound === undefined ? "REJECT" : undefined;
  if (
    (rule.action ?? "route") !== "route" ||
    typeof rule.outbound !== "string"
  ) {
    return undefined;
  }
  if (rule.outbound === "direct") return "DIRECT";
  if (rule.outbound === "proxy") return "PROXY";
  if (!rule.outbound || /[,()#]/.test(rule.outbound)) return undefined;
  return rule.outbound;
}

/** Writes a remark-less one-line form of one rule; JSON when text cannot hold it. */
function ruleLines(rule: RuleJSON): string[] {
  const outbound = outboundText(rule);
  const cond = toCond(stripTarget(rule));
  if (!outbound || !cond) {
    // An odd outbound is kept inside the JSON as well.
    if (!outbound) return [`JSON,${JSON.stringify(rule)}`];
    return [`JSON,${JSON.stringify(stripTarget(rule))},${outbound}`];
  }
  // Address alternatives become one line each; parse merges them back.
  if (cond.kind === "OR" && "subs" in cond && isAddressOnly(rule)) {
    return cond.subs.map((sub) => line(sub, outbound));
  }
  return [line(cond, outbound)];
}

function isAddressOnly(rule: RuleJSON) {
  const match = stripTarget(rule);
  return (
    isDefault(match) &&
    !match.invert &&
    fieldsOf(match).every((key) => ADDRESS_FIELDS.includes(key))
  );
}

export function format(rules: TextRule[], final?: string): string {
  const blocks = rules.map(({ rule, remark, enabled }) => {
    const lines = ruleLines(rule);
    const note = [enabled ? "" : DISABLED_MARK, remark?.trim() ?? ""]
      .filter(Boolean)
      .join(" ");
    return note ? [`# ${note}`, ...lines] : lines;
  });
  const text = blocks.map((block) => block.join("\n")).join("\n\n");
  if (final === undefined) return text ? `${text}\n` : "";
  const finalLine = `MATCH,${outboundText({ outbound: final }) ?? final}`;
  return text ? `${text}\n\n${finalLine}\n` : `${finalLine}\n`;
}
