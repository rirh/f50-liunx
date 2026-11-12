import { t } from "@/i18n";
import { z } from "zod";

/** admin.rs HOSTNAME_RE: one DNS label. */
const HOSTNAME_RE = /^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;
/** admin.rs ZONE_RE, and never "..". */
export const ZONE_RE =
  /^(UTC|[A-Z][A-Za-z_+-]{1,30}(\/[A-Za-z0-9_+-]{1,30}){1,2})$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f-\u009f]/;

export const hostnameSchema = () =>
  z.object({
    name: z
      .string()
      .min(1, t("system.settings.hostname.required"))
      .max(63, t("system.settings.hostname.tooLong"))
      .regex(HOSTNAME_RE, t("system.settings.hostname.invalid")),
  });

export const timezoneSchema = () =>
  z.object({
    zone: z
      .string()
      .min(1, t("system.settings.timezone.required"))
      .refine(
        (z) => ZONE_RE.test(z) && !z.includes(".."),
        t("system.settings.timezone.invalid"),
      ),
  });

export const passwordSchema = () =>
  z
    .object({
      password: z
        .string()
        .min(1, t("system.settings.password.required"))
        .max(128, t("system.settings.password.tooLong"))
        .refine(
          (s) => new TextEncoder().encode(s).length <= 128,
          t("system.settings.password.tooManyBytes"),
        )
        .refine(
          (s) => !CONTROL_RE.test(s),
          t("system.settings.password.control"),
        ),
      confirm: z.string().min(1, t("system.settings.password.confirmRequired")),
    })
    .refine((v) => v.password === v.confirm, {
      path: ["confirm"],
      message: t("system.settings.password.mismatch"),
    });

export type HostnameValues = z.infer<ReturnType<typeof hostnameSchema>>;
export type TimezoneValues = z.infer<ReturnType<typeof timezoneSchema>>;
export type PasswordValues = z.infer<ReturnType<typeof passwordSchema>>;

/** The zones the browser knows that f50d's rule accepts, UTC first. */
export function zoneList(): string[] {
  const all =
    typeof Intl.supportedValuesOf === "function"
      ? Intl.supportedValuesOf("timeZone")
      : [];
  return ["UTC", ...all.filter((z) => z !== "UTC" && ZONE_RE.test(z))];
}
