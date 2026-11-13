import { QueryState } from "@/components/f50/query-state";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { t } from "@/i18n";
import type { UseQueryResult } from "@tanstack/react-query";
import { QrCodeIcon } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import type { WifiCredentials, WifiInfo } from "../types";

/**
 * Wi-Fi QR fields end at an unescaped `;`, so only `\ ; , "` are escaped. A `:` stays as it is: some scanners
 * (WeChat's among them) keep the backslash of `\:` and join with a wrong password.
 */
function escape(value: string) {
  return value.replace(/[\\;,"]/g, "\\$&");
}

function wifiCode(credentials: WifiCredentials): string | null {
  const { ssid, password, security, hidden } = credentials;
  const open = security === "open";
  if (!open && ((security !== "wpa2" && security !== "wpa2-wpa3") || !password))
    return null;
  return `WIFI:T:${open ? "nopass" : "WPA"};S:${escape(ssid)};${
    open ? "" : `P:${escape(password!)};`
  }H:${hidden};;`;
}

function Details({ credentials }: { credentials: WifiCredentials }) {
  const value = wifiCode(credentials);
  return (
    <div className="flex flex-col items-center gap-6 sm:flex-row sm:items-center">
      <div className="shrink-0 rounded-xl border bg-white p-3 shadow-xs">
        {value ? (
          <QRCodeSVG
            value={value}
            size={168}
            level="M"
            marginSize={0}
            bgColor="#ffffff"
            fgColor="#000000"
            title={t("network.wifi.share.code", { ssid: credentials.ssid })}
            className="block size-42"
          />
        ) : (
          <div className="flex size-42 items-center justify-center p-2 text-center text-xs text-muted-foreground">
            {t("network.wifi.share.unavailable")}
          </div>
        )}
      </div>
      <div className="flex w-full min-w-0 flex-col gap-4">
        <div className="flex flex-col gap-1">
          <span className="text-xs text-muted-foreground">
            {t("network.wifi.settings.ssid")}
          </span>
          <span className="truncate text-base font-medium">
            {credentials.ssid}
          </span>
        </div>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {t("network.wifi.share.reconnect")}
        </p>
      </div>
    </div>
  );
}

/** The saved hotspot as a scannable code beside its name; the password itself shows only in the settings dialog. */
export function WifiConnectCard({
  info,
  query,
}: {
  info: WifiInfo;
  query: UseQueryResult<WifiCredentials>;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <QrCodeIcon className="size-4 text-muted-foreground" />
          {t("network.wifi.share.title")}
        </CardTitle>
        <CardDescription>
          {!info.enabled || info.client_mode
            ? t("network.wifi.share.inactive")
            : t("network.wifi.share.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <QueryState query={query}>
          {(credentials) => <Details credentials={credentials} />}
        </QueryState>
      </CardContent>
    </Card>
  );
}
