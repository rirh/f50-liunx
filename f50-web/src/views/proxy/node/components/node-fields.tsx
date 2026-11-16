/**
 * The node form's fields by protocol (one-node's `node-fields.tsx` and the pushed pages of `sub-pages.tsx`, laid
 * out as sections of one page here): credentials, then for VLESS / VMess / Trojan the transport and the security.
 */
import { FieldSeparator } from "@/components/ui/field";
import { t } from "@/i18n";
import { SelectField, SwitchField, TextField } from "./fields";
import {
  allowsFlow,
  allowsMux,
  DEFAULT_FRAGMENT,
  FINGERPRINTS,
  type Fragment,
  NETWORKS,
  type NodeForm,
  SECURITIES,
  SS_METHODS,
  usesTransport,
  validRange,
  VMESS_CIPHERS,
  XHTTP_MODES,
} from "../../lib/node-form";

export type Setter = <K extends keyof NodeForm>(
  key: K,
) => (value: NodeForm[K]) => void;

const GRID = "grid gap-4 sm:grid-cols-2";

/** Credentials and the protocol's own options (between 端口 and 备注). */
export function CredentialFields({
  form,
  set,
}: {
  form: NodeForm;
  set: Setter;
}) {
  if (usesTransport(form.protocol))
    return (
      <div className={GRID}>
        {form.protocol === "trojan" ? (
          <TextField
            label={t("proxy.form.field.password")}
            value={form.password}
            onChange={set("password")}
            placeholder={t("proxy.form.required")}
            invalid={!form.password}
            secret
          />
        ) : (
          <TextField
            label="UUID"
            value={form.uuid}
            onChange={set("uuid")}
            placeholder={t("proxy.form.required")}
            invalid={!form.uuid.trim()}
            secret
            mono
          />
        )}
        {form.protocol === "vless" && (
          <TextField
            label={t("proxy.form.field.encryption")}
            value={form.encryption}
            onChange={set("encryption")}
            placeholder={t("proxy.form.ph.vlessEncryption")}
            mono
          />
        )}
        {form.protocol === "vmess" && (
          <SelectField
            label={t("proxy.form.field.encryption")}
            value={form.cipher}
            options={VMESS_CIPHERS()}
            onChange={set("cipher")}
          />
        )}
        {allowsFlow(form) && (
          <SelectField
            label="Flow"
            value={form.flow || "none"}
            options={[
              { value: "none", label: t("common.state.none") },
              {
                value: "xtls-rprx-vision",
                label: "xtls-rprx-vision",
                hint: t("proxy.form.hint.vision"),
              },
            ]}
            onChange={(v) => set("flow")(v === "none" ? "" : v)}
          />
        )}
      </div>
    );
  if (form.protocol === "shadowsocks")
    return (
      <div className={GRID}>
        <SelectField
          label={t("proxy.form.field.method")}
          value={form.method}
          options={SS_METHODS()}
          onChange={set("method")}
        />
        <TextField
          label={t("proxy.form.field.password")}
          value={form.password}
          onChange={set("password")}
          placeholder={
            form.method.startsWith("2022-")
              ? t("proxy.form.ph.ssKey")
              : t("proxy.form.required")
          }
          invalid={!form.password}
          secret
          mono
        />
      </div>
    );
  if (form.protocol === "socks" || form.protocol === "http")
    return (
      <div className={GRID}>
        <TextField
          label={t("proxy.form.field.username")}
          value={form.username}
          onChange={set("username")}
          placeholder={t("proxy.form.optional")}
          invalid={!!form.password && !form.username.trim()}
        />
        <TextField
          label={t("proxy.form.field.password")}
          value={form.password}
          onChange={set("password")}
          placeholder={t("proxy.form.optional")}
          secret
        />
        {form.protocol === "http" && (
          <SwitchField
            label={t("proxy.form.field.httpsProxy")}
            checked={form.security === "tls"}
            onChange={(on) => set("security")(on ? "tls" : "none")}
            className="sm:col-span-2"
          />
        )}
        {form.protocol === "http" && form.security === "tls" && (
          <TextField
            label="SNI"
            value={form.sni}
            onChange={set("sni")}
            placeholder={t("proxy.form.ph.sameAsServer")}
          />
        )}
      </div>
    );
  // Hysteria2
  return (
    <div className={GRID}>
      <TextField
        label={t("proxy.form.field.password")}
        value={form.password}
        onChange={set("password")}
        placeholder={t("proxy.form.required")}
        invalid={!form.password}
        secret
      />
      <TextField
        label="SNI"
        value={form.sni}
        onChange={set("sni")}
        placeholder={t("proxy.form.ph.sameAsServer")}
      />
      <TextField
        label={t("proxy.form.field.hopPorts")}
        value={form.hopPorts}
        onChange={set("hopPorts")}
        placeholder={t("proxy.form.ph.hopPorts")}
        mono
      />
    </div>
  );
}

