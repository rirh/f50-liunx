/** f50d's AT terminal speaks English; these are the messages it answers with, in Chinese. */
import type { AtClass } from "./types";

export const CLASS_LABEL: Record<AtClass, { title: string; detail: string }> = {
  scan: {
    title: "网络扫描",
    detail: "耗时数分钟并一直占用控制通道，将作为后台任务运行（最长 3 分钟）。",
  },
  nv: {
    title: "写入 NV 设置",
    detail:
      "修改调制解调器保存在 NV 中的设置（网络模式、NR 模式、EN-DC、频段或小区锁定），重启后仍然生效。",
  },
  radio: {
    title: "射频 / 连接",
    detail:
      "影响射频、数据连接或调制解调器本身（CFUN、SFUN、CGATT、CGACT、COPS=n、ATD 等），可能导致断网。",
  },
  apn: {
    title: "数据配置",
    detail: "修改保存的数据配置（CGDCONT、CGAUTH），可能影响上网。",
  },
  sim: {
    title: "SIM 卡 PIN",
    detail: "涉及 SIM 卡 PIN（CLCK、CPWD、CPIN）；多次输错 PIN 会锁住 SIM 卡。",
  },
};

/** The class named in `this command needs confirmation: nv`. */
export function classOf(message: string): AtClass | null {
  const m = /confirmation:\s*([a-z]+)/i.exec(message);
  const c = m?.[1]?.toLowerCase();
  return c && c in CLASS_LABEL ? (c as AtClass) : null;
}

const KNOWN: [RegExp, string][] = [
  [
    /printable ASCII/i,
    "AT 命令须为一行以 AT 开头的可打印 ASCII 字符，最多 512 字节",
  ],
  [/chaining/i, "不允许用“;”串联多条命令"],
  [/wedges the AT port/i, "已拒绝：该命令会让 AT 端口卡死，直到重启"],
  [
    /moves the SIM selection/i,
    "已拒绝：该命令会切换 SIM 卡或数据路由，请在 SIM 页面操作",
  ],
  [
    /did not answer in time/i,
    "调制解调器未及时应答；命令可能仍在执行，请先确认再重发",
  ],
];

/** f50d's message in Chinese where it is one the terminal knows, else as it came. */
export function translate(message: string): string {
  const busy = /still running in the background \(([^)]+)\)/i.exec(message);
  if (busy)
    return `后台仍有长时间 AT 命令在运行（任务 ${busy[1]}），请等待它结束`;
  for (const [re, zh] of KNOWN) if (re.test(message)) return zh;
  return message;
}
