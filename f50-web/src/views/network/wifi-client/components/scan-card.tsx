import { useEffect, useRef, useState } from "react";
import { JobProgress } from "@/components/f50/job-progress";
import { PendingButton } from "@/components/f50/pending-button";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { format } from "date-fns";
import { PlusIcon, RadarIcon, WifiIcon } from "lucide-react";
import { toast } from "sonner";
import { NetworksTable } from "./networks-table";
import { networksOf, scanNetworks, scanReboot } from "../api";
import type { ClientStatus, ScanNetwork } from "../types";

/**
 * Scan for networks: an immediate list while the supplicant holds the radio, else a job (`iw scan`). While the radio
 * is the hotspot it cannot scan at all, so Scan offers a reboot into scan mode (the hotspot off for ten minutes);
 * in that mode the card says until when, scans once by itself, and can bring the hotspot back early.
 */
export function ScanCard({
  status,
  disabled,
  onJoin,
  onRebooting,
  onRestoreHotspot,
}: {
  status: ClientStatus;
  disabled: boolean;
  onJoin: (n: ScanNetwork | null) => void;
  /** The device reboots into scan mode now (at this time, ms). */
  onRebooting: (since: number) => void;
  onRestoreHotspot: () => void;
}) {
  const [job, setJob] = useState<string | null>(null);
  const [jobRunning, setJobRunning] = useState(false);
  const [networks, setNetworks] = useState<ScanNetwork[] | null>(null);
  const [confirm, setConfirm] = useState(false);
  const hotspot = status.mode === "hotspot";
  const scanMode = status.scan_until != null;

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
  const reboot = useMutation({
    mutationFn: scanReboot,
    onSuccess: () => {
      setConfirm(false);
      onRebooting(Date.now());
    },
    onError: (e) =>
      toast.error(
        t("network.client.scan.rebootFailed", { error: errorMessage(e) }),
      ),
  });
  const scanning = scan.isPending || jobRunning;

  // a scan-mode boot is for scanning: start once by itself
  const autoScanned = useRef(false);
  useEffect(() => {
    if (scanMode && !autoScanned.current && !disabled) {
      autoScanned.current = true;
      scan.mutate();
    }
  }, [scanMode, disabled, scan]);

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("network.client.scan.title")}</CardTitle>
        <CardDescription>
          {hotspot
            ? t("network.client.scan.descriptionHotspot")
            : t("network.client.scan.description")}
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
            onClick={() => (hotspot ? setConfirm(true) : scan.mutate())}
          >
            <RadarIcon data-icon="inline-start" />
            {scanning
              ? t("network.client.scan.scanning")
              : t("network.client.scan.scan")}
          </PendingButton>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {scanMode && status.scan_until != null && (
          <Alert>
            <RadarIcon />
            <AlertTitle>{t("network.client.scan.modeTitle")}</AlertTitle>
            <AlertDescription className="flex flex-wrap items-center gap-2">
              {t("network.client.scan.modeBody", {
                time: format(status.scan_until * 1000, "HH:mm"),
              })}
              <Button
                size="xs"
                variant="outline"
                disabled={disabled}
                onClick={onRestoreHotspot}
              >
                <WifiIcon data-icon="inline-start" />
                {t("network.client.scan.restoreHotspot")}
              </Button>
            </AlertDescription>
          </Alert>
        )}
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
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("network.client.scan.rebootTitle")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("network.client.scan.rebootBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={reboot.isPending}>
              {t("common.action.cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              disabled={reboot.isPending}
              onClick={(e) => {
                e.preventDefault();
                reboot.mutate();
              }}
            >
              {t("network.client.scan.rebootConfirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  );
}