/** 传输: the network and its settings, then mux and TCP Fast Open. */
export function TransportFields({
  form,
  set,
}: {
  form: NodeForm;
  set: Setter;
}) {
  const n = form.network;
  return (
    <div className="flex flex-col gap-4">
      <div className={GRID}>
        <SelectField
          label={t("proxy.form.field.network")}
          value={n}
          options={NETWORKS()}
          onChange={set("network")}
        />
        {n === "xhttp" && (
          <SelectField
            label={t("proxy.form.field.xhttpMode")}
            value={form.xhttpMode}
            options={XHTTP_MODES.map((m) => ({ value: m, label: m }))}
            onChange={set("xhttpMode")}
            hint={t("proxy.form.hint.xhttpMode")}
          />
        )}
        {(n === "ws" || n === "httpupgrade" || n === "xhttp") && (
          <>
            <TextField
              label={t("proxy.form.field.path")}
              value={form.path}
              onChange={set("path")}
              placeholder="/"
              mono
            />
            <TextField
              label="Host"
              value={form.host}
              onChange={set("host")}
              placeholder={t("proxy.form.optional")}
              hint={t("proxy.form.hint.pathHost")}
            />
          </>
        )}
        {n === "grpc" && (
          <>
            <TextField
              label="Service Name"
              value={form.serviceName}
              onChange={set("serviceName")}
              mono
            />
            <TextField
              label="Authority"
              value={form.authority}
              onChange={set("authority")}
              placeholder={t("proxy.form.optional")}
            />
            <SwitchField
              label={t("proxy.form.field.grpcMulti")}
              checked={form.grpcMulti}
              onChange={set("grpcMulti")}
              hint={t("proxy.form.hint.grpcFields")}
              className="sm:col-span-2"
            />
          </>
        )}
      </div>
      <FieldSeparator />
      <div className={GRID}>
        <SwitchField
          label={t("proxy.form.field.mux")}
          checked={Boolean(form.mux) && allowsMux(form)}
          disabled={!allowsMux(form)}
          onChange={(on) => set("mux")(on ? "8" : "")}
          hint={
            allowsMux(form)
              ? undefined
              : t("proxy.form.hint.noMux", {
                  what: form.network === "xhttp" ? "XHTTP" : "Vision",
                })
          }
        />
        <SwitchField
          label="TCP Fast Open"
          checked={form.tfo}
          onChange={set("tfo")}
        />
        {form.mux && allowsMux(form) && (
          <TextField
            label={t("proxy.form.field.muxConcurrency")}
            value={form.mux}
            onChange={(v) => set("mux")(v.replace(/\D/g, ""))}
            placeholder="8"
            inputMode="numeric"
            invalid={!validRange(form.mux, 1024)}
          />
        )}
      </div>
    </div>
  );
}

/** 安全: none / TLS / REALITY and their fields, the uTLS fingerprint and ClientHello fragmenting. */
export function SecurityFields({ form, set }: { form: NodeForm; set: Setter }) {
  const s = form.security;
  const frag = form.fragment;
  const edit = (patch: Partial<Fragment>) =>
    set("fragment")({ ...(frag ?? DEFAULT_FRAGMENT), ...patch });
  return (
    <div className="flex flex-col gap-4">
      <div className={GRID}>
        <SelectField
          label={t("proxy.form.field.security")}
          value={s}
          options={SECURITIES()}
          onChange={set("security")}
        />
        {s !== "none" && (
          <TextField
            label="SNI"
            value={form.sni}
            onChange={set("sni")}
            placeholder={
              s === "reality"
                ? t("proxy.form.required")
                : t("proxy.form.ph.sameAsServer")
            }
            invalid={s === "reality" && !form.sni.trim()}
          />
        )}
        {s === "reality" && (
          <>
            <TextField
              label={t("proxy.form.field.publicKey")}
              value={form.publicKey}
              onChange={set("publicKey")}
              placeholder={t("proxy.form.required")}
              invalid={!form.publicKey.trim()}
              mono
            />
            <TextField
              label="Short ID"
              value={form.shortId}
              onChange={set("shortId")}
              placeholder={t("proxy.form.optional")}
              mono
            />
            <TextField
              label="SpiderX"
              value={form.spiderX}
              onChange={set("spiderX")}
              placeholder="/"
              mono
              hint={t("proxy.form.hint.realityFields")}
              className="sm:col-span-2"
            />
          </>
        )}
        {s === "tls" && (
          <>
            <TextField
              label="ALPN"
              value={form.alpn}
              onChange={set("alpn")}
              placeholder="h2,http/1.1"
              mono
            />
            <TextField
              label="ECH"
              value={form.ech}
              onChange={set("ech")}
              placeholder={t("proxy.form.ph.ech")}
              mono
            />
            <TextField
              label={t("proxy.form.field.pin")}
              value={form.pinSha256}
              onChange={set("pinSha256")}
              placeholder={t("proxy.form.optional")}
              mono
              hint={t("proxy.form.hint.pin")}
              className="sm:col-span-2"
            />
          </>
        )}
      </div>
      {s !== "none" && (
        <>
          <FieldSeparator />
          <div className={GRID}>
            <SelectField
              label={t("proxy.form.field.fingerprint")}
              value={form.fingerprint}
              options={FINGERPRINTS()}
              onChange={set("fingerprint")}
              hint={t("proxy.form.hint.fingerprint")}
            />
            <SwitchField
              label={t("proxy.form.field.fragment")}
              checked={Boolean(frag)}
              onChange={(on) => set("fragment")(on ? DEFAULT_FRAGMENT : null)}
              hint={t("proxy.form.hint.fragment")}
            />
            {frag && (
              <>
                <TextField
                  label={t("proxy.form.field.fragmentLength")}
                  value={frag.length}
                  onChange={(length) => edit({ length })}
                  placeholder="40-60"
                  invalid={!validRange(frag.length)}
                  mono
                />
                <TextField
                  label={t("proxy.form.field.fragmentInterval")}
                  value={frag.interval}
                  onChange={(interval) => edit({ interval })}
                  placeholder="30-50"
                  invalid={!validRange(frag.interval)}
                  mono
                />
                <TextField
                  label={t("proxy.form.field.fragmentPackets")}
                  value={frag.packets}
                  onChange={(packets) => edit({ packets })}
                  placeholder="tlshello"
                  invalid={
                    frag.packets !== "tlshello" && !validRange(frag.packets)
                  }
                  hint={t("proxy.form.hint.fragmentPackets")}
                  mono
                />
              </>
            )}
          </div>
        </>
      )}
    </div>
  );
}
