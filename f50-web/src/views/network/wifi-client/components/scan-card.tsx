import { useState } from "react";
import { JobProgress } from "@/components/f50/job-progress";
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
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { isJobStarted } from "@/lib/jobs";
import { useMutation } from "@tanstack/react-query";
import { PlusIcon, RadarIcon } from "lucide-react";
import { toast } from "sonner";
import { NetworksTable } from "./networks-table";
import { networksOf, scanNetworks } from "../api";
import type { ScanNetwork } from "../types";

/**
 * Scan for networks: an immediate list while the supplicant holds the radio, else a job (`iw scan`, which fails while
 * the radio is the hotspot's).
 */
export function ScanCard({
  disabled,
  onJoin,
}: {
  disabled: boolean;
  onJoin: (n: ScanNetwork | null) => void;
}) {
  const [job, setJob] = useState<string | null>(null);
  const [jobRunning, setJobRunning] = useState(false);
  const [networks, setNetworks] = useState<ScanNetwork[] | null>(null);
  const scan = useMutation({
    mutationFn: scanNetworks,
    onSuccess: (r) => {
      if (isJobStarted(r)) {
        setJob(r.job);
        setJobRunning(true);
      } else {
        setJob(null);
        setNetworks(r.networks);
      }
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const scanning = scan.isPending || jobRunning;

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("network.client.scan.title")}</CardTitle>
        <CardDescription>
          {t("network.client.scan.description")}
        </CardDescription>
        <CardAction className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={disabled}
            onClick={() => onJoin(null)}
          >
            <PlusIcon data-icon="inline-start" />
            {t("network.client.scan.manual")}
          </Button>
          <PendingButton
            size="sm"
            pending={scanning}
            disabled={disabled}
            onClick={() => scan.mutate()}
          >
            <RadarIcon data-icon="inline-start" />
            {scanning
              ? t("network.client.scan.scanning")
              : t("network.client.scan.scan")}
          </PendingButton>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {job && (
          <JobProgress
            key={job}
            id={job}
            title={t("network.client.scan.job")}
            onDone={(j) => {
              setJobRunning(false);
              if (j.state === "done") {
                setNetworks(networksOf(j.result));
                setJob(null);
              }
            }}
          />
        )}
        {networks ? (
          <NetworksTable
            networks={networks}
            disabled={disabled}
            onJoin={onJoin}
          />
        ) : (
          !job && (
            <Empty>
              <EmptyHeader>
                <EmptyTitle>{t("network.client.scan.emptyTitle")}</EmptyTitle>
                <EmptyDescription>
                  {t("network.client.scan.emptyDescription")}
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )
        )}
      </CardContent>
    </Card>
  );
}
