import { errorMessage } from "@/lib/format";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ledKey, setLed } from "../api";
import type { LedKey } from "../types";

/** One control's `f50.led set`: the reply replaces the cached settings. */
export function useLedSet(key: LedKey, onError?: (e: unknown) => void) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (value: string) => setLed(key, value),
    onSuccess: (r) => {
      qc.setQueryData(ledKey, r);
      toast.success("已保存");
    },
    onError: onError ?? ((e) => toast.error(errorMessage(e))),
    onSettled: () => qc.invalidateQueries({ queryKey: ledKey }),
  });
}
