/**
 * The add/edit node form and its share-link round trip (one-node's `web/src/app/pc/node/form.ts`: the link building
 * and parsing are unchanged; labels and hints are read in the current language). Links are what f50d stores and the
 * engine parses, so the form only builds and reads links. VLESS, VMess and Trojan share the transports current Xray
 * keeps (none / TLS / REALITY on top); OnLink is VLESS without a user (this device's key is the credential)
 * over TCP with REALITY or TLS only; Hysteria2, Shadowsocks, SOCKS5 and HTTP are an address plus credentials.
 */
import { t } from "@/i18n";

export type Protocol =
  | "vless"
  | "onlink"
  | "hysteria2"
  | "shadowsocks"
  | "trojan"
  | "vmess"
  | "socks"
  | "http";
export type Network = "tcp" | "ws" | "httpupgrade" | "grpc" | "xhttp";
export type Security = "none" | "tls" | "reality";

export type NodeForm = {
  protocol: Protocol;
  name: string;
  server: string;
  port: string;
  /** VLESS */
  uuid: string;
  encryption: string;
  flow: "" | "xtls-rprx-vision";
  network: Network;
  /** ws, httpupgrade, xhttp */
  path: string;
  host: string;
  xhttpMode: string;
  /** grpc */
  serviceName: string;
  grpcMulti: boolean;
  authority: string;
  security: Security;
  publicKey: string;
  shortId: string;
  spiderX: string;
  alpn: string;
  fingerprint: string;
  /** TLS: pinned certificate SHA-256 (pcs), ECH config list (ech). */
  pinSha256: string;
  ech: string;
  /** Extensions: mux concurrency ('' = off), TCP Fast Open, ClientHello fragment. */
  mux: string;
  tfo: boolean;
  fragment: Fragment | null;
  /** both */
  sni: string;
  /** Hysteria2, Trojan, Shadowsocks, SOCKS5, HTTP */
  password: string;
  hopPorts: string;
  /** Shadowsocks cipher */
  method: string;
  /** VMess cipher (the link's encryption) */
  cipher: string;
  /** SOCKS5, HTTP */
  username: string;
};

export const PROTOCOLS: { value: Protocol; label: string }[] = [
  { value: "vless", label: "VLESS" },
  { value: "onlink", label: "OnLink" },
  { value: "hysteria2", label: "Hysteria2" },
  { value: "shadowsocks", label: "Shadowsocks" },
  { value: "trojan", label: "Trojan" },
  { value: "vmess", label: "VMess" },
  { value: "socks", label: "SOCKS5" },
  { value: "http", label: "HTTP" },
];

/** The protocols that run over the transport / security choices. */
export const usesTransport = (p: Protocol) =>
  p === "vless" || p === "onlink" || p === "vmess" || p === "trojan";

/** OnLink: no user in the link, the device key (the engine's) is the credential. */
export const isLink = (p: Protocol) => p === "onlink";
export type Fragment = { length: string; interval: string; packets: string };
export const DEFAULT_FRAGMENT: Fragment = {
  length: "40-60",
  interval: "30-50",
  packets: "tlshello",
};

type Choice<T extends string> = { value: T; label: string; hint?: string };

/** The transports a protocol may use: OnLink is TCP only. */
export const networksFor = (p: Protocol) =>
  NETWORKS().filter((n) => !isLink(p) || n.value === "tcp");

/** The securities a protocol may use: OnLink needs REALITY or TLS. */
export const securitiesFor = (p: Protocol) =>
  SECURITIES().filter((s) => !isLink(p) || s.value !== "none");

