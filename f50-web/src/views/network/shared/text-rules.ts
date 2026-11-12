import { t } from "@/i18n";

/** The text rules f50d checks (`f50_core::rpc` and `f50-net/src/rules.rs`), mirrored for the forms. */

const encoder = new TextEncoder();

/** UTF-8 length, as Rust's `str::len`. */
export function byteLength(s: string): number {
  return encoder.encode(s).length;
}

/** One line: no control character (Rust's `char::is_control`: C0, DEL and C1). */
export function isTextLine(s: string): boolean {
  // eslint-disable-next-line no-control-regex
  return !/[\u0000-\u001f\u007f-\u009f]/.test(s);
}

/** Printable ASCII only (0x20-0x7e). */
export function isAsciiLine(s: string): boolean {
  return /^[\x20-\x7e]*$/.test(s);
}

/** A hotspot or network name: 1-32 bytes, one line, no space at either end, not starting with "-". */
export function ssidOk(s: string): boolean {
  return (
    s.length > 0 &&
    byteLength(s) <= 32 &&
    isTextLine(s) &&
    !s.startsWith(" ") &&
    !s.endsWith(" ") &&
    !s.startsWith("-")
  );
}

/** WPA2 passphrase: 8-63 printable ASCII. */
export function pskOk(s: string): boolean {
  return isAsciiLine(s) && s.length >= 8 && s.length <= 63;
}

/** A raw WPA key: 64 hex digits. */
export function rawKeyOk(s: string): boolean {
  return /^[0-9a-fA-F]{64}$/.test(s);
}

export const ssidHint = () => t("network.shared.ssidHint");
export const pskHint = () => t("network.shared.pskHint");
