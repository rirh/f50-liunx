/** The checks of `f50.traffic set` (f50-data/src/traffic/{mod,store}.rs). */
import { t } from "@/i18n";
import { z } from "zod";

export const UNITS = { MB: 1024 ** 2, GB: 1024 ** 3, TB: 1024 ** 4 } as const;
export type CapUnit = keyof typeof UNITS;

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

/** Built per call so the messages are in the current language. */
export const trafficSchema = () =>
  z
    .object({
      reset_day: z
        .string()
        .regex(
          /^(?:[1-9]|[12][0-9]|3[01])$/,
          t("cellular.traffic.schema.resetDay"),
        ),
      cap: z
        .string()
        .regex(
          /^(?:|[0-9]+(?:\.[0-9]{1,3})?)$/,
          t("cellular.traffic.schema.cap"),
        ),
      unit: z.enum(["MB", "GB", "TB"]),
      warn: z
        .string()
        .regex(/^(?:[1-9]|[1-9][0-9]|100)$/, t("cellular.traffic.schema.warn")),
      cut: z.boolean(),
      plan: z
        .string()
        .refine(
          (v) => new TextEncoder().encode(v).length <= 96,
          t("cellular.traffic.schema.planBytes"),
        )
        .refine(
          (v) => !CONTROL.test(v),
          t("cellular.traffic.schema.planControl"),
        )
        .refine(
          (v) => !v.startsWith("-"),
          t("cellular.traffic.schema.planDash"),
        ),
    })
    .superRefine((v, ctx) => {
      if (capBytes(v.cap, v.unit) > Number.MAX_SAFE_INTEGER) {
        ctx.addIssue({
          code: "custom",
          path: ["cap"],
          message: t("cellular.traffic.schema.tooLarge"),
        });
      }
    });

export type TrafficForm = z.infer<ReturnType<typeof trafficSchema>>;

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
