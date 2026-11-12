/** The checks of `f50.data set` (f50-data/src/objects.rs, settings.rs `apn_ok`, `credential_ok`). */
import { t } from "@/i18n";
import { z } from "zod";

// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f-\u009f]/;
const PRINTABLE_NO_QUOTE = /^[\x20-\x7e]*$/;

const credential = (what: string) =>
  z
    .string()
    .max(64, t("cellular.data.schema.credentialMax", { what }))
    .refine(
      (v) => PRINTABLE_NO_QUOTE.test(v) && !/["\\]/.test(v),
      t("cellular.data.schema.credentialChars", { what }),
    );

/** Built per call so the messages are in the current language. */
export const dataSchema = () =>
  z
    .object({
      apn: z
        .string()
        .max(100, t("cellular.data.schema.apnMax"))
        .refine(
          (v) => !/["\\]/.test(v) && !CONTROL.test(v),
          t("cellular.data.schema.apnChars"),
        )
        .refine((v) => !v.startsWith("-"), t("cellular.data.schema.apnDash")),
      auth: z.enum(["none", "pap", "chap", "both", "invalid"]),
      username: credential(t("cellular.data.settings.username")).refine(
        (v) => !v.startsWith("-"),
        t("cellular.data.schema.usernameDash"),
      ),
      /** Empty keeps the saved password. */
      password: credential(t("cellular.data.settings.password")),
      pdp: z.enum(["ipv4", "ipv6", "ipv4v6"]),
      roaming: z.boolean(),
    })
    .superRefine((v, ctx) => {
      if (v.auth === "invalid") {
        ctx.addIssue({
          code: "custom",
          path: ["auth"],
          message: t("cellular.data.schema.authInvalid"),
        });
      }
      if (
        (v.auth === "pap" || v.auth === "chap" || v.auth === "both") &&
        v.apn === ""
      ) {
        ctx.addIssue({
          code: "custom",
          path: ["apn"],
          message: t("cellular.data.schema.apnRequired"),
        });
      }
    });

export type DataForm = z.infer<ReturnType<typeof dataSchema>>;
