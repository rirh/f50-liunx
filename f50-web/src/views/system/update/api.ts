import type { JobStarted } from "@/lib/jobs";
import { uploadRequest } from "@/lib/request";
import { f50 } from "@/lib/ubus";
import type {
  CgiUploadReply,
  Releases,
  RollbackWhat,
  UpdateStatus,
} from "./types";

export const UPDATE_KEY = ["f50.update"] as const;
export const STATUS_KEY = [...UPDATE_KEY, "status"] as const;
export const RELEASES_KEY = [...UPDATE_KEY, "releases"] as const;

export const getStatus = () => f50<UpdateStatus>("f50.update", "status");
export const getReleases = () => f50<Releases>("f50.update", "releases");

export const check = () => f50<JobStarted>("f50.update", "check");

/** Installs a release the last check offered; not cancellable. */
export const apply = (tag?: string) =>
  f50<JobStarted>("f50.update", "apply", {
    confirm: "apply",
    ...(tag ? { tag } : {}),
  });

export const rollback = (what: RollbackWhat) =>
  f50<JobStarted>("f50.update", "rollback", { confirm: "rollback", what });

/** Makes room for the upload; the path cgi-upload must write it to. */
export const uploadBegin = (size: number, sha256: string) =>
  f50<{ path: string }>("f50.update", "upload_begin", { size, sha256 });

/** The file itself, through uhttpd's cgi-io (the field order matters: `filedata` last). */
export function uploadFile(sessionid: string, path: string, file: File) {
  const form = new FormData();
  form.append("sessionid", sessionid);
  form.append("filename", path);
  form.append("filedata", file, file.name);
  return uploadRequest<CgiUploadReply>("/cgi-bin/cgi-upload", form);
}

export const uploadDone = () => f50<JobStarted>("f50.update", "upload_done");
export const uploadCancel = () =>
  f50<Record<string, never>>("f50.update", "upload_cancel");

export const applyFile = (
  sha256: string,
  opts: { unsigned: boolean; downgrade: boolean },
) =>
  f50<JobStarted>("f50.update", "apply_file", {
    sha256,
    confirm: "apply",
    ...(opts.unsigned ? { unsigned: true } : {}),
    ...(opts.downgrade ? { downgrade: true } : {}),
  });
