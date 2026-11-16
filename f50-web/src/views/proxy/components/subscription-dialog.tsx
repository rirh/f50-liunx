import { useState } from "react";
import { ConfirmButton } from "@/components/f50/confirm-button";
import { FormDialog } from "@/components/f50/form-dialog";
import { Button } from "@/components/ui/button";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { t } from "@/i18n";
import { PencilIcon } from "lucide-react";
import { subscriptionTitle } from "../lib/format";
import { useEditSubscription, useRemoveSubscription } from "../queries";
import type { Subscription } from "../types";

const URL_RE = /^https?:\/\/\S+$/;

/** A subscription's name and address, edited in a dialog; deleting it (with its nodes) from the same place. */
export function SubscriptionDialog({ sub }: { sub: Subscription }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState(sub.name);
  const [url, setUrl] = useState(sub.url);
  const edit = useEditSubscription();
  const remove = useRemoveSubscription();
  const formId = `sub-${sub.id}`;
  const urlOk = URL_RE.test(url.trim());
  const dirty = name.trim() !== sub.name || url.trim() !== sub.url;

  const onOpenChange = (next: boolean) => {
    if (next) {
      setName(sub.name);
      setUrl(sub.url);
    }
    setOpen(next);
  };

  return (
    <>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={t("proxy.sub.edit", { name: subscriptionTitle(sub) })}
        className="text-muted-foreground hover:text-primary"
        onClick={() => onOpenChange(true)}
      >
        <PencilIcon />
      </Button>
      <FormDialog
        open={open}
        onOpenChange={onOpenChange}
        title={t("proxy.sub.editTitle")}
        description={t("proxy.sub.editDescription")}
        formId={formId}
        pending={edit.isPending}
        dirty={dirty && urlOk}
        extra={
          <ConfirmButton
            size="sm"
            title={t("proxy.sub.deleteTitle")}
            description={t("proxy.sub.deleteBody", {
              name: subscriptionTitle(sub),
              count: sub.nodes.length,
            })}
            confirmText={t("common.action.delete")}
            pending={remove.isPending}
            onConfirm={() =>
              remove.mutate(sub.id, { onSuccess: () => setOpen(false) })
            }
          >
            {t("proxy.sub.delete")}
          </ConfirmButton>
        }
      >
        <form
          id={formId}
          onSubmit={(e) => {
            e.preventDefault();
            if (!urlOk) return;
            edit.mutate(
              { id: sub.id, name: name.trim(), url: url.trim() },
              { onSuccess: () => setOpen(false) },
            );
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${formId}-name`}>
                {t("proxy.sub.name")}
              </FieldLabel>
              <Input
                id={`${formId}-name`}
                value={name}
                placeholder={sub.title ?? t("proxy.sub.namePlaceholder")}
                onChange={(e) => setName(e.target.value)}
              />
            </Field>
            <Field data-invalid={!urlOk || undefined}>
              <FieldLabel htmlFor={`${formId}-url`}>
                {t("proxy.sub.url")}
              </FieldLabel>
              <Input
                id={`${formId}-url`}
                value={url}
                inputMode="url"
                spellCheck={false}
                autoCapitalize="off"
                aria-invalid={!urlOk || undefined}
                className="font-mono text-xs"
                onChange={(e) => setUrl(e.target.value)}
              />
            </Field>
          </FieldGroup>
        </form>
      </FormDialog>
    </>
  );
}
