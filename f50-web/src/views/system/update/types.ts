/** `f50.update` (f50d/crates/f50-update/src/{object,status,inspect,notice}.rs, release/index.rs). */

export type SigState =
  "valid" | "bad" | "unsigned" | "nokey" | "badkey" | "notool";

export interface SystemCopy {
  name: string;
  release: string | null;
  running: boolean;
  /** The copy kept for a rollback. */
  previous: { release: string | null; running: boolean } | null;
}

export interface Notice {
  tag: string;
  installed: string | null;
  at: number;
}

/** What `upload_done` found the file to be (kept for `apply_file`). */
export interface Inspected {
  sha256: string;
  size: number;
  tag: string | null;
  signature: SigState;
  whole: boolean;
  fits: boolean;
  systems: string[];
  kernel: boolean;
  downgrade: boolean;
  refused: string | null;
}

export interface UpdateStatus {
  installed: string | null;
  restart_pending: boolean;
  systems: SystemCopy[];
  kernel: { running: string | null; choice: string; choices: string[] };
  boot: {
    release: string | null;
    kernel: string | null;
    slot: string;
    previous: {
      release: string | null;
      kernel: string | null;
      usable: boolean;
    } | null;
    default: "linux" | "android";
  };
  disk: {
    free_kb: number | null;
    total_kb: number | null;
    need_one_kb: number;
    need_all_kb: number;
  };
  lock: { pid: number; command: string | null; this_daemon: boolean } | null;
  /** The id of the running job of kind `update`. */
  job: string | null;
  unfinished: boolean;
  recovery: { at: number; lines: string[] } | null;
  notice: Notice | null;
  signing: { key: "none" | "unsafe" | "ok" };
  source: {
    api: string;
    download: string;
    mirror: boolean;
    fallback: "auto" | "never" | "always";
    fell_back: boolean;
  };
  report_new: boolean;
  upload: { present: boolean; inspect: Inspected | null };
}

export interface Asset {
  name: string;
  size: number;
}

export interface Offer {
  tag: string;
  title: string | null;
  published: string | null;
  prerelease: boolean;
  signed: boolean;
  /** The systems that are not that release yet. */
  systems: string[];
  files: Asset[];
  download: number;
}

export interface Checked {
  checked: number;
  latest: Offer;
  beta: Offer | null;
}

/** `releases`: the last check's answer and its age in seconds (both null before any check). */
export interface Releases {
  age: number | null;
  check: Checked | null;
}

/** uhttpd cgi-io's reply to an upload. */
export interface CgiUploadReply {
  size?: number;
  checksum?: string;
}

export type RollbackWhat = "system" | "boot";

/** The job the page follows, and what it is called. */
export interface ActiveJob {
  id: string;
  title: string;
}
