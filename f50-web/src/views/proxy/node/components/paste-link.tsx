import { useState } from "react";
import { PendingButton } from "@/components/f50/pending-button";
import { Textarea } from "@/components/ui/textarea";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { ClipboardPasteIcon } from "lucide-react";
import { inspectNodes } from "../../api";
import type { InspectedNode } from "../../types";

/**
 * Paste share links (vless, hysteria2, ss, trojan, vmess, socks5, http; One Node, v2rayN or Shadowrocket form): the
 * engine reads them (`node_inspect`) and hands them on. One good link is saved at once; one with a problem fills the
 * form to fix by hand; several are all added as local nodes.
 */
export function PasteLink({
  onNodes,
  saving,
}: {
  onNodes: (nodes: InspectedNode[]) => void;
  saving: boolean;
}) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const count = text.split(/\s+/).filter((l) => l.includes("://")).length;

  const read = async (value: string) => {
    if (!value.includes("://") || reading) return;
    setReading(true);
    setError(null);
    try {
      const { nodes } = await inspectNodes(value.trim());
      if (!nodes.length) setError(t("proxy.paste.none"));
      else {
        onNodes(nodes);
        setText("");
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setReading(false);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <Textarea
        aria-label={t("proxy.paste.label")}
        rows={3}
        spellCheck={false}
        value={text}
        placeholder={t("proxy.paste.placeholder")}
        onChange={(e) => {
          setText(e.target.value);
          setError(null);
        }}
        onPaste={(e) => {
          // a paste into the empty box is read at once
          const pasted = e.clipboardData.getData("text");
          if (!text.trim() && pasted.includes("://")) {
            e.preventDefault();
            setText(pasted);
            void read(pasted);
          }
        }}
        className="min-h-20 font-mono text-xs"
      />
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs text-muted-foreground">
          {error ? (
            <span className="text-destructive">{error}</span>
          ) : count > 1 ? (
            t("proxy.paste.many", { count })
          ) : (
            t("proxy.paste.hint")
          )}
        </span>
        <PendingButton
          type="button"
          size="sm"
          variant="outline"
          pending={reading || saving}
          disabled={!count}
          onClick={() => void read(text)}
        >
          <ClipboardPasteIcon data-icon="inline-start" />
          {saving ? t("common.action.saving") : t("proxy.paste.read")}
        </PendingButton>
      </div>
    </div>
  );
}
