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
import { t } from "@/i18n";
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
    return (
      <p className="text-xs text-muted-foreground">
        {t("common.state.notProvided")}
      </p>
    );
  if (!part.ok) {
    if (part.code === "unsupported")
      return (
        <p className="text-xs text-muted-foreground">
          {t("common.state.unsupported")}
        </p>
      );
    return (
      <Alert variant="destructive">
        <AlertDescription>
          {part.error || t("common.state.readFailed")}
        </AlertDescription>
      </Alert>
    );
  }
  return <>{children(part.data)}</>;
}

/**
 * A compact dashboard tile: the title links to the page with the details; `action` (a switch) appears once there is
 * data. Content is a headline row and at most a few dense facts, so six tiles fit two rows of one screen.
 */
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
    <Card size="sm" className="gap-2">
      <CardHeader className="gap-0">
        <CardTitle className="flex items-center gap-2 text-sm">
          <Icon aria-hidden className="size-4 text-primary" />
          <TextLink to={to}>{title}</TextLink>
        </CardTitle>
        {action && part?.ok && <CardAction>{action(part.data)}</CardAction>}
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <PartBody part={part}>{children}</PartBody>
      </CardContent>
    </Card>
  );
}

export function TileSkeleton({ className }: { className?: string }) {
  return (
    <Card size="sm" className={className}>
      <CardHeader>
        <Skeleton className="h-4 w-24" />
      </CardHeader>
      <CardContent className="flex flex-col gap-2">
        <Skeleton className="h-5 w-3/4" />
        <Skeleton className="h-4 w-full" />
      </CardContent>
    </Card>
  );
}

/** A tile's headline: a figure with a small caption. */
export function Figure({
  value,
  caption,
}: {
  value: ReactNode;
  caption?: ReactNode;
}) {
  return (
    <div className="flex min-w-0 flex-col">
      <span className="truncate text-base leading-tight font-semibold tabular-nums">
        {value}
      </span>
      {caption && (
        <span className="truncate text-xs text-muted-foreground">
          {caption}
        </span>
      )}
    </div>
  );
}
