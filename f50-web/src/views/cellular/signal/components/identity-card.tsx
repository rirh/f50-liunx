import { KvList } from "@/components/f50/kv-list";
import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { errorMessage, formatAgo, formatTime } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { EyeIcon, EyeOffIcon } from "lucide-react";
import { toast } from "sonner";
import { getIdentity } from "../api";
import type { ModemIdentity } from "../types";

const MSISDN_SRC = { sim: "SIM 卡", ims: "IMS 身份" } as const;

function IdentityList({ d }: { d: ModemIdentity }) {
  const id = d.ident;
  return (
    <KvList
      items={[
        {
          label: "IMEI",
          value: <span className="font-mono">{id?.imei ?? "—"}</span>,
        },
        {
          label: "IMSI",
          value: <span className="font-mono">{id?.imsi ?? "—"}</span>,
        },
        {
          label: "ICCID",
          value: <span className="font-mono">{id?.iccid ?? "—"}</span>,
        },
        {
          label: "本机号码",
          value: d.msisdn ? (
            <span className="font-mono">
              {d.msisdn}
              {d.msisdn_src && (
                <span className="text-muted-foreground">
                  {" "}
                  （来自{MSISDN_SRC[d.msisdn_src]}）
                </span>
              )}
            </span>
          ) : (
            "未读到"
          ),
        },
        {
          label: "厂商 / 型号",
          value: [id?.mfg, id?.model].filter(Boolean).join(" ") || "—",
        },
        { label: "固件", value: id?.fw ?? "—" },
        { label: "SIM 卡槽", value: `SIM${d.slot}` },
        {
          label: "读取时间",
          value: id?.ts ? formatAgo(id.ts) : "—",
          title: id?.ts ? formatTime(id.ts) : undefined,
        },
      ]}
    />
  );
}

/** IMEI, IMSI, ICCID and the own number: private, fetched only when asked for (a write-access method). */
export function IdentityCard() {
  const reveal = useMutation({
    mutationFn: getIdentity,
    onError: (e) => toast.error(errorMessage(e)),
  });
  const shown = reveal.isSuccess;
  return (
    <Card>
      <CardHeader>
        <CardTitle>设备与 SIM 标识</CardTitle>
        <CardDescription>
          IMEI、IMSI、ICCID 与本机号码属于隐私信息，按需显示
        </CardDescription>
        <CardAction>
          {shown ? (
            <Button variant="outline" size="sm" onClick={() => reveal.reset()}>
              <EyeOffIcon data-icon="inline-start" />
              隐藏
            </Button>
          ) : (
            <PendingButton
              variant="outline"
              size="sm"
              pending={reveal.isPending}
              icon={<EyeIcon data-icon="inline-start" />}
              onClick={() => reveal.mutate()}
            >
              显示
            </PendingButton>
          )}
        </CardAction>
      </CardHeader>
      <CardContent>
        {reveal.isPending ? (
          <div className="flex flex-col gap-2">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-5 w-full" />
            ))}
          </div>
        ) : shown ? (
          <IdentityList d={reveal.data} />
        ) : (
          <p className="text-sm text-muted-foreground">
            已隐藏。点击「显示」读取。
          </p>
        )}
      </CardContent>
    </Card>
  );
}
