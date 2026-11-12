/**
 * How f50d will split a text into SMS parts (crates/f50-sms/src/pdu/encode.rs, gsm7.rs): GSM 7-bit when every
 * character is in the default alphabet or its extension table (those take two septets), else UCS-2 (UTF-16 units,
 * a surrogate pair takes two). One part holds 160 septets / 70 units; a long message 153 / 67 per part, a character
 * never cut across parts. At most 10 parts, and the API refuses a text over 1530 characters.
 */
import { t } from "@/i18n";

export const MAX_PARTS = 10;
export const MAX_CHARS = 1530;

const GSM_SINGLE = 160;
const GSM_PART = 153;
const UCS2_SINGLE = 70;
const UCS2_PART = 67;

// The default alphabet's code points (septet 27, the escape, is not a character).
const DEFAULT = new Set<number>([
  64, 163, 36, 165, 232, 233, 249, 236, 242, 199, 10, 216, 248, 13, 197, 229,
  916, 95, 934, 915, 923, 937, 928, 936, 931, 920, 926, 198, 230, 223, 201, 32,
  33, 34, 35, 164, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51,
  52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 161, 65, 66, 67, 68, 69, 70,
  71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89,
  90, 196, 214, 209, 220, 167, 191, 97, 98, 99, 100, 101, 102, 103, 104, 105,
  106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120,
  121, 122, 228, 246, 241, 252, 224,
]);
// The extension table: escape + septet.
const EXTENSION = new Set<number>([
  12, 94, 123, 125, 92, 91, 126, 93, 124, 8364,
]);

export interface SmsEncoding {
  coding: "gsm" | "ucs2";
  /** Characters (code points), as the API counts them. */
  chars: number;
  /** Septets (GSM) or UTF-16 units (UCS-2). */
  units: number;
  parts: number;
  /** What one part holds for this length. */
  perPart: number;
  /** Units left in the last part. */
  remaining: number;
  /** Whether f50d would accept it. */
  ok: boolean;
}

function split(sizes: number[], single: number, part: number) {
  const total = sizes.reduce((a, b) => a + b, 0);
  const limit = total <= single ? single : part;
  let parts = 1;
  let used = 0;
  for (const n of sizes) {
    if (used + n > limit) {
      parts++;
      used = 0;
    }
    used += n;
  }
  return { total, limit, parts, remaining: limit - used };
}

export function smsEncoding(text: string): SmsEncoding {
  const chars = Array.from(text);
  const gsm: number[] = [];
  let isGsm = true;
  for (const c of chars) {
    const cp = c.codePointAt(0) ?? 0;
    if (DEFAULT.has(cp)) gsm.push(1);
    else if (EXTENSION.has(cp)) gsm.push(2);
    else {
      isGsm = false;
      break;
    }
  }
  const sizes = isGsm ? gsm : chars.map((c) => c.length);
  const s = isGsm
    ? split(sizes, GSM_SINGLE, GSM_PART)
    : split(sizes, UCS2_SINGLE, UCS2_PART);
  const parts = chars.length === 0 ? 0 : s.parts;
  return {
    coding: isGsm ? "gsm" : "ucs2",
    chars: chars.length,
    units: s.total,
    parts,
    perPart: s.limit,
    remaining: chars.length === 0 ? s.limit : s.remaining,
    ok: chars.length > 0 && chars.length <= MAX_CHARS && parts <= MAX_PARTS,
  };
}

/** The zod refinement message for a text f50d would refuse, or null. */
export function smsTextProblem(text: string): string | null {
  if (!text) return t("sms.form.textRequired");
  const e = smsEncoding(text);
  if (e.chars > MAX_CHARS)
    return t("sms.form.textTooLong", { chars: e.chars, max: MAX_CHARS });
  if (e.parts > MAX_PARTS)
    return t("sms.form.tooManyParts", { parts: e.parts, max: MAX_PARTS });
  return null;
}

/** A number f50d can send to (`\+?[0-9]{1,20}`). */
export const NUMBER_RE = /^\+?[0-9]{1,20}$/;
