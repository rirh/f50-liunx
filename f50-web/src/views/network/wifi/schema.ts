import { z } from "zod";
import { ap5Channels, channelsFor, widthOf } from "./channels";
import type { WifiInfo, WifiSetArgs } from "./types";
import { PSK_HINT, SSID_HINT, pskOk, ssidOk } from "../shared/text-rules";

export const WIFI_FIELDS = [
  "ssid",
  "password",
  "band",
  "channel",
  "width",
  "hidden",
  "country",
] as const;

const base = z.object({
  ssid: z.string().refine(ssidOk, SSID_HINT),
  /** Empty: unchanged. */
  password: z.string().refine((v) => v === "" || pskOk(v), PSK_HINT),
  band: z.enum(["2.4", "5"]),
  channel: z.string(),
  width: z.enum(["auto", "20", "40", "80"]),
  hidden: z.boolean(),
  country: z.string().regex(/^[A-Z]{2}$/, "两位大写国家代码，如 CN"),
});

export type WifiForm = z.infer<typeof base>;

export function wifiDefaults(info: WifiInfo): WifiForm {
  const width = widthOf(info.htmode) as WifiForm["width"];
  return {
    ssid: info.ssid ?? "",
    password: "",
    band: info.band,
    channel: info.channel,
    width,
    hidden: info.hidden,
    country: info.country,
  };
}

/**
 * The form's rules, as `settings.rs request()` checks the fields it is sent: a channel only when band, channel or
 * country change (an unchanged one is not sent), a country only when it changes.
 */
export function wifiSchema(initial: WifiForm) {
  return base.superRefine((v, ctx) => {
    const bandChanged = v.band !== initial.band;
    const countryChanged = v.country !== initial.country;
    const channelTouched =
      bandChanged || countryChanged || v.channel !== initial.channel;
    if (channelTouched && v.channel !== "auto") {
      const ch = Number(v.channel);
      if (!channelsFor(v.band, v.country).includes(ch)) {
        ctx.addIssue({
          code: "custom",
          path: ["channel"],
          message:
            v.band === "2.4"
              ? "2.4 GHz 信道为 1-13"
              : `该国家不允许 5 GHz 信道 ${v.channel}（或为 DFS 信道）`,
        });
      }
    }
    if (v.band === "2.4" && v.width === "80") {
      ctx.addIssue({
        code: "custom",
        path: ["width"],
        message: "2.4 GHz 最大 40 MHz",
      });
    }
    if (
      v.band === "5" &&
      (bandChanged || countryChanged) &&
      ap5Channels(v.country).length === 0
    ) {
      ctx.addIssue({
        code: "custom",
        path: [bandChanged ? "band" : "country"],
        message: `国家 ${v.country} 没有无 DFS 的 5 GHz 信道，请先切换到 2.4 GHz`,
      });
    }
  });
}

/** Only what changed (an unchanged channel is not resent: a new band starts on its own automatic channel). */
export function wifiArgs(v: WifiForm, initial: WifiForm): WifiSetArgs {
  const a: WifiSetArgs = {};
  if (v.ssid !== initial.ssid) a.ssid = v.ssid;
  if (v.password) a.password = v.password;
  if (v.band !== initial.band) a.band = v.band;
  if (v.channel !== initial.channel || v.band !== initial.band)
    a.channel = v.channel;
  if (v.width !== initial.width) a.width = v.width;
  if (v.hidden !== initial.hidden) a.hidden = v.hidden;
  if (v.country !== initial.country) a.country = v.country;
  return a;
}
