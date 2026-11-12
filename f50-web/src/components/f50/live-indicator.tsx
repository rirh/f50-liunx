import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { type LiveState, useLiveState } from "@/hooks/use-live";
import { type Key, t } from "@/i18n";
import { cn } from "@/lib/utils";

/** The label and the tooltip of each state, as keys (read in the current language at render). */
const TEXT: Record<Exclude<LiveState, "off">, { label: Key; hint: Key }> = {
  live: { label: "app.live.live", hint: "app.live.hint.live" },
  connecting: {
    label: "app.live.connecting",
    hint: "app.live.hint.connecting",
  },
  retrying: { label: "app.live.polling", hint: "app.live.hint.retrying" },
  denied: { label: "app.live.polling", hint: "app.live.hint.denied" },
};

/** Whether the pages are pushed to or polling, in the header. */
export function LiveIndicator() {
  const state = useLiveState();
  if (state === "off") return null;
  const text = { label: t(TEXT[state].label), hint: t(TEXT[state].hint) };
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs text-muted-foreground"
          aria-label={`${text.label}: ${text.hint}`}
        >
          <span className="relative flex size-2">
            {state === "live" && (
              <span className="absolute inline-flex size-full animate-ping rounded-full bg-emerald-500/60 motion-reduce:animate-none" />
            )}
            <span
              className={cn(
                "relative inline-flex size-2 rounded-full",
                state === "live" && "bg-emerald-500",
                state === "connecting" && "bg-amber-400",
                (state === "retrying" || state === "denied") &&
                  "bg-muted-foreground/50",
              )}
            />
          </span>
          {text.label}
        </span>
      </TooltipTrigger>
      <TooltipContent>{text.hint}</TooltipContent>
    </Tooltip>
  );
}
