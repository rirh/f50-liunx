import { PendingButton } from "@/components/f50/pending-button";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { t } from "@/i18n";
import { useAtom } from "jotai";
import {
  CopyIcon,
  DownloadIcon,
  RefreshCwIcon,
  SearchIcon,
} from "lucide-react";
import { autoRefreshAtom, filterAtom, linesAtom } from "../store";
import { LINE_OPTIONS, type LineCount } from "../types";

export function LogToolbar({
  fetching,
  hasText,
  onRefresh,
  onCopy,
  onDownload,
}: {
  fetching: boolean;
  hasText: boolean;
  onRefresh: () => void;
  onCopy: () => void;
  onDownload: () => void;
}) {
  const [lines, setLines] = useAtom(linesAtom);
  const [auto, setAuto] = useAtom(autoRefreshAtom);
  const [filter, setFilter] = useAtom(filterAtom);
  return (
    <div className="flex flex-col gap-3 md:flex-row md:flex-wrap md:items-center">
      <InputGroup className="md:w-64">
        <InputGroupInput
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={t("maintenance.logs.toolbar.filterPlaceholder")}
          aria-label={t("maintenance.logs.toolbar.filterAria")}
        />
        <InputGroupAddon>
          <SearchIcon />
        </InputGroupAddon>
      </InputGroup>
      <div className="flex flex-wrap items-center gap-3">
        <Select
          value={String(lines)}
          onValueChange={(v) => setLines(Number(v) as LineCount)}
        >
          <SelectTrigger
            className="w-36"
            aria-label={t("maintenance.logs.toolbar.linesAria")}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {LINE_OPTIONS.map((n) => (
                <SelectItem key={n} value={String(n)}>
                  {t("maintenance.logs.toolbar.lines", { count: n })}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <Field orientation="horizontal" className="w-auto">
          <Switch id="logs-auto" checked={auto} onCheckedChange={setAuto} />
          <FieldLabel htmlFor="logs-auto" className="whitespace-nowrap">
            {t("maintenance.logs.toolbar.auto")}
          </FieldLabel>
        </Field>
      </div>
      <div className="flex flex-wrap items-center gap-2 md:ml-auto">
        <PendingButton
          size="sm"
          variant="outline"
          pending={fetching}
          onClick={onRefresh}
        >
          <RefreshCwIcon data-icon="inline-start" />
          {t("common.action.refresh")}
        </PendingButton>
        <Button
          size="sm"
          variant="outline"
          disabled={!hasText}
          onClick={onCopy}
        >
          <CopyIcon data-icon="inline-start" />
          {t("common.action.copy")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={!hasText}
          onClick={onDownload}
        >
          <DownloadIcon data-icon="inline-start" />
          {t("common.action.download")}
        </Button>
      </div>
    </div>
  );
}
