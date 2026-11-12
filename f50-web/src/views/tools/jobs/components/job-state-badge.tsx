import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";
import { JOB_STATE_LABEL, type JobState } from "@/lib/jobs";

const VARIANT: Record<
  JobState,
  "default" | "secondary" | "destructive" | "outline"
> = {
  running: "default",
  done: "secondary",
  failed: "destructive",
  cancelled: "outline",
  interrupted: "destructive",
};

export function JobStateBadge({ state }: { state: JobState }) {
  return (
    <Badge variant={VARIANT[state]}>
      {state === "running" && <Spinner data-icon="inline-start" />}
      {JOB_STATE_LABEL[state]}
    </Badge>
  );
}