/** Read in the current language each time. */
export const NETWORKS = (): Choice<Network>[] => [
  { value: "tcp", label: "TCP", hint: t("proxy.form.hint.tcp") },
  { value: "xhttp", label: "XHTTP", hint: t("proxy.form.hint.xhttp") },
  { value: "ws", label: "WebSocket", hint: t("proxy.form.hint.ws") },
  {
    value: "httpupgrade",
    label: "HTTPUpgrade",
    hint: t("proxy.form.hint.httpupgrade"),
  },
  { value: "grpc", label: "gRPC", hint: t("proxy.form.hint.grpc") },
];
/** Read in the current language each time. */
export const SECURITIES = (): Choice<Security>[] => [
  { value: "reality", label: "REALITY", hint: t("proxy.form.hint.reality") },
  { value: "tls", label: "TLS", hint: t("proxy.form.hint.tls") },
  {
    value: "none",
    label: t("common.state.none"),
    hint: t("proxy.form.hint.securityNone"),
  },
];
/** uTLS names Xray accepts; chrome negotiates X25519MLKEM768, which current REALITY servers want. */
/** Read in the current language each time. */
export const FINGERPRINTS = (): Choice<string>[] => [
  { value: "chrome", label: "Chrome", hint: t("proxy.form.hint.recommended") },
  { value: "firefox", label: "Firefox" },
  { value: "safari", label: "Safari" },
  { value: "ios", label: "iOS" },
  { value: "android", label: "Android" },
  { value: "edge", label: "Edge" },
  { value: "360", label: "360" },
  { value: "qq", label: "QQ" },
  {
    value: "random",
    label: t("proxy.form.fp.random"),
    hint: t("proxy.form.hint.fpRandom"),
  },
  {
    value: "randomized",
    label: t("proxy.form.fp.randomized"),
    hint: t("proxy.form.hint.fpRandomized"),
  },
];
export const XHTTP_MODES = ["auto", "packet-up", "stream-up", "stream-one"];
/** Ciphers Xray's Shadowsocks takes; 2022 ones need a base64 key of the cipher's size. */
/** Read in the current language each time. */
export const SS_METHODS = (): Choice<string>[] => [
  {
    value: "2022-blake3-aes-128-gcm",
    label: "2022-blake3-aes-128-gcm",
    hint: t("proxy.form.hint.ss128"),
  },
  {
    value: "2022-blake3-aes-256-gcm",
    label: "2022-blake3-aes-256-gcm",
    hint: t("proxy.form.hint.ss256"),
  },
  {
    value: "2022-blake3-chacha20-poly1305",
    label: "2022-blake3-chacha20-poly1305",
    hint: t("proxy.form.hint.ss256"),
  },
  { value: "aes-128-gcm", label: "aes-128-gcm" },
  { value: "aes-256-gcm", label: "aes-256-gcm" },
  { value: "chacha20-ietf-poly1305", label: "chacha20-ietf-poly1305" },
  { value: "xchacha20-ietf-poly1305", label: "xchacha20-ietf-poly1305" },
];
/** Read in the current language each time. */
export const VMESS_CIPHERS = (): Choice<string>[] => [
  { value: "auto", label: "auto", hint: t("proxy.form.hint.vmessAuto") },
  { value: "aes-128-gcm", label: "aes-128-gcm" },
  { value: "chacha20-poly1305", label: "chacha20-poly1305" },
  { value: "none", label: "none", hint: t("proxy.form.hint.vmessNone") },
  { value: "zero", label: "zero", hint: t("proxy.form.hint.vmessZero") },
];

/**
 * Defaults follow One Node's managed config: VLESS REALITY + Vision on 443.
 * Trojan starts on TLS over TCP, VMess on TLS over WebSocket, the usual shapes.
 */
export const emptyForm = (protocol: Protocol = "vless"): NodeForm => ({
  ...baseForm(protocol),
  ...(protocol === "trojan" && { security: "tls" as const, flow: "" as const }),
  ...(protocol === "vmess" && {
    security: "tls" as const,
    network: "ws" as const,
    flow: "" as const,
  }),
  ...(protocol === "socks" && { port: "1080", security: "none" as const }),
  ...(protocol === "http" && { port: "8080", security: "none" as const }),
  ...(protocol === "shadowsocks" && { port: "8388" }),
});

const baseForm = (protocol: Protocol): NodeForm => ({
  protocol,
  name: "",
  server: "",
  port: "443",
  uuid: "",
  encryption: "",
  flow: "xtls-rprx-vision",
  network: "tcp",
  path: "",
  host: "",
  xhttpMode: "auto",
  serviceName: "",
  grpcMulti: false,
  authority: "",
  security: "reality",
  publicKey: "",
  shortId: "",
  spiderX: "",
  alpn: "",
  fingerprint: "chrome",
  pinSha256: "",
  ech: "",
  mux: "",
  tfo: false,
  fragment: null,
  sni: "",
  password: "",
  hopPorts: "",
  method: "2022-blake3-aes-128-gcm",
  cipher: "auto",
  username: "",
});

