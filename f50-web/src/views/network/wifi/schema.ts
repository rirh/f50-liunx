import { t } from "@/i18n";
import { z } from "zod";
import { ap5Channels, channelsFor, widthOf } from "./channels";
import type { WifiInfo, WifiSetArgs } from "./types";
import { pskHint, ssidHint, pskOk, ssidOk } from "../shared/text-rules";

export const WIFI_FIELDS = [
  "ssid",
  "password",
  "band",
  "channel",
  "width",
  "hidden",
  "country",
] as const;

const base = () =>
  z.object({
    ssid: z.string().refine(ssidOk, ssidHint()),
    /** Empty: unchanged. */
    password: z.string().refine((v) => v === "" || pskOk(v), pskHint()),
    band: z.enum(["2.4", "5"]),
    channel: z.string(),
    width: z.enum(["auto", "20", "40", "80"]),
    hidden: z.boolean(),
    country: z.string().regex(/^[A-Z]{2}$/, t("network.wifi.schema.country")),
  });

export type WifiForm = z.infer<ReturnType<typeof base>>;

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
  return base().superRefine((v, ctx) => {
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
              ? t("network.wifi.schema.channel24")
              : t("network.wifi.schema.channel5", { channel: v.channel }),
        });
      }
    }
    if (v.band === "2.4" && v.width === "80") {
      ctx.addIssue({
        code: "custom",
        path: ["width"],
        message: t("network.wifi.schema.width24"),
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
        message: t("network.wifi.schema.noBand5", { country: v.country }),
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
