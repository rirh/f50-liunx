import { errorMessage } from "@/lib/format";
import { ApiError } from "@/lib/ubus";
import type { FieldValues, Path, UseFormSetError } from "react-hook-form";
import { toast } from "sonner";

/**
 * A refusal shown where it belongs: on the form field f50d names (`ApiError.field`), else as a toast.
 *
 * `fields` says which of f50d's field names the form has: a list (the same names), or a map from f50d's name to the
 * form's. Without it, f50d's name is taken as the form's. Without `setError` (a plain button), always a toast.
 */
export function reportFormError<T extends FieldValues>(
  e: unknown,
  setError?: UseFormSetError<T>,
  fields?: readonly Path<T>[] | Partial<Record<string, Path<T>>>,
) {
  if (setError && e instanceof ApiError && e.field) {
    let name: Path<T> | undefined;
    if (!fields) name = e.field as Path<T>;
    else if (Array.isArray(fields))
      name = fields.includes(e.field as Path<T>)
        ? (e.field as Path<T>)
        : undefined;
    else name = (fields as Partial<Record<string, Path<T>>>)[e.field];
    if (name) {
      setError(
        name,
        { type: "server", message: errorMessage(e) },
        { shouldFocus: true },
      );
      return;
    }
  }
  toast.error(errorMessage(e));
}
