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

interface Paged {
  page: number;
  pages: number;
  per_page: number;
  sim_storage: SimStorage | null;
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