/** Vision only runs over raw TCP with TLS or REALITY. */
export const allowsFlow = (f: NodeForm) =>
  (f.protocol === "vless" || isLink(f.protocol)) &&
  f.network === "tcp" &&
  f.security !== "none";

/** The flow that will actually be written. */
export const flowOf = (f: NodeForm) => (allowsFlow(f) ? f.flow : "");

/** Mux can't carry Vision, and Xray servers refuse TCP inside mux over XHTTP. */
export const allowsMux = (f: NodeForm) => !flowOf(f) && f.network !== "xhttp";

/** REALITY runs over TCP, XHTTP and gRPC only; Xray rejects it on WebSocket and HTTPUpgrade. */
export const realityFits = (f: NodeForm) =>
  f.security !== "reality" ||
  (f.network !== "ws" && f.network !== "httpupgrade");

/** A range like 40-60 or a single number, each within 1–max. */
export const validRange = (v: string, max = 10000) => {
  const m = /^(\d+)(?:-(\d+))?$/.exec(v.trim());
  if (!m) return false;
  const [a, b] = [Number(m[1]), Number(m[2] ?? m[1])];
  return a >= 1 && b <= max && a <= b;
};

/** Fields still needed before saving, as user-facing names. */
export function missingFields(f: NodeForm): string[] {
  const miss: string[] = [];
  if (!f.server.trim()) miss.push(t("proxy.form.field.server"));
  const port = Number(f.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535)
    miss.push(t("proxy.form.field.port"));
  if (usesTransport(f.protocol)) {
    if (isLink(f.protocol)) {
      if (f.network !== "tcp" || f.security === "none")
        miss.push(t("proxy.form.missing.linkTransport"));
    } else if (f.protocol === "trojan" ? !f.password : !f.uuid.trim())
      miss.push(
        f.protocol === "trojan" ? t("proxy.form.field.password") : "UUID",
      );
    if (f.security === "reality" && !f.sni.trim()) miss.push("SNI");
    if (f.security === "reality" && !f.publicKey.trim())
      miss.push(t("proxy.form.field.publicKey"));
    if (!realityFits(f)) miss.push(t("proxy.form.missing.realityTransport"));
    if (f.mux && allowsMux(f) && !validRange(f.mux, 1024))
      miss.push(t("proxy.form.missing.mux"));
    if (
      f.fragment &&
      (!validRange(f.fragment.length) ||
        !validRange(f.fragment.interval) ||
        !(f.fragment.packets === "tlshello" || validRange(f.fragment.packets)))
    )
      miss.push(t("proxy.form.missing.fragment"));
  } else if (f.protocol === "shadowsocks") {
    if (!f.method.trim()) miss.push(t("proxy.form.field.method"));
    if (!f.password) miss.push(t("proxy.form.field.password"));
  } else if (f.protocol === "socks" || f.protocol === "http") {
    if (f.password && !f.username.trim())
      miss.push(t("proxy.form.field.username"));
  } else if (!f.password) {
    miss.push(t("proxy.form.field.password"));
  }
  return miss;
}

const enc = encodeURIComponent;
const hostPort = (f: NodeForm) =>
  `${f.server.includes(":") ? `[${f.server.trim()}]` : f.server.trim()}:${Number(f.port)}`;
const fragment = (f: NodeForm) =>
  f.name.trim() ? `#${enc(f.name.trim())}` : "";

function query(entries: Record<string, string | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(entries)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `?${s}` : "";
}

