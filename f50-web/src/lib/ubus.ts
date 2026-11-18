/**
 * f50d over uhttpd's `/ubus` JSON-RPC endpoint.
 *
 * Every call carries the rpcd session from the login. f50d answers inside the call's data with its own envelope:
 * `{ ok: true, data }` or `{ ok: false, code, error, field }`; `f50()` unwraps it and throws an `ApiError` for a
 * refusal. A rejected session (expired, logged out elsewhere) clears the stored one and returns to the login page.
 */
import { type Key, t } from "@/i18n";
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
  /** The message is f50d's own (English) text, shown after the name of the code; otherwise it is the console's. */
  readonly fromDevice: boolean;

  constructor(
    code: ApiErrorCode,
    message: string,
    field?: string,
    fromDevice = false,
  ) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.field = field;
    this.fromDevice = fromDevice;
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

// ubus status codes (UBUS_STATUS_*): the code, and the message's key (read when the error is made)
const UBUS_STATUS: Record<number, [ApiErrorCode, Key]> = {
  2: ["bad-request", "app.error.ubus.badRequest"],
  3: ["unknown-method", "app.error.ubus.noMethod"],
  4: ["not-found", "app.error.ubus.noObject"],
  5: ["failed", "app.error.ubus.noData"],
  6: ["unauthorized", "app.error.ubus.denied"],
  7: ["timeout", "app.error.ubus.timeout"],
  8: ["unsupported", "app.error.ubus.unsupported"],
  10: ["transport", "app.error.ubus.noConnection"],
};

let rpcId = 0;

function sessionExpired(session: string) {
  if (readSession()?.id !== session) return;
  writeSession(null);
  if (window.location.pathname === "/login") return;
  // outside React: the router follows a history entry it is told about with popstate
  window.history.pushState(null, "", "/login");
  window.dispatchEvent(new PopStateEvent("popstate"));
}

/** A read method every login is granted, in every f50d: answers while the session is alive. */
const ALIVE_CHECK = ["f50.sms", "unread"] as const;

async function sessionAlive(
  session: string,
  signal?: AbortSignal,
): Promise<boolean> {
  if (session === ANONYMOUS_SESSION) return false;
  try {
    await ubusCall(ALIVE_CHECK[0], ALIVE_CHECK[1], {}, session, {
      probe: true,
      signal,
    });
    return true;
  } catch (e) {
    if (signal && e instanceof ApiError && e.code === "transport") throw e;
    // only a refused session is dead; a check that failed some other way (a timeout) says nothing
    return !(e instanceof ApiError && e.code === "unauthorized");
  }
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
  { probe = false, signal }: { probe?: boolean; signal?: AbortSignal } = {},
): Promise<T> {
  let response: RpcResponse;
  try {
    response = await apiRequest<RpcResponse>("", {
      method: "POST",
      signal,
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
      e instanceof Error ? e.message : t("app.error.unreachable"),
    );
  }
  if (response.error) {
    // -32002: the session is unknown or has no access
    if (response.error.code === -32002) {
      // rpcd says the same for an expired session and for a method this session was never granted (a daemon older
      // than the console, or a session from before an update added the method: rpcd fixes a session's ACL at
      // login). Only an expired session ends the login.
      if (!probe && (await sessionAlive(session, signal))) {
        throw new ApiError(
          "forbidden",
          t("app.error.noAccess", { method: `${object}.${method}` }),
        );
      }
      if (!probe) sessionExpired(session);
      throw new ApiError("unauthorized", t("app.error.sessionExpired"));
    }
    // -32000 / -32601: uhttpd found no such object or method on the bus: the device answers, f50d (or this method
    // of it) is not there - an older image, or the daemon not running
    if (response.error.code === -32000 || response.error.code === -32601) {
      throw new ApiError(
        "not-found",
        t("app.error.missingMethod", { method: `${object}.${method}` }),
      );
    }
    throw new ApiError("transport", response.error.message);
  }
  const [status, data] = response.result ?? [9];
  if (status !== 0) {
    const known = UBUS_STATUS[status];
    const [code, message]: [ApiErrorCode, string] = known
      ? [known[0], t(known[1])]
      : ["failed", t("app.error.ubus.other", { status })];
    if (code === "unauthorized" && !probe) sessionExpired(session);
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
  { session, signal }: { session?: string; signal?: AbortSignal } = {},
): Promise<T> {
  const env = await ubusCall<Envelope<T>>(object, method, args, session, {
    signal,
  });
  if (!env || typeof env !== "object" || !("ok" in env)) {
    throw new ApiError("failed", t("app.error.unreadable"));
  }
  if (!env.ok) {
    throw new ApiError(
      env.code ?? "failed",
      env.error ?? t("app.error.failed"),
      env.field,
      !!env.error,
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
        throw new ApiError("unauthorized", t("app.login.wrong"));
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
