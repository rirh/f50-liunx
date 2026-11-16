import { cn } from "@/lib/utils";

/** The F50 logo (the same drawing as public/favicon.svg), inline so it is sharp at any size and needs no request. */
export function BrandMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 512 512"
      aria-hidden
      // the tile is near black: in the dark theme a faint ring keeps its edge on a dark background
      className={cn(
        "size-6 shrink-0 rounded-[22.6%] dark:ring-1 dark:ring-white/15",
        className,
      )}
    >
      <rect width="512" height="512" rx="116" fill="#0f1b1f" />
      <g fill="none" stroke="#fff" strokeWidth="42" strokeLinecap="round">
        <path d="M188.8 298.8A95 95 0 0 1 323.2 298.8" />
        <path d="M139.3 249.3A165 165 0 0 1 372.7 249.3" />
        <path d="M89.8 199.8A235 235 0 0 1 422.2 199.8" />
      </g>
      <circle cx="256" cy="366" r="38" fill="#2dd4bf" />
    </svg>
  );
}
