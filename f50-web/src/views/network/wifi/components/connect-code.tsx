import { QueryState } from "@/components/f50/query-state";
import { Field, FieldLabel } from "@/components/ui/field";
import { t } from "@/i18n";
import type { UseQueryResult } from "@tanstack/react-query";
import { QRCodeSVG } from "qrcode.react";
import { PasswordInput } from "../../shared/password-input";
import type { WifiCredentials, WifiInfo } from "../types";

/** Wi-Fi QR fields use MECARD escaping, not URL encoding. */
function escape(value: string) {
  return value.replace(/[\\;,:"]/g, "\\$&");
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
    <div className="flex flex-col items-center gap-4">
      {value ? (
        <QRCodeSVG
          value={value}
          size={200}
          level="M"
          marginSize={2}
          bgColor="#ffffff"
          fgColor="#000000"
          title={t("network.wifi.share.code", { ssid: credentials.ssid })}
          className="h-auto max-w-full rounded-lg border"
        />
      ) : (
        <p className="text-sm text-muted-foreground">
          {t("network.wifi.share.unavailable")}
        </p>
      )}
      {credentials.password !== null && (
        <Field>
          <FieldLabel htmlFor="wifi-share-password">
            {t("network.wifi.settings.password")}
          </FieldLabel>
          <PasswordInput
            id="wifi-share-password"
            defaultVisible
            value={credentials.password}
            readOnly
            autoComplete="off"
          />
        </Field>
      )}
    </div>
  );
}

/** The saved hotspot as a scannable code, with its password, shown in the status card itself. */
export function WifiConnectCode({
  info,
  query,
}: {
  info: WifiInfo;
  query: UseQueryResult<WifiCredentials>;
}) {
  return (
    <section className="flex flex-col gap-3 border-t pt-4">
      <div>
        <h3 className="font-medium">{t("network.wifi.share.title")}</h3>
        <p className="text-sm text-muted-foreground">
          {!info.enabled || info.client_mode
            ? t("network.wifi.share.inactive")
            : t("network.wifi.share.description")}
        </p>
      </div>
      <QueryState query={query}>
        {(credentials) => <Details credentials={credentials} />}
      </QueryState>
      <p className="text-xs text-muted-foreground">
        {t("network.wifi.share.reconnect")}
      </p>
    </section>
  );
}
