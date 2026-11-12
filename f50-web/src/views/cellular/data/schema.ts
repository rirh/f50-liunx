/** The checks of `f50.data set` (f50-data/src/objects.rs, settings.rs `apn_ok`, `credential_ok`). */
import { z } from "zod";

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;
const PRINTABLE_NO_QUOTE = /^[\x20-\x7e]*$/;

const credential = (what: string) =>
  z
    .string()
    .max(64, `${what}最多 64 个字符`)
    .refine(
      (v) => PRINTABLE_NO_QUOTE.test(v) && !/["\\]/.test(v),
      `${what}只能是可打印 ASCII，且不含引号或反斜杠`,
    );

export const dataSchema = z
  .object({
    apn: z
      .string()
      .max(100, "APN 最多 100 个字符")
      .refine(
        (v) => !/["\\]/.test(v) && !CONTROL.test(v),
        "APN 不能含引号、反斜杠或控制字符",
      )
      .refine((v) => !v.startsWith("-"), "APN 不能以 - 开头"),
    auth: z.enum(["none", "pap", "chap", "both", "invalid"]),
    username: credential("用户名").refine(
      (v) => !v.startsWith("-"),
      "用户名不能以 - 开头",
    ),
    /** Empty keeps the saved password. */
    password: credential("密码"),
    pdp: z.enum(["ipv4", "ipv6", "ipv4v6"]),
    roaming: z.boolean(),
  })
  .superRefine((v, ctx) => {
    if (v.auth === "invalid") {
      ctx.addIssue({
        code: "custom",
        path: ["auth"],
        message: "保存的认证方式无效，请重新选择",
      });
    }
    if (
      (v.auth === "pap" || v.auth === "chap" || v.auth === "both") &&
      v.apn === ""
    ) {
      ctx.addIssue({
        code: "custom",
        path: ["apn"],
        message: "使用认证时必须填写 APN",
      });
    }
  });

export type DataForm = z.infer<typeof dataSchema>;
