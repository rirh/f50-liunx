import type { Rat, SimState } from "./types";

export const RAT_LABEL: Record<Rat, string> = {
  gsm: "2G",
  umts: "3G",
  lte: "4G LTE",
  nsa: "5G NSA",
  sa: "5G SA",
};

export const SIM_STATE_LABEL: Record<SimState, string> = {
  ready: "就绪",
  absent: "未插卡",
  locked: "需要 PIN/PUK",
};

/** 27.007 registration `<stat>`. */
export const REG_STAT_LABEL: Record<number, string> = {
  0: "未注册",
  1: "已注册",
  2: "搜索中",
  3: "被拒绝",
  4: "未知",
  5: "已注册（漫游）",
  10: "已注册（仅紧急）",
};

export const CFUN_LABEL: Record<number, string> = {
  0: "关闭",
  1: "开启",
  2: "关闭",
  4: "飞行模式",
};
