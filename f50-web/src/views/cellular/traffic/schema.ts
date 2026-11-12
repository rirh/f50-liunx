/** The checks of `f50.traffic set` (f50-data/src/traffic/{mod,store}.rs). */
import { z } from "zod";

export const UNITS = { MB: 1024 ** 2, GB: 1024 ** 3, TB: 1024 ** 4 } as const;
export type CapUnit = keyof typeof UNITS;

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

export const trafficSchema = z
  .object({
    reset_day: z
      .string()
      .regex(/^(?:[1-9]|[12][0-9]|3[01])$/, "1–31 之间的整数"),
    cap: z
      .string()
      .regex(/^(?:|[0-9]+(?:\.[0-9]{1,3})?)$/, "请输入数字，留空或 0 表示不限"),
    unit: z.enum(["MB", "GB", "TB"]),
    warn: z.string().regex(/^(?:[1-9]|[1-9][0-9]|100)$/, "1–100 之间的整数"),
    cut: z.boolean(),
    plan: z
      .string()
      .refine((v) => new TextEncoder().encode(v).length <= 96, "最多 96 字节")
      .refine((v) => !CONTROL.test(v), "不能含控制字符")
      .refine((v) => !v.startsWith("-"), "不能以 - 开头"),
  })
  .superRefine((v, ctx) => {
    if (capBytes(v.cap, v.unit) > Number.MAX_SAFE_INTEGER) {
      ctx.addIssue({ code: "custom", path: ["cap"], message: "数值过大" });
    }
  });

export type TrafficForm = z.infer<typeof trafficSchema>;

/** The cap in bytes as the wire wants it (`0|[1-9][0-9]{0,15}`). */
export function capBytes(cap: string, unit: CapUnit): number {
  return cap === "" ? 0 : Math.round(Number(cap) * UNITS[unit]);
}

/** A byte cap shown in the largest unit that keeps it readable. */
export function capDisplay(bytes: number): { cap: string; unit: CapUnit } {
  if (bytes <= 0) return { cap: "", unit: "GB" };
  const unit: CapUnit =
    bytes >= UNITS.TB ? "TB" : bytes >= UNITS.GB ? "GB" : "MB";
  return { cap: String(Number((bytes / UNITS[unit]).toFixed(3))), unit };
}
