/**
 * Dev-only stand-in for `f50.proxy`, for UI work without a device (in the style of one-node's `shell-mock.ts`).
 * Loaded only when `import.meta.env.DEV` and `VITE_PROXY_MOCK=1` (see `api.ts`); a production build never imports it.
 * State lives in memory and resets on reload.
 */
import { ApiError } from "@/lib/ubus";
import type {
  Config,
  Conn,
  GroupStatus,
  LogLine,
  Profile,
  ProxyNode,
  ProxyStatus,
  RulesInfo,
  Subscription,
} from "./types";

const now = () => Math.floor(Date.now() / 1000);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function node(
  id: string,
  name: string,
  link: string,
  extra: Partial<ProxyNode> = {},
): ProxyNode {
  const scheme = link.slice(0, link.indexOf("://"));
  const protocol =
    scheme === "ss"
      ? "shadowsocks"
      : scheme === "hy2"
        ? "hysteria2"
        : scheme === "socks5"
          ? "socks"
          : scheme;
  const u = new URL(link.replace(/^(\w+):\/\//, "onenode://"));
  const kind =
    protocol === "hysteria2"
      ? "hy2"
      : protocol === "shadowsocks"
        ? "ss"
        : protocol === "socks"
          ? "socks"
          : protocol === "http"
            ? "http"
            : u.searchParams.get("security") === "reality"
              ? "reality"
              : ((u.searchParams.get("type") as ProxyNode["kind"]) ?? "tcp");
  return {
    id,
    name,
    kind,
    protocol,
    server: u.hostname,
    port: Number(u.port) || 443,
    link,
    summary: `${protocol.toUpperCase()} / ${kind.toUpperCase()} / UDP`,
    problem: null,
    warnings: [],
    via: null,
    ...extra,
  };
}

const reality = (host: string, name: string) =>
  `vless://0b0a5f4e-1c2d-4e5f-8a9b-0c1d2e3f4a5b@${host}:443?encryption=none&security=reality&type=tcp&sni=www.apple.com&fp=chrome&flow=xtls-rprx-vision&pbk=Zx3m1Ue0nJ7m5aH2gS8kXq3w9pB0uYvLr6tC4dE5fG&sid=6ba85179#${encodeURIComponent(name)}`;

const sub: Subscription = {
  id: "s1",
  name: "",
  url: "https://sub.example.com/api/v1/client/subscribe?token=demo",
  title: "One Node",
  usage: {
    upload: 3.2 * 2 ** 30,
    download: 48.7 * 2 ** 30,
    total: 200 * 2 ** 30,
    expire: now() + 86400 * 41,
  },
  updated_at: now() - 3600 * 5,
  error: null,
  nodes: [
    node("s1-1", "香港 01", reality("hk1.example.com", "香港 01")),
    node("s1-2", "香港 02", reality("hk2.example.com", "香港 02")),
    node("s1-3", "日本 Tokyo", reality("jp1.example.com", "日本 Tokyo")),
    node("s1-4", "🇸🇬 Singapore", reality("sg1.example.com", "Singapore")),
    node(
      "s1-5",
      "US-LA Hysteria2",
      "hysteria2://pass@us1.example.com:8443?sni=us1.example.com&mport=20000-20100#US-LA",
    ),
    node(
      "s1-6",
      "台湾 TW 03",
      "trojan://secret@tw.example.com:443?security=tls&type=ws&path=%2Fws&host=tw.example.com#TW",
      { problem: "unsupported transport: kcp" },
    ),
  ],
};

let profile: Profile = {
  enabled: true,
  routing: "rule",
  selected: "s1-1",
  subscriptions: [sub],
  local_nodes: [
    node(
      "L1",
      "家里 SOCKS",
      "socks5://user:pw@203.0.113.7:1080#%E5%AE%B6%E9%87%8C",
    ),
    node(
      "L2",
      "落地 SS",
      "ss://2022-blake3-aes-128-gcm:a2V5a2V5a2V5a2V5a2V5aw%3D%3D@198.51.100.4:8388#SS",
      { via: "s1-1" },
    ),
  ],
  configs: [
    {
      id: "default",
      name: "默认规则",
      text: "",
      url: null,
      updated_at: null,
      builtin: true,
      info: {
        final: "proxy",
        groups: [],
        source: 6,
        rules: 6,
        warnings: [],
      },
    },
    {
      id: "c1",
      name: "流媒体走日本",
      text: `${JSON.stringify(
        {
          groups: [
            {
              name: "日本",
              type: "urltest",
              filter: "(?i)日本|jp|tokyo",
              interval: 600,
            },
            { name: "香港", type: "select", nodes: ["香港 01", "香港 02"] },
          ],
          apps: [
            {
              app: "openai",
              outbound: "proxy",
              match: { rule_set: ["geosite-openai"] },
            },
            {
              app: "netflix",
              outbound: "日本",
              match: { rule_set: ["geosite-netflix"] },
            },
          ],
          rules: [
            { domain_suffix: ["corp.example.com"], outbound: "direct" },
            { domain_keyword: ["adservice"], action: "reject" },
          ],
          final: "香港",
        },
        null,
        2,
      )}\n`,
      url: null,
      updated_at: now() - 86400,
      builtin: false,
      info: {
        final: "香港",
        groups: [],
        source: 4,
        rules: 4,
        warnings: ["rule 3: geosite-foo is not in the rule library"],
      },
    },
  ],
  active_config: "c1",
  group_selections: { c1: { 香港: "香港 02" } },
  settings: {
    intercept_router: true,
    bypass_macs: ["aa:bb:cc:dd:ee:ff"],
    probe_url: "https://www.gstatic.com/generate_204",
    subscription_refresh_hours: 12,
    rule_sets_url: "https://marseo.eu.org/api/v1/one/rules/",
    auto_failover: true,
    log_level: "warning",
  },
};

let since: number | null = now() - 3725;
let up = 12_000_000;
let down = 340_000_000;
let groups: GroupStatus[] = [
  {
    name: "日本",
    type: "urltest",
    selected: "日本 Tokyo",
    nodes: [{ name: "日本 Tokyo", delay_ms: 182, alive: true }],
  },
  {
    name: "香港",
    type: "select",
    selected: "香港 02",
    nodes: [
      { name: "香港 01", delay_ms: 64, alive: true },
      { name: "香港 02", delay_ms: 71, alive: true },
    ],
  },
];

const nodes = () => [
  ...profile.subscriptions.flatMap((s) => s.nodes),
  ...profile.local_nodes,
];

function status(): ProxyStatus {
  const running = profile.enabled;
  if (running) {
    up += Math.round(20_000 + Math.random() * 80_000);
    down += Math.round(200_000 + Math.random() * 2_000_000);
  }
  const n = nodes().find((x) => x.id === profile.selected);
  return {
    enabled: profile.enabled,
    state: running ? "running" : "off",
    error: null,
    routing: profile.routing,
    config: profile.active_config,
    node: n ? { id: n.id, name: n.name } : null,
    since: running ? since : null,
    up: running ? up : 0,
    down: running ? down : 0,
    mem: 38 * 2 ** 20,
    xray: "26.10.29",
    engine: "0.1.0",
    intercept_router: profile.settings.intercept_router,
    groups: running && profile.routing === "rule" ? groups : [],
    health: running
      ? { online: true, node_ok: true, delay_ms: 88, since: Date.now() - 6e5 }
      : null,
  };
}

const HOSTS = [
  ["www.google.com", "proxy", "tls"],
  ["api.openai.com", "proxy", "tls"],
  ["www.baidu.com", "direct", "tls"],
  ["i0.hdslb.com", "direct", "http1"],
  ["ad.doubleclick.net", "block", ""],
  ["rr3---sn-a5mekn6s.googlevideo.com", "node:日本 Tokyo", "quic"],
  ["github.com", "proxy", "tls"],
  ["dns.alidns.com", "direct", "tls"],
] as const;
const SOURCES = ["192.168.77.120", "192.168.77.134", "192.168.77.1"];
let connId = 100;
let active: Conn[] = [];
let closed: Conn[] = [];
function tickConnections() {
  const t = Date.now();
  for (const c of active) {
    c.down += Math.round(Math.random() * 60_000);
    c.up += Math.round(Math.random() * 6_000);
  }
  if (active.length < 14 || Math.random() < 0.4) {
    const [host, outbound, protocol] =
      HOSTS[Math.floor(Math.random() * HOSTS.length)];
    active.push({
      id: ++connId,
      start: t - Math.round(Math.random() * 90_000),
      end: 0,
      network: protocol === "quic" ? "udp" : "tcp",
      host,
      ip: "142.250.66.4",
      port: 443,
      protocol: protocol || undefined,
      inbound: "tproxy",
      outbound,
      group: outbound.startsWith("node:") ? "日本" : undefined,
      up: Math.round(Math.random() * 20_000),
      down: Math.round(Math.random() * 400_000),
      source: SOURCES[Math.floor(Math.random() * SOURCES.length)],
    });
  }
  if (active.length > 18 || Math.random() < 0.3) {
    const c = active.shift();
    if (c) closed = [{ ...c, end: t }, ...closed].slice(0, 200);
  }
}

let logSeq = 0;
const logs: LogLine[] = [];
const LOG_TEXT = [
  [
    "info",
    "[1234] proxy/vless/outbound: tunneling request to tcp:www.google.com:443 via hk1.example.com:443",
  ],
  ["info", "[1235] app/dispatcher: sniffed domain: api.openai.com"],
  [
    "warning",
    "[1236] transport/internet/tcp: dial tcp 1.2.3.4:443: i/o timeout",
  ],
  ["error", "f50-proxy: probe s1-6: unsupported transport: kcp"],
  [
    "info",
    "[1237] app/router: taking detour [direct] for [tcp:www.baidu.com:443]",
  ],
] as const;
function tickLogs() {
  const n = Math.floor(Math.random() * 3);
  for (let i = 0; i < n; i++) {
    const [level, text] = LOG_TEXT[Math.floor(Math.random() * LOG_TEXT.length)];
    logs.push({
      seq: ++logSeq,
      time: Date.now(),
      run: Date.now() - (since ?? now()) * 1000,
      level,
      text,
    });
  }
  if (logs.length > 1000) logs.splice(0, logs.length - 1000);
}

const jobs = new Map<string, () => unknown>();
let jobId = 0;
function startJob(result: () => unknown) {
  const id = `mock-${++jobId}`;
  jobs.set(id, result);
  return { job: id };
}

export async function mockJob<T>(id: string): Promise<T> {
  await sleep(1200);
  const run = jobs.get(id);
  if (!run) throw new ApiError("not-found", "no such job");
  return run() as T;
}

const info = (text: string): RulesInfo => {
  let rules = 0;
  try {
    const doc = JSON.parse(text || "{}") as {
      rules?: unknown[];
      apps?: unknown[];
      final?: string;
    };
    rules = (doc.rules?.length ?? 0) + (doc.apps?.length ?? 0);
    return {
      final: doc.final ?? "proxy",
      groups: [],
      source: rules,
      rules,
      warnings: [],
    };
  } catch {
    throw new ApiError("bad-value", "rules: invalid JSON", "text", true);
  }
};

export async function mockCall<T>(
  method: string,
  args: Record<string, unknown>,
): Promise<T> {
  await sleep(method === "status" ? 30 : 250);
  const a = args as Record<string, string & number & boolean>;
  const reply = ((): unknown => {
    switch (method) {
      case "status":
        return status();
      case "profile":
        return structuredClone(profile);
      case "set": {
        if (a.enabled === true && !profile.enabled) since = now();
        profile = {
          ...profile,
          ...(a.enabled !== undefined && { enabled: a.enabled }),
          ...(a.routing !== undefined && { routing: a.routing }),
          ...(a.selected !== undefined && { selected: a.selected }),
          ...(a.active_config !== undefined && {
            active_config: a.active_config,
          }),
        };
        return status();
      }
      case "settings_set": {
        const s = { ...profile.settings };
        for (const [k, v] of Object.entries(args)) {
          if (k === "bypass_macs")
            s.bypass_macs = String(v)
              .split(",")
              .map((m) => m.trim())
              .filter(Boolean);
          else (s as Record<string, unknown>)[k] = v;
        }
        profile = { ...profile, settings: s };
        return s;
      }
      case "subscription_add":
        return startJob(() => {
          const id = `s${profile.subscriptions.length + 1}`;
          const copy: Subscription = {
            ...structuredClone(sub),
            id,
            url: a.url,
            name: a.name ?? "",
            title: "Demo",
            nodes: sub.nodes.slice(0, 3).map((n, i) => ({
              ...n,
              id: `${id}-${i + 1}`,
              name: `${n.name} (${id})`,
            })),
          };
          profile = {
            ...profile,
            subscriptions: [...profile.subscriptions, copy],
          };
          return { id, nodes: copy.nodes.length };
        });
      case "subscription_update":
        return startJob(() => {
          profile = {
            ...profile,
            subscriptions: profile.subscriptions.map((s) =>
              a.id === "all" || s.id === a.id
                ? { ...s, updated_at: now(), error: null }
                : s,
            ),
          };
          return {
            updated: a.id === "all" ? profile.subscriptions.length : 1,
            failed: 0,
          };
        });
      case "subscription_set":
        profile = {
          ...profile,
          subscriptions: profile.subscriptions.map((s) =>
            s.id === a.id
              ? {
                  ...s,
                  ...(a.name !== undefined && { name: a.name }),
                  ...(a.url !== undefined && { url: a.url }),
                }
              : s,
          ),
        };
        return structuredClone(profile);
      case "subscription_remove":
        profile = {
          ...profile,
          subscriptions: profile.subscriptions.filter((s) => s.id !== a.id),
        };
        return structuredClone(profile);
      case "node_inspect": {
        const links = String(a.text)
          .split(/\s+/)
          .filter((l) => l.includes("://"));
        if (!links.length)
          throw new ApiError("bad-value", "no share link", "text", true);
        return {
          nodes: links.map((l) => {
            try {
              const {
                id: _id,
                via: _via,
                ...n
              } = node(
                "",
                decodeURIComponent(l.split("#")[1] ?? "") || "node",
                l,
              );
              return n;
            } catch {
              throw new ApiError(
                "bad-value",
                `not a share link: ${l.slice(0, 40)}`,
                "text",
                true,
              );
            }
          }),
        };
      }
      case "node_save": {
        const id = a.id || `L${profile.local_nodes.length + 1}`;
        const n = node(
          id,
          a.name ||
            decodeURIComponent(String(a.link).split("#")[1] ?? "") ||
            id,
          a.link,
          {
            via: a.via || null,
          },
        );
        const isSub = profile.subscriptions.some((s) =>
          s.nodes.some((x) => x.id === id),
        );
        profile = isSub
          ? {
              ...profile,
              subscriptions: profile.subscriptions.map((s) => ({
                ...s,
                nodes: s.nodes.map((x) => (x.id === id ? n : x)),
              })),
            }
          : {
              ...profile,
              local_nodes: profile.local_nodes.some((x) => x.id === id)
                ? profile.local_nodes.map((x) => (x.id === id ? n : x))
                : [...profile.local_nodes, n],
            };
        return n;
      }
      case "node_delete":
        profile = {
          ...profile,
          local_nodes: profile.local_nodes.filter((n) => n.id !== a.id),
        };
        return structuredClone(profile);
      case "probe": {
        const n = nodes().find((x) => x.id === a.id);
        if (!n || n.problem || Math.random() < 0.1)
          throw new ApiError("failed", "probe: timeout", undefined, true);
        return { ms: Math.round(40 + Math.random() * 1600) };
      }
      case "group_select":
        groups = groups.map((g) =>
          g.name === a.group ? { ...g, selected: a.node } : g,
        );
        return groups;
      case "group_test":
        groups = groups.map((g) =>
          g.name === a.group
            ? {
                ...g,
                nodes: g.nodes.map((n) => ({
                  ...n,
                  delay_ms: Math.round(40 + Math.random() * 400),
                })),
              }
            : g,
        );
        return groups;
      case "config_save": {
        const c: Config = {
          id: a.id || `c${profile.configs.length}`,
          name: a.name,
          text: a.text,
          url: a.url || null,
          updated_at: now(),
          builtin: false,
          info: info(a.text),
        };
        profile = {
          ...profile,
          configs: profile.configs.some((x) => x.id === c.id)
            ? profile.configs.map((x) => (x.id === c.id ? c : x))
            : [...profile.configs, c],
        };
        return c;
      }
      case "config_delete":
        profile = {
          ...profile,
          configs: profile.configs.filter((c) => c.id !== a.id),
          active_config:
            profile.active_config === a.id ? "default" : profile.active_config,
        };
        return structuredClone(profile);
      case "config_fetch":
        return {
          text: `${JSON.stringify({ rules: [{ domain_suffix: ["example.org"], outbound: "direct" }], final: "proxy" }, null, 2)}\n`,
        };
      case "rules_check":
        return info(a.text);
      case "connections":
        tickConnections();
        return {
          epoch: "e1",
          active: structuredClone(active),
          closed: structuredClone(closed),
        };
      case "connections_close": {
        const gone = a.id === 0 ? active : active.filter((c) => c.id === a.id);
        closed = [
          ...gone.map((c) => ({ ...c, end: Date.now() })),
          ...closed,
        ].slice(0, 200);
        active = a.id === 0 ? [] : active.filter((c) => c.id !== a.id);
        return { closed: gone.length };
      }
      case "logs": {
        tickLogs();
        const after = a.epoch === "e1" ? Number(a.after ?? 0) : 0;
        return {
          epoch: "e1",
          next: logSeq,
          lines: logs.filter((l) => l.seq > after),
        };
      }
      default:
        throw new ApiError("unknown-method", `f50.proxy.${method}`);
    }
  })();
  return reply as T;
}
