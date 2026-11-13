import { useMemo } from "react";
import { cn } from "@/lib/utils";
import NiceAvatar, { genConfig } from "react-nice-avatar";

/** The signed-in user's face: a react-nice-avatar seeded by the username, so it stays the same everywhere. */
export function UserAvatar({
  name,
  className,
}: {
  name: string;
  className?: string;
}) {
  const config = useMemo(() => genConfig(name), [name]);
  return (
    <NiceAvatar
      aria-hidden
      className={cn("size-8 shrink-0 rounded-full", className)}
      {...config}
    />
  );
}
