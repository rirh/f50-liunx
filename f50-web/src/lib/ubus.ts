/**
 * f50d over uhttpd's `/ubus` JSON-RPC endpoint.
 *
 * Every call carries the rpcd session from the login. f50d answers inside the call's data with its own envelope:
 * `{ ok: true, data }` or `{ ok: false, code, error, field }`; `f50()` unwraps it and throws an `ApiError` for a
 * refusal. A rejected session (expired, logged out elsewhere) clears the stored one and returns to the login page.
 */
import { apiRequest } from "@/lib/request";

export const ANONYMOUS_SESSION = "00000000000000000000000000000000";
const SESSION_KEY = "f50-session";

/** The codes f50d answers with (`f50_core::Code`), plus the transport's own. */
export type ApiErrorCode =
  | "bad-request"
  | "unknown-method"
  | "unknown-field"
  | "bad-value"
  | "missing"
  | "forbidden"
  | "unsupported"
  | "busy"
  | "timeout"
  | "failed"
  | "not-found"
  | "unauthorized"
  | "transport";

export class ApiError extends Error {
  readonly code: ApiErrorCode;
  readonly field?: string;

  constructor(code: ApiErrorCode, message: string, field?: string) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.field = field;
  }
}

export interface Session {
  id: string;
  username: string;
  /** Unix seconds when rpcd forgets an idle session. */
  expires: number;
}

export function readSession(): Session | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    return raw ? (JSON.parse(raw) as Session) : null;
  } catch {
    return null;
  }
}

export function writeSession(session: Session | null) {
  if (session) localStorage.setItem(SESSION_KEY, JSON.stringify(session));
  else localStorage.removeItem(SESSION_KEY);
  window.dispatchEvent(new Event("f50-session"));
}

interface RpcResponse {
  result?: [number, unknown?];
  error?: { code: number; message: string };
}

// ubus status codes (UBUS_STATUS_*)
const UBUS_STATUS: Record<number, [ApiErrorCode, string]> = {
  2: ["bad-request", "参数无效"],
  3: ["unknown-method", "没有这个方法"],
  4: ["not-found", "设备上没有这个服务（f50d 未运行？）"],
  5: ["failed", "没有返回数据"],
  6: ["unauthorized", "没有权限，或登录已过期"],
  7: ["timeout", "设备响应超时"],
  8: ["unsupported", "不支持"],
  10: ["transport", "无法连接设备上的服务"],
};

let rpcId = 0;

function sessionExpired() {
  writeSession(null);
  if (!window.location.hash.startsWith("#/login"))
    window.location.hash = "#/login";
}

/**
 * One raw ubus call: the call's data, or an `ApiError` for a transport or ubus failure. A `probe` (is the device up?)
 * never ends the login when it is refused.
 */
export async function ubusCall<T = unknown>(
  object: string,
  method: string,
  args: Record<string, unknown> = {},
  session: string = readSession()?.id ?? ANONYMOUS_SESSION,
  { probe = false }: { probe?: boolean } = {},
): Promise<T> {
  let response: RpcResponse;
  try {
    response = await apiRequest<RpcResponse>("", {
      method: "POST",
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: ++rpcId,
        method: "call",
        params: [session, object, method, args],
      }),
    });
  } catch (e) {
    throw new ApiError(
      "transport",
      e instanceof Error ? e.message : "无法连接设备",
    );
  }
  if (response.error) {
    // -32002: the session is unknown or has no access
    if (response.error.code === -32002) {
      if (!probe) sessionExpired();
      throw new ApiError("unauthorized", "登录已过期，请重新登录");
    }
    throw new ApiError("transport", response.error.message);
  }
  const [status, data] = response.result ?? [9];
  if (status !== 0) {
    const [code, message] =
      UBUS_STATUS[status] ??
      (["failed", `ubus 错误 ${status}`] as [ApiErrorCode, string]);
    if (code === "unauthorized" && !probe) sessionExpired();
    throw new ApiError(code, message);
  }
  return data as T;
}

interface Envelope<T> {
  ok: boolean;
  data?: T;
  code?: ApiErrorCode;
  error?: string;
  field?: string;
}

/** One f50d method: its `data`, or an `ApiError` with f50d's code, message and field. */
export async function f50<T = unknown>(
  object: string,
  method: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const env = await ubusCall<Envelope<T>>(object, method, args);
  if (!env || typeof env !== "object" || !("ok" in env)) {
    throw new ApiError("failed", "设备返回了无法识别的数据");
  }
  if (!env.ok) {
    throw new ApiError(
      env.code ?? "failed",
      env.error ?? "操作失败",
      env.field,
    );
  }
  return env.data as T;
}

/** rpcd login: a new session for this user. */
export async function login(
  username: string,
  password: string,
): Promise<Session> {
  const data = await ubusCall<{
    ubus_rpc_session: string;
    expires: number;
    data?: { username?: string };
  }>("session", "login", { username, password }, ANONYMOUS_SESSION).catch(
    (e: unknown) => {
      if (
        e instanceof ApiError &&
        (e.code === "unauthorized" || e.code === "transport")
      ) {
        throw new ApiError("unauthorized", "用户名或密码错误");
      }
      throw e;
    },
  );
  const session = {
    id: data.ubus_rpc_session,
    username: data.data?.username ?? username,
    expires: Math.floor(Date.now() / 1000) + (data.expires ?? 300),
  };
  writeSession(session);
  return session;
}

export async function logout() {
  const s = readSession();
  if (s) await ubusCall("session", "destroy", {}, s.id).catch(() => undefined);
  writeSession(null);
}

/** Whether an error is f50d saying the device lacks the feature (the page then says so instead of an error). */
export function isUnsupported(e: unknown) {
  return e instanceof ApiError && e.code === "unsupported";
}
