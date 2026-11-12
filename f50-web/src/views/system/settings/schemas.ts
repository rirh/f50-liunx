import { z } from "zod";

/** admin.rs HOSTNAME_RE: one DNS label. */
const HOSTNAME_RE = /^[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/;
/** admin.rs ZONE_RE, and never "..". */
export const ZONE_RE =
  /^(UTC|[A-Z][A-Za-z_+-]{1,30}(\/[A-Za-z0-9_+-]{1,30}){1,2})$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f-\u009f]/;

export const hostnameSchema = z.object({
  name: z
    .string()
    .min(1, "请输入主机名")
    .max(63, "最多 63 个字符")
    .regex(HOSTNAME_RE, "只能包含字母、数字和连字符，且不能以连字符开头或结尾"),
});

export const timezoneSchema = z.object({
  zone: z
    .string()
    .min(1, "请选择时区")
    .refine((z) => ZONE_RE.test(z) && !z.includes(".."), "时区名称无效"),
});

export const passwordSchema = z
  .object({
    password: z
      .string()
      .min(1, "请输入新密码")
      .max(128, "最多 128 个字符")
      .refine((s) => new TextEncoder().encode(s).length <= 128, "最多 128 字节")
      .refine((s) => !CONTROL_RE.test(s), "不能包含控制字符"),
    confirm: z.string().min(1, "请再次输入新密码"),
  })
  .refine((v) => v.password === v.confirm, {
    path: ["confirm"],
    message: "两次输入的密码不一致",
  });

export type HostnameValues = z.infer<typeof hostnameSchema>;
export type TimezoneValues = z.infer<typeof timezoneSchema>;
export type PasswordValues = z.infer<typeof passwordSchema>;

/** The zones the browser knows that f50d's rule accepts, UTC first. */
export function zoneList(): string[] {
  const all =
    typeof Intl.supportedValuesOf === "function"
      ? Intl.supportedValuesOf("timeZone")
      : [];
  return ["UTC", ...all.filter((z) => z !== "UTC" && ZONE_RE.test(z))];
}
