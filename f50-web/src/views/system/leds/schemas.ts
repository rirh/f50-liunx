import { t } from "@/i18n";
import { z } from "zod";

/** settings.rs `timeout`: digits, no leading zero (0 itself is fine), at most 86400. */
export const timeoutSchema = () =>
  z.object({
    seconds: z
      .string()
      .trim()
      .min(1, t("system.leds.timeout.required"))
      .regex(/^(0|[1-9][0-9]{0,4})$/, t("system.leds.timeout.invalid"))
      .refine((s) => Number(s) <= 86_400, t("system.leds.timeout.tooLong")),
  });

export type TimeoutValues = z.infer<ReturnType<typeof timeoutSchema>>;
