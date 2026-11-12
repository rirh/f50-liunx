import type { JobStarted } from "@/lib/jobs";
import { f50 } from "@/lib/ubus";
import type {
  ConversationPage,
  DeleteResult,
  SmsMessage,
  ThreadPage,
  UnreadCounts,
} from "./types";

const OBJ = "f50.sms";

export const smsKeys = {
  all: ["sms"] as const,
  conversations: ["sms", "conversations"] as const,
  thread: (peer: string) => ["sms", "thread", peer] as const,
  unread: ["sms", "unread"] as const,
};

/** Conversations newest first (one page). */
export const listConversations = (page: number, perPage: number) =>
  f50<ConversationPage>(OBJ, "list", { page, per_page: perPage });

/** One conversation's messages, newest first (one page). Nothing is marked read. */
export const listThread = (peer: string, page: number, perPage: number) =>
  f50<ThreadPage>(OBJ, "list", { peer, page, per_page: perPage });

/** One message whole; a received one is marked read. */
export const readMessage = (id: number) =>
  f50<SmsMessage>(OBJ, "read", { id: String(id) });

/** Starts a send job. */
export const sendSms = (number: string, text: string) =>
  f50<JobStarted>(OBJ, "send", { number, text });

/** One message, or `"all"`; with `sim`, the matching ones in the modem's store too. */
export const deleteSms = (id: number | "all", sim: boolean) =>
  f50<DeleteResult>(OBJ, "delete", { id: String(id), sim });

/** A sync of the modem's store, in the background. */
export const syncSms = () => f50<{ started: boolean }>(OBJ, "sync");

export const unreadCounts = () => f50<UnreadCounts>(OBJ, "unread");
