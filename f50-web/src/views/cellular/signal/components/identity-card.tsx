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
import { t, type Key } from "@/i18n";
import { errorMessage, formatAgo, formatTime } from "@/lib/format";
import { useMutation } from "@tanstack/react-query";
import { EyeIcon, EyeOffIcon } from "lucide-react";
import { toast } from "sonner";
import { getIdentity } from "../api";
import type { ModemIdentity } from "../types";

const MSISDN_SRC = {
  sim: "cellular.signal.identity.msisdnSrc.sim",
  ims: "cellular.signal.identity.msisdnSrc.ims",
} as const satisfies Record<string, Key>;

function IdentityList({ d }: { d: ModemIdentity }) {
  const id = d.ident;
  return (
    <KvList
      items={[
        {
          label: "IMEI",
          value: <span className="tabular-nums">{id?.imei ?? "—"}</span>,
        },
        {
          label: "IMSI",
          value: <span className="tabular-nums">{id?.imsi ?? "—"}</span>,
        },
        {
          label: "ICCID",
          value: <span className="tabular-nums">{id?.iccid ?? "—"}</span>,
        },
        {
          label: t("cellular.signal.identity.ownNumber"),
          value: d.msisdn ? (
            <span className="tabular-nums">
              {d.msisdn}
              {d.msisdn_src && (
                <span className="text-muted-foreground">
                  {t("cellular.signal.identity.from", {
                    src: t(MSISDN_SRC[d.msisdn_src]),
                  })}
                </span>
              )}
            </span>
          ) : (
            t("cellular.signal.identity.notRead")
          ),
        },
        {
          label: t("cellular.signal.identity.vendorModel"),
          value: [id?.mfg, id?.model].filter(Boolean).join(" ") || "—",
        },
        { label: t("cellular.signal.identity.firmware"), value: id?.fw ?? "—" },
        { label: t("cellular.shared.simSlot"), value: `SIM${d.slot}` },
        {
          label: t("cellular.signal.identity.readAt"),
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
        <CardTitle>{t("cellular.signal.identity.title")}</CardTitle>
        <CardDescription>
          {t("cellular.signal.identity.description")}
        </CardDescription>
        <CardAction>
          {shown ? (
            <Button variant="outline" size="sm" onClick={() => reveal.reset()}>
              <EyeOffIcon data-icon="inline-start" />
              {t("common.action.hide")}
            </Button>
          ) : (
            <PendingButton
              variant="outline"
              size="sm"
              pending={reveal.isPending}
              icon={<EyeIcon data-icon="inline-start" />}
              onClick={() => reveal.mutate()}
            >
              {t("common.action.show")}
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
            {t("cellular.signal.identity.hidden")}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
