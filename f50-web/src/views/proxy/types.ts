/** `f50.proxy` replies (f50-proxy/DESIGN.md §2; the f50d crate `f50-proxy`). */

export type Routing = "rule" | "global" | "direct";
export type ProxyState = "off" | "starting" | "running" | "error";
export type LogLevel = "error" | "warning" | "info" | "debug";
export type GroupType = "select" | "urltest" | "fallback";

/** The engine's grouping: the transport for VLESS / VMess / Trojan, else the protocol. */
export type NodeKind =
  | "reality"
  | "xhttp"
  | "tls"
  | "tcp"
  | "ws"
  | "httpupgrade"
  | "grpc"
  | "hy2"
  | "ss"
  | "socks"
  | "http"
  | "";

/** A node as the engine read its share link, plus f50d's id and 前置代理. */
export interface ProxyNode {
  /** `<sub id>-<n>` for a subscription's, `L<n>` for a local one. */
  id: string;
  name: string;
  kind: NodeKind;
  protocol: string;
  server: string;
  port: number;
  link: string;
  /** "VLESS / REALITY / XTLS-RPRX-VISION / UDP". */
  summary: string;
  problem: string | null;
  warnings: string[];
  /** The id of the node this one connects through, or null. */
  via: string | null;
}

/** `node_inspect`: a node the engine read, nothing saved (no id, no via). */
export type InspectedNode = Omit<ProxyNode, "id" | "via">;

export interface Usage {
  upload: number;
  download: number;
  total: number;
  /** Unix seconds, 0 = never. */
  expire: number;
}

export interface Subscription {
  id: string;
  name: string;
  url: string;
  /** The provider's Profile-Title. */
  title: string | null;
  usage: Usage | null;
  updated_at: number | null;
  /** The last refresh's error. */
  error: string | null;
  nodes: ProxyNode[];
}

export interface RouteGroupInfo {
  name: string;
  type: GroupType;
  nodes: string[];
  interval?: number;
}

/** The engine's `/v1/rules/check` reply. */
export interface RulesInfo {
  /** proxy | direct | block | a node or group name. */
  final: string;
  groups: RouteGroupInfo[] | null;
  /** Rules that took effect (apps included). */
  source: number;
  rules: number;
  warnings: string[] | null;
}

export interface Config {
  id: string;
  name: string;
  /** The route document JSON (`{groups, apps, rules, rule_set, final}`). */
  text: string;
  url: string | null;
  updated_at: number | null;
  builtin: boolean;
  info: RulesInfo | null;
}

export interface ProxySettings {
  intercept_router: boolean;
  bypass_macs: string[];
  probe_url: string;
  /** 0 = never. */
  subscription_refresh_hours: number;
  rule_sets_url: string;
  auto_failover: boolean;
  log_level: LogLevel;
}

export interface Profile {
  enabled: boolean;
  routing: Routing;
  selected: string | null;
  subscriptions: Subscription[];
  local_nodes: ProxyNode[];
  /** configs[0] is the built-in default. */
  configs: Config[];
  active_config: string;
  group_selections: Record<string, Record<string, string>>;
  settings: ProxySettings;
}

export interface GroupNode {
  name: string;
  delay_ms: number | null;
  alive: boolean;
}

export interface GroupStatus {
  name: string;
  type: GroupType;
  selected: string;
  nodes: GroupNode[];
}

/** The engine's health watcher. */
export interface Health {
  online: boolean;
  node_ok: boolean;
  delay_ms?: number;
  /** Unix ms. */
  since: number;
  error?: string;
}

export interface ProxyStatus {
  enabled: boolean;
  state: ProxyState;
  error: string | null;
  routing: Routing;
  config: string;
  node: { id: string; name: string } | null;
  /** Unix seconds. */
  since: number | null;
  /** Bytes since the start. */
  up: number;
  down: number;
  mem: number;
  xray: string;
  engine: string;
  intercept_router: boolean;
  groups: GroupStatus[];
  health: Health | null;
}

export interface Conn {
  id: number;
  /** Unix ms. */
  start: number;
  /** Unix ms, 0 while open. */
  end: number;
  network: string;
  host: string;
  ip?: string;
  port: number;
  protocol?: string;
  inbound: string;
  /** proxy | direct | block | node:<name>. */
  outbound: string;
  group?: string;
  up: number;
  down: number;
  /** The LAN client (or the router itself). */
  source: string;
}

export interface Connections {
  epoch: string;
  active: Conn[];
  closed: Conn[];
}

export interface LogLine {
  seq: number;
  /** Unix ms. */
  time: number;
  /** ms since the current connection started. */
  run: number;
  level: LogLevel;
  text: string;
}

export interface LogPage {
  epoch: string;
  next: number;
  lines: LogLine[];
}

export interface SetArgs {
  enabled?: boolean;
  routing?: Routing;
  selected?: string;
  active_config?: string;
}

export interface SettingsArgs {
  intercept_router?: boolean;
  /** Comma list, may be empty. */
  bypass_macs?: string;
  probe_url?: string;
  subscription_refresh_hours?: number;
  rule_sets_url?: string;
  auto_failover?: boolean;
  log_level?: LogLevel;
}

export type ProbeMethod = "url" | "tcp" | "icmp";
