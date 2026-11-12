import { t } from "@/i18n";
import { z } from "zod";
import type { LanInfo, LanSetArgs } from "./types";

/** `rules.rs LAN_ADDRESS`: a private /24's .1. */
const OCTET = "(25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])";
export const LAN_ADDRESS = new RegExp(
  `^(10\\.${OCTET}\\.${OCTET}|172\\.(1[6-9]|2[0-9]|3[01])\\.${OCTET}|192\\.168\\.${OCTET})\\.1$`,
);

/** `lan.rs MASKS`: /16 to /30. */
export const NETMASKS = [
  ...["0", "128", "192", "224", "240", "248", "252", "254", "255"].map(
    (o) => `255.255.${o}.0`,
  ),
  ...["128", "192", "224", "240", "248", "252"].map((o) => `255.255.255.${o}`),
];

/** `lan.rs LEASETIME`. */
const LEASETIME = /^([1-9][0-9]{0,4}[mh]|infinite)$/;
const INT = /^(0|[1-9][0-9]*)$/;

export const LAN_FIELDS = [
  "address",
  "netmask",
  "dhcp",
  "dhcp_start",
  "dhcp_limit",
  "leasetime",
] as const;

export const lanSchema = () =>
  z
    .object({
      address: z.string().regex(LAN_ADDRESS, t("network.lan.schema.address")),
      netmask: z
        .string()
        .refine((m) => NETMASKS.includes(m), t("network.lan.schema.netmask")),
      dhcp: z.boolean(),
      dhcp_start: z
        .string()
        .refine(
          (v) => INT.test(v) && Number(v) >= 2 && Number(v) <= 254,
          t("network.lan.schema.dhcpStart"),
        ),
      dhcp_limit: z
        .string()
        .refine(
          (v) => INT.test(v) && Number(v) >= 1 && Number(v) <= 253,
          t("network.lan.schema.dhcpLimit"),
        ),
      leasetime: z.string().regex(LEASETIME, t("network.lan.schema.leasetime")),
    })
    .superRefine((v, ctx) => {
      if (
        INT.test(v.dhcp_start) &&
        INT.test(v.dhcp_limit) &&
        Number(v.dhcp_start) + Number(v.dhcp_limit) > 255
      ) {
        ctx.addIssue({
          code: "custom",
          path: ["dhcp_limit"],
          message: t("network.lan.schema.poolOverflow"),
        });
      }
    });

export type LanForm = z.infer<ReturnType<typeof lanSchema>>;

export function lanDefaults(info: LanInfo): LanForm {
  return {
    address: info.address,
    netmask: info.netmask,
    dhcp: info.dhcp.enabled,
    dhcp_start: String(info.dhcp.start ?? 100),
    dhcp_limit: String(info.dhcp.limit ?? 150),
    leasetime: info.dhcp.leasetime ?? "12h",
  };
}

/** Only what changed. */
export function lanArgs(v: LanForm, initial: LanForm): LanSetArgs {
  const a: LanSetArgs = {};
  if (v.address !== initial.address) a.address = v.address;
  if (v.netmask !== initial.netmask) a.netmask = v.netmask;
  if (v.dhcp !== initial.dhcp) a.dhcp = v.dhcp;
  if (v.dhcp_start !== initial.dhcp_start) a.dhcp_start = Number(v.dhcp_start);
  if (v.dhcp_limit !== initial.dhcp_limit) a.dhcp_limit = Number(v.dhcp_limit);
  if (v.leasetime !== initial.leasetime) a.leasetime = v.leasetime;
  return a;
}

const toInt = (ip: string) =>
  ip.split(".").reduce((n, o) => n * 256 + Number(o), 0);
const toIp = (n: number) =>
  [24, 16, 8, 0].map((s) => Math.floor(n / 2 ** s) % 256).join(".");

/** The DHCP pool as addresses (dnsmasq counts `start` from the network address), `null` when it cannot be worked out. */
export function dhcpRange(
  address: string,
  netmask: string,
  start: string,
  limit: string,
): string | null {
  if (
    !LAN_ADDRESS.test(address) ||
    !NETMASKS.includes(netmask) ||
    !INT.test(start) ||
    !INT.test(limit)
  )
    return null;
  const mask = toInt(netmask);
  const net = toInt(address) - (toInt(address) % (2 ** 32 - mask));
  const last = net + (2 ** 32 - mask) - 2;
  const first = net + Number(start);
  const end = Math.min(first + Number(limit) - 1, last);
  if (first > last) return null;
  return `${toIp(first)} – ${toIp(end)}`;
}

/** The console's address once the LAN moves to `address`. */
export function consoleUrlAt(address: string): string {
  const { protocol, port, pathname, search, hash } = window.location;
  return `${protocol}//${address}${port ? `:${port}` : ""}${pathname}${search}${hash}`;
}
