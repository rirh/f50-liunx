/** `f50.system logs` (f50-system/src/admin.rs): dmesg's and logread's last `lines` lines each, the newest 256 KiB. */
export interface LogsReply {
  /**
   * "--- dmesg (last N lines)\n…\n--- system log (last N lines)\n…\n", sanitised. Oldest first within each part, so the
   * newest system-log line is last.
   */
  text: string;
}

/** The line counts offered (f50d accepts 10..2000). */
export const LINE_OPTIONS = [100, 300, 500, 1000, 2000] as const;
export type LineCount = (typeof LINE_OPTIONS)[number];
