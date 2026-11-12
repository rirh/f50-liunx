import { t } from "@/i18n";

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "/ubus";

/** The one HTTP transport of the app: native fetch to the device, JSON in and out. */
export async function apiRequest<T>(
  input: string,
  init?: RequestInit,
): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${input}`, {
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    ...init,
  });

  if (!response.ok) {
    throw new Error(t("app.error.http", { status: response.status }));
  }

  return response.json() as Promise<T>;
}

/** A multipart upload (uhttpd's cgi-upload); the path is outside the JSON-RPC endpoint. */
export async function uploadRequest<T>(
  url: string,
  body: FormData,
): Promise<T> {
  const response = await fetch(url, { method: "POST", body });
  if (!response.ok) {
    throw new Error(t("app.error.upload", { status: response.status }));
  }
  return response.json() as Promise<T>;
}
