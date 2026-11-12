import { useSyncExternalStore } from "react";
import { type Session, readSession } from "@/lib/ubus";

function subscribe(onChange: () => void) {
  window.addEventListener("f50-session", onChange);
  window.addEventListener("storage", onChange);
  return () => {
    window.removeEventListener("f50-session", onChange);
    window.removeEventListener("storage", onChange);
  };
}

let cached: { raw: string | null; session: Session | null } = { raw: null, session: null };

function snapshot(): Session | null {
  const raw = localStorage.getItem("f50-session");
  if (raw !== cached.raw) cached = { raw, session: readSession() };
  return cached.session;
}

/** The rpcd login of this browser, kept in step with other tabs and with an expiry seen by any call. */
export function useSession() {
  return useSyncExternalStore(subscribe, snapshot, () => null);
}
