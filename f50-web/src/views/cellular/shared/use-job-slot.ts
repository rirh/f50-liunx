import { useCallback, useState } from "react";

export interface RunningJob {
  id: string;
  title: string;
}

/** The job a page started last; `JobSlot` shows it until the next one replaces it. */
export function useJobSlot() {
  const [job, setJob] = useState<RunningJob | null>(null);
  const start = useCallback(
    (id: string, title: string) => setJob({ id, title }),
    [],
  );
  return { job, start };
}
