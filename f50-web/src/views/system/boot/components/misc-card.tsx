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
          {info.successful ? "是" : "否"}
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
        <CardTitle>misc 启动控制块</CardTitle>
        <CardDescription>引导程序读取的 A/B 槽位状态</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {!misc ? (
          <Alert>
            <AlertDescription>无法读取 misc 分区。</AlertDescription>
          </Alert>
        ) : (
          <>
            <div className="flex flex-wrap gap-2">
              <Badge variant={misc.magic ? "secondary" : "destructive"}>
                {misc.magic ? "标识正确" : "标识缺失"}
              </Badge>
              <Badge variant={misc.crc_ok ? "secondary" : "destructive"}>
                {misc.crc_ok ? "CRC 正确" : "CRC 错误"}
              </Badge>
            </div>
            <div className="overflow-x-auto rounded-md border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>槽位</TableHead>
                    <TableHead>优先级</TableHead>
                    <TableHead>剩余尝试</TableHead>
                    <TableHead>已成功启动</TableHead>
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
                  原始数据
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
