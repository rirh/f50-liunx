/** The mainland carrier an operator string names (for its mark on the dashboard). */

export type Carrier = "cmcc" | "cucc" | "ctcc" | "cbn";

/** The four mainland carriers' PLMNs (MCC 460): the modem may report the numeric code instead of a name. */
const PLMN: Record<string, Carrier> = {
  "46000": "cmcc",
  "46002": "cmcc",
  "46004": "cmcc",
  "46007": "cmcc",
  "46008": "cmcc",
  "46013": "cmcc",
  "46001": "cucc",
  "46006": "cucc",
  "46009": "cucc",
  "46010": "cucc",
  "46003": "ctcc",
  "46005": "ctcc",
  "46011": "ctcc",
  "46012": "ctcc",
  "46015": "cbn",
};

/** Which carrier an operator string names: a Chinese or English long name, a short name, or a PLMN. */
export function carrierOf(operator: string | null | undefined): Carrier | null {
  if (!operator) return null;
  const s = operator.trim();
  const plmn = PLMN[s.replace(/\D/g, "").slice(0, 5)];
  if (/^\d{5,6}$/.test(s)) return plmn ?? null;
  const u = s.toUpperCase();
  if (/移动|MOBILE|CMCC|CMNET/.test(u)) return "cmcc";
  if (/联通|UNICOM|CUCC|CHN-CU/.test(u)) return "cucc";
  if (/电信|TELECOM|CTCC|CHN-CT/.test(u)) return "ctcc";
  if (/广电|BROADNET|CBN/.test(u)) return "cbn";
  return null;
}

/** The carrier's name as people say it. */
export const CARRIER_NAME: Record<Carrier, string> = {
  cmcc: "中国移动",
  cucc: "中国联通",
  ctcc: "中国电信",
  cbn: "中国广电",
};
