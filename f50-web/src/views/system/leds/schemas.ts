import { z } from "zod";

/** settings.rs `timeout`: digits, no leading zero (0 itself is fine), at most 86400. */
export const timeoutSchema = z.object({
  seconds: z
    .string()
    .trim()
    .min(1, "请输入秒数")
    .regex(/^(0|[1-9][0-9]{0,4})$/, "请输入不带前导零的整数")
    .refine((s) => Number(s) <= 86_400, "最长 86400 秒（24 小时）"),
});

export type TimeoutValues = z.infer<typeof timeoutSchema>;
