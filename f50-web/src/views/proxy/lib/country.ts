/**
 * The country a node is in, from its name (one-node's `web/src/app/pc/components/node-flag.tsx`, the matching
 * unchanged): a flag emoji, a Chinese or English name, or a two-letter code as its own word.
 */

const COUNTRY: Record<string, string> = {
  美国: "US",
  香港: "HK",
  日本: "JP",
  新加坡: "SG",
  台湾: "TW",
  韩国: "KR",
  英国: "GB",
  德国: "DE",
  法国: "FR",
  荷兰: "NL",
  加拿大: "CA",
  俄罗斯: "RU",
  意大利: "IT",
  澳门: "MO",
  马来西亚: "MY",
  泰国: "TH",
  越南: "VN",
  菲律宾: "PH",
  印度尼西亚: "ID",
  印尼: "ID",
  印度: "IN",
  土耳其: "TR",
  巴西: "BR",
  墨西哥: "MX",
  西班牙: "ES",
  瑞典: "SE",
  瑞士: "CH",
  波兰: "PL",
  阿联酋: "AE",
  澳大利亚: "AU",
  澳洲: "AU",
  新西兰: "NZ",
  爱尔兰: "IE",
  芬兰: "FI",
  挪威: "NO",
  丹麦: "DK",
  奥地利: "AT",
  比利时: "BE",
  葡萄牙: "PT",
  捷克: "CZ",
  乌克兰: "UA",
  罗马尼亚: "RO",
  匈牙利: "HU",
  希腊: "GR",
  以色列: "IL",
  沙特: "SA",
  南非: "ZA",
  阿根廷: "AR",
  智利: "CL",
  巴基斯坦: "PK",
  哈萨克斯坦: "KZ",
  蒙古: "MN",
  柬埔寨: "KH",
  尼日利亚: "NG",
  埃及: "EG",
  中国: "CN",
};
const CODES = new Set([
  ...Object.values(COUNTRY),
  "UK",
  "AU",
  "MY",
  "TH",
  "VN",
  "PH",
  "ID",
  "IN",
  "TR",
  "BR",
  "MX",
  "ES",
  "SE",
  "CH",
  "PL",
  "AE",
  "MO",
  "CN",
]);

// English names, matched as whole words: "Japan 01", "Hong Kong-2", "Tokyo".
const ENGLISH: [RegExp, string][] = [
  [
    /\b(united states|america|usa|los angeles|san jose|seattle|new york|silicon valley)\b/i,
    "US",
  ],
  [/\bhong ?kong\b/i, "HK"],
  [/\b(japan|tokyo|osaka)\b/i, "JP"],
  [/\bsingapore\b/i, "SG"],
  [/\b(taiwan|taipei)\b/i, "TW"],
  [/\b(korea|seoul)\b/i, "KR"],
  [/\b(united kingdom|britain|england|london)\b/i, "GB"],
  [/\b(germany|frankfurt)\b/i, "DE"],
  [/\b(france|paris)\b/i, "FR"],
  [/\b(netherlands|amsterdam)\b/i, "NL"],
  [/\bcanada\b/i, "CA"],
  [/\brussia\b/i, "RU"],
  [/\bmacau\b/i, "MO"],
  [/\bmalaysia\b/i, "MY"],
  [/\bthailand\b/i, "TH"],
  [/\bvietnam\b/i, "VN"],
  [/\bphilippines\b/i, "PH"],
  [/\bindonesia\b/i, "ID"],
  [/\bindia\b/i, "IN"],
  [/\bturkey\b/i, "TR"],
  [/\baustralia\b/i, "AU"],
];

/** 🇸🇬 → SG (two regional indicator symbols). */
function emojiCode(name: string): string {
  const m = name.match(/[\u{1F1E6}-\u{1F1FF}]{2}/u);
  if (!m) return "";
  return [...m[0]]
    .map((c) => String.fromCharCode(c.codePointAt(0)! - 0x1f1e6 + 65))
    .join("");
}

/**
 * Country from the node name: a flag emoji, a Chinese or English name, or a two-letter code as its own word anywhere
 * ("US-1", "Qutke-SG", "AWS JP 2"). A code after the first word must be upper case, so "Plug in" isn't India.
 */
export function countryOf(name: string): string {
  const emoji = emojiCode(name);
  if (emoji) return emoji === "UK" ? "GB" : emoji;
  for (const [word, code] of Object.entries(COUNTRY))
    if (name.includes(word)) return code;
  for (const [re, code] of ENGLISH) if (re.test(name)) return code;
  const words = name.split(/[^A-Za-z]+/).filter(Boolean);
  const first = /^[A-Za-z]/.test(name) ? words[0] : undefined;
  const code =
    words.find(
      (w) =>
        w.length === 2 &&
        CODES.has(w.toUpperCase()) &&
        (w === first || w === w.toUpperCase()),
    ) ?? "";
  return code.toUpperCase() === "UK" ? "GB" : code.toUpperCase();
}

/** The name without its flag emoji, for where the flag is already drawn beside it ("🇸🇬 Singapore" → "Singapore"). */
export function plainName(name: string): string {
  return name.replace(/[\u{1F1E6}-\u{1F1FF}]{2}\s*/gu, "").trim() || name;
}
