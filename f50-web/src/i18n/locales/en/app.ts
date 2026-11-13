import type { Messages } from "../zh";

const m: Messages["app"] = {
  brand: "ZTE F50",
  logout: "Sign out",
  language: "Language",
  notices: "Site notices",
  loading: "Loading",
  shell: {
    home: "Home",
    scrollLeft: "Scroll left",
    scrollRight: "Scroll right",
    closeTab: "Close {name}",
    account: "Account",
    logoutTitle: "Sign out?",
    logoutBody: "You will need the root password to sign in again.",
  },
  notFound: { title: "404 · Page not found", back: "Back to the dashboard" },
  login: {
    description:
      "Sign in with the device's system account (OpenWrt's root password)",
    username: "Username",
    password: "Password",
    submit: "Sign in",
    usernameRequired: "Enter a username",
    wrong: "Wrong username or password",
  },
  theme: { toLight: "Switch to light mode", toDark: "Switch to dark mode" },
  build: { label: "Build information", builtAt: "Built", unknown: "unknown" },
  pageUpdate: {
    title: "A new version of this page",
    description:
      "Reload to use it. Save anything you have not submitted first.",
    action: "Reload",
    running: "Reloading…",
    close: "Dismiss the update notice",
    failed: "Update failed, please try again.",
  },
  routeProgress: "Changing page",
  query: {
    unsupported: "Not supported on this device",
    failed: "Could not read",
    retry: "Retry",
  },
  job: {
    state: {
      running: "Running",
      done: "Done",
      failed: "Failed",
      cancelled: "Cancelled",
      interrupted: "Interrupted",
    },
    stopRequested: "Stop requested",
  },
  live: {
    live: "Live",
    connecting: "Connecting",
    polling: "Polling",
    hint: {
      live: "The device pushes figures as they change (f50.live)",
      connecting: "Opening the live stream; pages refresh on a timer meanwhile",
      retrying:
        "The live stream dropped and is reconnecting; pages refresh on a timer meanwhile",
      denied:
        "This login has no live-stream access (it predates the device update). Sign out and in again to enable it",
    },
    noStream: "No data stream",
    network: "Network error",
    broken: "Connection lost",
  },
  error: {
    code: {
      "bad-request": "Invalid request",
      "unknown-method": "The device does not know this operation",
      "unknown-field": "The request has an unexpected field",
      "bad-value": "Invalid value",
      missing: "A required field is missing",
      forbidden: "Not allowed",
      unsupported: "Not supported by the device",
      busy: "The device is busy",
      timeout: "Timed out",
      failed: "Operation failed",
      "not-found": "Not found",
      unauthorized: "Your session expired",
      transport: "Cannot reach the device",
    },
    ubus: {
      badRequest: "Invalid arguments",
      noMethod: "No such method",
      noObject: "The device has no such service (is f50d running?)",
      noData: "No data returned",
      denied: "Not permitted, or your session expired",
      timeout: "The device did not answer in time",
      unsupported: "Not supported",
      noConnection: "Cannot reach the service on the device",
      other: "ubus error {status}",
    },
    sessionExpired: "Your session expired, please sign in again",
    missingMethod:
      "The device has no {method}: f50d is not installed or not running",
    unreadable: "The device returned data that could not be understood",
    failed: "Operation failed",
    unreachable: "Cannot reach the device",
    http: "Request failed (HTTP {status})",
    upload: "Upload failed (HTTP {status})",
    withDetail: "{label}: {detail}",
  },
  format: {
    duration: {
      dh: "{d} d {h} h",
      hm: "{h} h {m} min",
      ms: "{m} min {s} s",
      s: "{s} s",
    },
  },
};
export default m;
