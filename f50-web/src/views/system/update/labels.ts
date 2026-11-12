import type { SigState } from "./types";

/** A job's title by its method (`f50.job list` gives the method of a job found running). */
export const JOB_TITLE: Record<string, string> = {
  "f50.update check": "检查更新",
  "f50.update apply": "安装更新",
  "f50.update rollback": "回滚",
  "f50.update upload_done": "检查上传的文件",
  "f50.update apply_file": "安装离线更新包",
};

export const SIG_LABEL: Record<SigState, string> = {
  valid: "签名有效",
  bad: "签名错误",
  unsigned: "未签名",
  nokey: "本机没有发布密钥，无法校验",
  badkey: "本机发布密钥不安全",
  notool: "本机无法校验此签名格式",
};