/** Query keys the form owns; anything else in an edited link (pinSHA256, extra…) is kept. */
const TRANSPORT_KEYS = [
  "security",
  "type",
  "sni",
  "fp",
  "path",
  "host",
  "mode",
  "serviceName",
  "authority",
  "alpn",
  "pbk",
  "sid",
  "spx",
  "pcs",
  "ech",
  "mux",
  "tfo",
  "fragment",
];
const FORM_KEYS = {
  vmess: ["encryption", ...TRANSPORT_KEYS],
  trojan: TRANSPORT_KEYS,
  shadowsocks: [],
  socks: [],
  http: ["security", "sni"],
  vless: [
    "encryption",
    "security",
    "type",
    "sni",
    "fp",
    "flow",
    "path",
    "host",
    "mode",
    "serviceName",
    "authority",
    "alpn",
    "pbk",
    "sid",
    "spx",
    "pcs",
    "ech",
    "mux",
    "tfo",
    "fragment",
  ],
  onlink: [
    "security",
    "type",
    "sni",
    "fp",
    "flow",
    "alpn",
    "pbk",
    "sid",
    "spx",
    "pcs",
    "ech",
    "mux",
    "tfo",
    "fragment",
  ],
  hysteria2: ["sni", "peer", "mport"],
} satisfies Record<Protocol, string[]>;

