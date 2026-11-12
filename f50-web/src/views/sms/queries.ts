import { useEffect, useMemo, useRef } from "react";
import { usePoll } from "@/hooks/use-live";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { toast } from "sonner";
import {
  deleteSms,
  listConversations,
  listThread,
  readMessage,
  smsKeys,
  syncSms,
  unreadCounts,
} from "./api";
import type { SmsMessage } from "./types";

const POLL_MS = 15_000;
const LIST_PAGE = 50;
const THREAD_PAGE = 50;

export function useConversations() {
  return useInfiniteQuery({
    queryKey: smsKeys.conversations,
    queryFn: ({ pageParam }) => listConversations(pageParam, LIST_PAGE),
    initialPageParam: 1,
    getNextPageParam: (last) =>
      last.page < last.pages ? last.page + 1 : undefined,
    refetchInterval: POLL_MS,
  });
}

export function useUnread() {
  return useQuery({
    queryKey: smsKeys.unread,
    queryFn: unreadCounts,
    refetchInterval: usePoll(POLL_MS, "sms"),
  });
}

/** A conversation's messages, oldest first for display; incoming unread ones are marked read once shown. */
export function useThread(peer: string) {
  const qc = useQueryClient();
  const query = useInfiniteQuery({
    queryKey: smsKeys.thread(peer),
    queryFn: ({ pageParam }) => listThread(peer, pageParam, THREAD_PAGE),
    initialPageParam: 1,
    getNextPageParam: (last) =>
      last.page < last.pages ? last.page + 1 : undefined,
    refetchInterval: POLL_MS,
  });
  const messages = useMemo<SmsMessage[]>(() => {
    const seen = new Set<number>();
    const all: SmsMessage[] = [];
    for (const p of query.data?.pages ?? []) {
      for (const m of p.messages) {
        if (seen.has(m.id)) continue;
        seen.add(m.id);
        all.push(m);
      }
    }
    return all.sort((a, b) => a.id - b.id);
  }, [query.data]);

  // `read {id}` is the only way to mark a message read: once per id while the thread is open
  const marked = useRef(new Set<number>());
  useEffect(() => {
    const unread = messages.filter(
      (m) =>
        m.dir === "in" && m.status === "unread" && !marked.current.has(m.id),
    );
    if (unread.length === 0) return;
    for (const m of unread) marked.current.add(m.id);
    void (async () => {
      for (const m of unread) await readMessage(m.id).catch(() => undefined);
      await Promise.all([
        qc.invalidateQueries({ queryKey: smsKeys.thread(peer) }),
        qc.invalidateQueries({ queryKey: smsKeys.conversations }),
        qc.invalidateQueries({ queryKey: smsKeys.unread }),
      ]);
    })();
  }, [messages, peer, qc]);

  return { ...query, messages };
}

function useInvalidateSms() {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: smsKeys.all });
}

export function useSyncSms() {
  const invalidate = useInvalidateSms();
  return useMutation({
    mutationFn: syncSms,
    onSuccess: () => {
      toast.success(t("sms.toast.syncStarted"));
      // the sync runs in the background: look again shortly
      window.setTimeout(() => void invalidate(), 3000);
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
}

export function useDeleteSms() {
  const invalidate = useInvalidateSms();
  return useMutation({
    mutationFn: ({ id, sim }: { id: number | "all"; sim: boolean }) =>
      deleteSms(id, sim),
    onSuccess: (r) =>
      toast.success(
        r.sim_deleted > 0
          ? t("sms.toast.deletedSim", { count: r.deleted, sim: r.sim_deleted })
          : t("sms.toast.deleted", { count: r.deleted }),
      ),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => invalidate(),
  });
}

/**
 * Deletes a whole conversation. f50d deletes by id or all, so this reads every message id of the peer and deletes
 * them one by one.
 */
export function useDeleteConversation() {
  const invalidate = useInvalidateSms();
  return useMutation({
    mutationFn: async ({ peer, sim }: { peer: string; sim: boolean }) => {
      const ids: number[] = [];
      for (let page = 1, pages = 1; page <= pages; page++) {
        const r = await listThread(peer, page, 100);
        pages = r.pages;
        ids.push(...r.messages.map((m) => m.id));
      }
      let deleted = 0;
      let simDeleted = 0;
      for (const id of ids) {
        const r = await deleteSms(id, sim);
        deleted += r.deleted;
        simDeleted += r.sim_deleted;
      }
      return { deleted, simDeleted };
    },
    onSuccess: (r) =>
      toast.success(
        r.simDeleted > 0
          ? t("sms.toast.deletedSim", { count: r.deleted, sim: r.simDeleted })
          : t("sms.toast.deleted", { count: r.deleted }),
      ),
    onError: (e) => toast.error(errorMessage(e)),
    onSettled: () => invalidate(),
  });
}
