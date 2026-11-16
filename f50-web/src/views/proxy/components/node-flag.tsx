import { cn } from "@/lib/utils";
import { GlobeIcon } from "lucide-react";
import { countryOf } from "../lib/country";

// 3:2 flags from country-flag-icons (MIT, see assets/flags/LICENSE), only the countries `countryOf` can name.
const FLAGS = import.meta.glob<string>("../assets/flags/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
});
const flagUrl = (code: string) => FLAGS[`../assets/flags/${code}.svg`];

/** The node's country flag, read from its name; a globe when the name says nothing. */
export function NodeFlag({
  name,
  className,
}: {
  name: string;
  className?: string;
}) {
  const code = countryOf(name);
  const url = code ? flagUrl(code) : undefined;
  return (
    <span
      title={code || undefined}
      className={cn(
        "grid h-4 w-6 shrink-0 place-items-center overflow-hidden rounded-[3px] bg-foreground/[0.06] text-muted-foreground shadow-[0_0_0_1px_var(--color-border)]",
        className,
      )}
    >
      {url ? (
        <img src={url} alt={code} className="size-full object-cover" />
      ) : code ? (
        <span className="text-[9px] leading-none font-semibold">{code}</span>
      ) : (
        <GlobeIcon aria-hidden className="size-3" />
      )}
    </span>
  );
}
