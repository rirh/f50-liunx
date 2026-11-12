import type { ReactNode } from "react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { t } from "@/i18n";
import { errorMessage } from "@/lib/format";
import { isUnsupported } from "@/lib/ubus";
import type { UseQueryResult } from "@tanstack/react-query";

/**
 * A query's three states in one place: a skeleton while loading, an alert (with retry) on failure — or "not
 * available on this device" when f50d says `unsupported` — and the content once there is data.
 */
export function QueryState<T>({
  query,
  children,
  skeletonRows = 3,
}: {
  query: UseQueryResult<T>;
  children: (data: T) => ReactNode;
  skeletonRows?: number;
}) {
  if (query.isPending) {
    return (
      <div className="flex flex-col gap-2">
        {Array.from({ length: skeletonRows }, (_, i) => (
          <Skeleton key={i} className="h-5 w-full" />
        ))}
      </div>
    );
  }
  if (query.isError) {
    if (isUnsupported(query.error)) {
      return (
        <Empty>
          <EmptyHeader>
            <EmptyTitle>{t("app.query.unsupported")}</EmptyTitle>
            <EmptyDescription>{errorMessage(query.error)}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      );
    }
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("app.query.failed")}</AlertTitle>
        <AlertDescription className="flex flex-col items-start gap-2">
          {errorMessage(query.error)}
          <Button size="sm" variant="outline" onClick={() => query.refetch()}>
            {t("app.query.retry")}
          </Button>
        </AlertDescription>
      </Alert>
    );
  }
  return <>{children(query.data as T)}</>;
}
