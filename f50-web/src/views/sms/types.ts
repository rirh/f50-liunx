/** `f50.sms` reply shapes (crates/f50-sms/src/pool/view.rs, lib.rs, modem.rs, sender.rs). */

export type SmsDir = "in" | "out";
export type SmsStatus = "unread" | "read" | "sent";
export type SmsCoding = "gsm" | "8bit" | "ucs2";

/** One message whole (`MessageView`). */
export interface SmsMessage {
  id: number;
  dir: SmsDir;
  status: SmsStatus;
  peer: string;
  /** The service centre's time (received) or the send time in UTC (sent), `YYYY-MM-DD HH:MM:SS`. */
  time: string;
  /** Minutes east of UTC of `time`, when known. */
  utc_offset: number | null;
  /** When the pool took it (Unix seconds). */
  pooled: number;
  coding: SmsCoding;
  parts: number;
  /** A long message pooled without all its parts. */
  incomplete: { have: number; total: number } | null;
  text: string;
}

export interface SmsConversation {
  peer: string;
  messages: number;
  unread: number;
  last_id: number;
  last_dir: SmsDir;
  last_status: SmsStatus;
  last_time: string;
  /** The zone of `last_time`, minutes east of UTC; null when the network did not send it. */
  last_utc_offset: number | null;
  /** The first line of the newest message, at most 44 characters. */
  preview: string;
}

/** The modem's message store as the last sync saw it. */
export interface SimStorage {
  store: string;
  used: number;
  total: number;
  /** Unix seconds. */
  at: number;
}

/**
 * The network's SMS paths as the last sync read them (crates/f50-sms/src/transport.rs); `null`: the modem did not say.
 * `ready: false`: no path at all, so nothing can be received or sent whatever the modem's SMS settings.
 */
export interface SmsTransport {
  /** Registered on IMS (SMS over IP). */
  ims: boolean | null;
  /** Registered in the CS domain or for SMS only (SMS over CS/SGs). */
  cs: boolean | null;
  /** The `+CREG` stat behind `cs` (8: emergency only). */
  cs_stat: number | null;
  /** 5G SA SMS over NAS allowed by the network. */
  nas: boolean | null;
  ready: boolean | null;
  /** Unix seconds. */
  at: number;
}

interface Paged {
  page: number;
  pages: number;
  per_page: number;
  sim_storage: SimStorage | null;
  /** Absent from an f50d older than the field. */
  transport?: SmsTransport | null;
}

/** `list` without `peer`. */
export interface ConversationPage extends Paged {
  total: number;
  unread: number;
  conversations_total: number;
  conversations: SmsConversation[];
}

/** `list {peer}`: newest first. */
export interface ThreadPage extends Paged {
  peer: string;
  total: number;
  unread: number;
  messages: SmsMessage[];
}

/** `list_unread`: unread incoming messages across all conversations. */
export interface UnreadPage extends Pick<Paged, "page" | "pages" | "per_page"> {
  unread: number;
  messages: SmsMessage[];
}

/** The send job's result. */
export interface SendResult {
  id: number;
  parts: number;
  references: number[];
}

export interface DeleteResult {
  deleted: number;
  sim_deleted: number;
}

export interface UnreadCounts {
  total: number;
  unread: number;
}

export type ListFilter = "all" | "unread";

/** `push`: the Telegram push settings; the token is never returned, only whether one is set and a hint. */
export interface PushSettings {
  enabled: boolean;
  token_set: boolean;
  /** `<bot id>:…<last four>` */
  token_hint: string | null;
  chat_id: string;
  api: string;
  default_api: string;
  /** Through the local proxy while it runs; absent from an older f50d. */
  proxy?: boolean;
  /** The proxy runs now (so `proxy` applies). */
  proxy_running?: boolean;
  queued: number;
  last: {
    at: number;
    ok: boolean;
    id?: number;
    error?: string;
  } | null;
}

/** `push_set` / `push_test`: an absent field keeps the saved value. */
export interface PushChange {
  enabled?: boolean;
  token?: string;
  chat_id?: string;
  api?: string;
  proxy?: boolean;
}
