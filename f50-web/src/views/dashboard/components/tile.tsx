import type { ReactNode } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import type { LucideIcon } from "lucide-react";
import { TextLink } from "./text-link";
import type { Part } from "../types";

/** One part's state: its own small error (or "unsupported") so a failed part never hides the others. */
export function PartBody<T>({
  part,
  children,
}: {
  part: Part<T> | undefined;
  children: (data: T) => ReactNode;
}) {
  if (!part)
    return <p className="text-sm text-muted-foreground">设备未提供此项</p>;
  if (!part.ok) {
    if (part.code === "unsupported")
      return <p className="text-sm text-muted-foreground">此设备不支持</p>;
    return (
      <Alert variant="destructive">
        <AlertDescription>{part.error || "读取失败"}</AlertDescription>
      </Alert>
    );
  }
  return <>{children(part.data)}</>;
}

/** A dashboard tile: its title links to the page with the details; `action` (a switch) only once there is data. */
export function Tile<T>({
  title,
  icon: Icon,
  to,
  part,
  action,
  children,
}: {
  title: string;
  icon: LucideIcon;
  to: string;
  part: Part<T> | undefined;
  action?: (data: T) => ReactNode;
  children: (data: T) => ReactNode;
}) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Icon aria-hidden className="text-muted-foreground" />
          <TextLink to={to}>{title}</TextLink>
        </CardTitle>
        {action && part?.ok && <CardAction>{action(part.data)}</CardAction>}
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <PartBody part={part}>{children}</PartBody>
      </CardContent>
    </Card>
  );
}

export function TileSkeleton() {
  return (
    <Card size="sm">
      <CardHeader>
        <Skeleton className="h-5 w-24" />
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <Skeleton className="h-5 w-full" />
        <Skeleton className="h-5 w-3/4" />
        <Skeleton className="h-5 w-1/2" />
      </CardContent>
    </Card>
  );
}

/** A big figure with a small caption, for the line under a tile's title. */
export function Figure({
  value,
  caption,
}: {
  value: ReactNode;
  caption?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col">
      <span className="truncate text-lg font-semibold tabular-nums">
        {value}
      </span>
      {caption && (
        <span className="text-xs text-muted-foreground">{caption}</span>
      )}
    </div>
  );
}
