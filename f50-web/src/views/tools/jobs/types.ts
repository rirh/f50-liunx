export type { JobState, JobStatus } from "@/lib/jobs";

/** What started a job, by its `kind`. */
export const JOB_KIND_LABEL: Record<string, string> = {
  at: "AT 命令",
  sms: "短信",
  update: "系统更新",
  wifi: "Wi-Fi 客户端",
  radio: "射频",
};

/** What started a job, by its method. */
export const JOB_METHOD_LABEL: Record<string, string> = {
  "f50.at send": "发送 AT 命令",
  "f50.sms send": "发送短信",
  "f50.update apply": "安装更新",
  "f50.update apply_file": "安装上传的固件",
  "f50.update check": "检查更新",
  "f50.update rollback": "回滚",
  "f50.update upload_done": "校验上传的固件",
  "f50.wificlient join": "连接 Wi-Fi",
  "f50.wificlient leave": "断开 Wi-Fi",
  "f50.wificlient scan": "扫描 Wi-Fi",
};
