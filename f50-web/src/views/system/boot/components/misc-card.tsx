import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { t } from "@/i18n";
import { ChevronDownIcon } from "lucide-react";
import type { MiscState, Slot, SlotInfo } from "../types";

function SlotRow({
  name,
  info,
  linux,
}: {
  name: Slot;
  info: SlotInfo;
  linux: boolean;
}) {
  return (
    <TableRow>
      <TableCell className="font-mono">
        <span className="flex items-center gap-2">
          {name.toUpperCase()}
          <Badge variant="outline">{linux ? "Linux" : "Android"}</Badge>
        </span>
      </TableCell>
      <TableCell className="tabular-nums">{info.priority}</TableCell>
      <TableCell className="tabular-nums">{info.tries}</TableCell>
      <TableCell>
        <Badge variant={info.successful ? "secondary" : "outline"}>
          {info.successful ? t("common.state.yes") : t("common.state.no")}
        </Badge>
      </TableCell>
    </TableRow>
  );
}

/** The A/B boot control block in misc, as read now. */
export function MiscCard({
  misc,
  slot,
}: {
  misc: MiscState | null;
  slot: Slot;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("maintenance.boot.misc.title")}</CardTitle>
        <CardDescription>
          {t("maintenance.boot.misc.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {!misc ? (
          <Alert>
            <AlertDescription>
              {t("maintenance.boot.misc.unreadable")}
            </AlertDescription>
          </Alert>
        ) : (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge variant={misc.magic ? "secondary" : "destructive"}>
                {misc.magic
                  ? t("maintenance.boot.misc.magicOk")
                  : t("maintenance.boot.misc.magicMissing")}
              </Badge>
              <Badge variant={misc.crc_ok ? "secondary" : "destructive"}>
                {misc.crc_ok
                  ? t("maintenance.boot.misc.crcOk")
                  : t("maintenance.boot.misc.crcBad")}
              </Badge>
            </div>
            <div className="overflow-x-auto rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("maintenance.boot.misc.slot")}</TableHead>
                    <TableHead>{t("maintenance.boot.misc.priority")}</TableHead>
                    <TableHead>{t("maintenance.boot.misc.tries")}</TableHead>
                    <TableHead>
                      {t("maintenance.boot.misc.successful")}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  <SlotRow name="a" info={misc.slot_a} linux={slot === "a"} />
                  <SlotRow name="b" info={misc.slot_b} linux={slot === "b"} />
                </TableBody>
              </Table>
            </div>
            <Collapsible>
              <CollapsibleTrigger asChild>
                <Button variant="ghost" size="sm" className="group self-start">
                  {t("maintenance.boot.misc.raw")}
                  <ChevronDownIcon
                    data-icon="inline-end"
                    className="transition-transform group-data-[state=open]:rotate-180"
                  />
                </Button>
              </CollapsibleTrigger>
              <CollapsibleContent>
                <pre className="mt-2 overflow-x-auto rounded-md bg-muted p-2 font-mono text-xs break-all whitespace-pre-wrap">
                  {misc.hex.match(/.{1,32}/g)?.join("\n")}
                </pre>
              </CollapsibleContent>
            </Collapsible>
          </>
        )}
      </CardContent>
    </Card>
  );
}
