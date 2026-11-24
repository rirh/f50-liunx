import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type {
  RemoteSettings,
  ConversationPage,
  DeleteResult,
  PushChange,
  PushSettings,
  SmsMessage,
  ThreadPage,
  UnreadCounts,
  UnreadPage,
} from "./types";

const OBJ = "f50.sms";

export const smsKeys = {
  all: ["sms"] as const,
  conversations: ["sms", "conversations"] as const,
  recipients: ["sms", "recipients"] as const,
  thread: (peer: string) => ["sms", "thread", peer] as const,
  unread: ["sms", "unread"] as const,
  push: ["sms", "push"] as const,
};

/** Conversations newest first (one page). */
export const listConversations = (page: number, perPage: number) =>
  f50<ConversationPage>(OBJ, "list", { page, per_page: perPage });

export const listRecipients = (signal?: AbortSignal) =>
  f50<{ recipients: string[] }>(OBJ, "recipients", {}, { signal });

/** One conversation's messages, newest first (one page). Nothing is marked read. */
export const listThread = (peer: string, page: number, perPage: number) =>
  f50<ThreadPage>(OBJ, "list", { peer, page, per_page: perPage });

/** Incoming unread messages across all conversations, newest first. */
export const listUnread = (
  page: number,
  perPage: number,
  signal?: AbortSignal,
) =>
  f50<UnreadPage>(OBJ, "list_unread", { page, per_page: perPage }, { signal });

/** One message whole; a received one is marked read. */
export const readMessage = (id: number) =>
  f50<SmsMessage>(OBJ, "read", { id: String(id) });

/** Starts a send job. */
export const sendSms = (number: string, text: string) =>
  f50<JobStarted>(OBJ, "send", { number, text });

/** One message; with `sim`, its copy in the modem's store too. The console never deletes all at once. */
export const deleteSms = (id: number, sim: boolean) =>
  f50<DeleteResult>(OBJ, "delete", { id: String(id), sim });

/** Starts a sync job; its result confirms that receiving setup and the store sync completed. */
export const syncSms = () => f50<JobStarted>(OBJ, "sync");

export const unreadCounts = () => f50<UnreadCounts>(OBJ, "unread");

/** The Telegram push settings (never the token). */
export const getPush = () => f50<PushSettings>(OBJ, "push");

export const setPush = (change: PushChange) =>
  f50<PushSettings>(OBJ, "push_set", { ...change });

/** A test message with the saved settings changed by `change` (not saved). */
export const testPush = (change: PushChange) =>
  f50<{ sent: boolean }>(OBJ, "push_test", { ...change });

/** 短信遥控 (`f50.remote`): its own object, beside the SMS pool's. */
export const getRemote = () => f50<RemoteSettings>("f50.remote", "get");
/** `numbers`: comma separated; an absent `secret` keeps it, "" clears it. */
export const setRemote = (change: {
  enabled?: boolean;
  numbers?: string;
  secret?: string;
}) => f50<RemoteSettings>("f50.remote", "set", change);