/** `base`'s query entries the form doesn't own, when it is the same protocol. */
function kept(f: NodeForm, base?: string): Record<string, string> {
  const original = base && parseLink(base);
  if (!original || original.protocol !== f.protocol) return {};
  const q = new URL(base.replace(/^([\w-]+):\/\//, "onenode://")).searchParams;
  const owned = new Set<string>(FORM_KEYS[f.protocol]);
  return Object.fromEntries([...q].filter(([k]) => !owned.has(k)));
}

/**
 * The same link shapes the One Node subgateway renders. Editing an existing
 * node passes its link as `base`, so parameters the form doesn't show survive.
 */
export function buildLink(f: NodeForm, base?: string): string {
  const hp = hostPort(f);
  if (f.protocol === "hysteria2") {
    return `hysteria2://${enc(f.password)}@${hp}${query({ ...kept(f, base), sni: f.sni, mport: f.hopPorts.trim() })}${fragment(f)}`;
  }
  // The plain forms the subgateway renders: ss://method:password@, socks5://user:pass@.
  if (f.protocol === "shadowsocks")
    return `ss://${enc(f.method.trim())}:${enc(f.password)}@${hp}${query(kept(f, base))}${fragment(f)}`;
  if (f.protocol === "socks" || f.protocol === "http") {
    const user = f.username.trim()
      ? `${enc(f.username.trim())}${f.password ? `:${enc(f.password)}` : ""}@`
      : "";
    const scheme =
      f.protocol === "socks"
        ? "socks5"
        : f.security === "tls"
          ? "https"
          : "http";
    const sni = scheme === "https" ? f.sni.trim() : "";
    return `${scheme}://${user}${hp}${query({ sni })}${fragment(f)}`;
  }
  const q: Record<string, string | undefined> = {
    ...kept(f, base),
    encryption:
      f.protocol === "vless"
        ? f.encryption.trim() || "none"
        : f.protocol === "vmess"
          ? f.cipher
          : "",
    security: f.security,
    type: f.network,
    sni: f.sni,
    fp: f.security === "none" ? "" : f.fingerprint,
    flow: flowOf(f),
  };
  if (f.security === "none") q.sni = "";
  if (f.network === "xhttp")
    Object.assign(q, { path: f.path || "/", host: f.host, mode: f.xhttpMode });
  if (f.network === "ws" || f.network === "httpupgrade")
    Object.assign(q, { path: f.path || "/", host: f.host });
  if (f.network === "grpc")
    Object.assign(q, {
      serviceName: f.serviceName.trim(),
      mode: f.grpcMulti ? "multi" : "",
      authority: f.authority.trim(),
    });
  if (f.security === "tls")
    Object.assign(q, {
      alpn: f.alpn,
      pcs: f.pinSha256.trim(),
      ech: f.ech.trim(),
    });
  if (f.security === "reality")
    Object.assign(q, { pbk: f.publicKey, sid: f.shortId, spx: f.spiderX });
  if (f.mux && allowsMux(f)) q.mux = f.mux.trim();
  if (f.tfo) q.tfo = "1";
  if (f.fragment && f.security !== "none")
    q.fragment = [
      f.fragment.length,
      f.fragment.interval,
      f.fragment.packets,
    ].join(",");
  if (isLink(f.protocol)) return `onlink://${hp}${query(q)}${fragment(f)}`;
  const user = f.protocol === "trojan" ? enc(f.password) : f.uuid.trim();
  return `${f.protocol}://${user}@${hp}${query(q)}${fragment(f)}`;
}

/** Reads a (core-normalized) share link back into the form; null for other schemes. */
export function parseLink(link: string): NodeForm | null {
  try {
    const scheme = link.slice(0, link.indexOf("://")).toLowerCase();
    // A non-special scheme keeps explicit default ports (:80, :443) and the userinfo.
    const u = new URL(link.replace(/^([\w-]+):\/\//, "onenode://"));
    const q = u.searchParams;
    const protocol = SCHEMES[scheme];
    if (!protocol) return null;
    const f = emptyForm(protocol);
    f.name = decodeURIComponent(u.hash.slice(1));
    f.server = u.hostname.replace(/^\[|\]$/g, "");
    f.port = u.port || f.port;
    const user = decodeURIComponent(u.username);
    const pass = decodeURIComponent(u.password);
    if (protocol === "hysteria2") {
      f.password = pass ? `${user}:${pass}` : user;
      f.sni = q.get("sni") ?? q.get("peer") ?? "";
      f.hopPorts = q.get("mport") ?? "";
      return f;
    }
    if (protocol === "shadowsocks") {
      f.method = user;
      f.password = pass;
      return f;
    }
    if (protocol === "socks" || protocol === "http") {
      f.username = user;
      f.password = pass;
      f.security = scheme === "https" ? "tls" : "none";
      f.sni = q.get("sni") ?? "";
      return f;
    }
    if (protocol === "trojan") f.password = pass ? `${user}:${pass}` : user;
    else if (protocol !== "onlink") f.uuid = user;
    if (protocol === "vmess") f.cipher = q.get("encryption") || "auto";
    else if (protocol !== "onlink")
      f.encryption =
        q.get("encryption") === "none" ? "" : (q.get("encryption") ?? "");
    const type = q.get("type");
    f.network =
      type === "xhttp" || type === "splithttp"
        ? "xhttp"
        : type === "ws" || type === "httpupgrade" || type === "grpc"
          ? type
          : "tcp";
    f.path = q.get("path") ?? "";
    f.host = q.get("host") ?? "";
    if (f.network === "xhttp") f.xhttpMode = q.get("mode") ?? "auto";
    f.serviceName = q.get("serviceName") ?? "";
    f.grpcMulti = f.network === "grpc" && q.get("mode") === "multi";
    f.authority = q.get("authority") ?? "";
    const security = q.get("security");
    f.security =
      security === "tls" || security === "none" || security === "reality"
        ? security
        : protocol === "trojan"
          ? "tls"
          : protocol === "vmess"
            ? "none"
            : "reality";
    f.pinSha256 = q.get("pcs") ?? "";
    f.ech = q.get("ech") ?? "";
    f.mux = q.get("mux") ?? "";
    f.tfo = q.get("tfo") === "1";
    const frag = q.get("fragment")?.split(",");
    f.fragment =
      frag?.length === 3
        ? { length: frag[0], interval: frag[1], packets: frag[2] }
        : null;
    f.sni = q.get("sni") ?? "";
    f.fingerprint = q.get("fp") ?? "chrome";
    f.alpn = q.get("alpn") ?? "";
    f.publicKey = q.get("pbk") ?? "";
    f.shortId = q.get("sid") ?? "";
    f.spiderX = q.get("spx") ?? "";
    f.flow =
      (protocol === "vless" || protocol === "onlink") &&
      q.get("flow") === "xtls-rprx-vision"
        ? "xtls-rprx-vision"
        : "";
    return f;
  } catch {
    return null;
  }
}

const SCHEMES: Record<string, Protocol> = {
  vless: "vless",
  onlink: "onlink",
  hysteria2: "hysteria2",
  hy2: "hysteria2",
  ss: "shadowsocks",
  trojan: "trojan",
  vmess: "vmess",
  socks: "socks",
  socks5: "socks",
  http: "http",
  https: "http",
};
