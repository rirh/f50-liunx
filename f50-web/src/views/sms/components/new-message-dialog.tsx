import { type RefObject, useMemo, useRef, useState } from "react";
import { ResponsiveDialog } from "@/components/f50/form-dialog";
import { JobProgress } from "@/components/f50/job-progress";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
  ComboboxTrigger,
} from "@/components/ui/combobox";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { InputGroup, InputGroupTextarea } from "@/components/ui/input-group";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t, useLocale } from "@/i18n";
import { useQuery } from "@tanstack/react-query";
import { useAtom } from "jotai";
import { HistoryIcon, SendIcon } from "lucide-react";
import { SmsCounter } from "./sms-counter";
import { useSendForm } from "./use-send-form";
import { listRecipients, smsKeys } from "../api";
import { type CallingRegion, callingRegions } from "../lib/calling-codes";
import { composeOpenAtom } from "../store/sms";

const FORM_ID = "sms-new-message";
const FLAGS = import.meta.glob<string>("../../proxy/assets/flags/*.svg", {
  eager: true,
  query: "?url",
  import: "default",
});

function RecipientFlag({ region }: { region: string }) {
  if (!region) return null;
  const url = FLAGS[`../../proxy/assets/flags/${region}.svg`];
  return (
    <span
      aria-hidden
      className="grid h-3.5 w-5 shrink-0 place-items-center overflow-hidden rounded-xs text-sm leading-none"
    >
      {url ? (
        <img src={url} alt="" className="size-full object-cover" />
      ) : (
        String.fromCodePoint(
          0x1f1e6 + region.charCodeAt(0) - 65,
          0x1f1e6 + region.charCodeAt(1) - 65,
        )
      )}
    </span>
  );
}

function RecipientHistory({
  container,
  disabled,
  onSelect,
}: {
  container: RefObject<HTMLElement | null>;
  disabled: boolean;
  onSelect: (number: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const history = useQuery({
    queryKey: smsKeys.recipients,
    queryFn: ({ signal }) => listRecipients(signal),
    enabled: open,
    retry: false,
  });
  const recipients = history.data?.recipients ?? [];
  const label = t("sms.newMessage.recipientHistory");

  return (
    <Combobox
      items={recipients}
      open={open}
      onOpenChange={setOpen}
      disabled={disabled}
      autoHighlight
      onValueChange={(number) => {
        if (number) onSelect(number);
      }}
    >
      <ComboboxTrigger
        render={<Button variant="ghost" size="icon-sm" type="button" />}
        showChevron={false}
        className="text-muted-foreground"
        tabIndex={0}
        aria-label={label}
        title={label}
      >
        <HistoryIcon aria-hidden />
      </ComboboxTrigger>
      <ComboboxContent
        container={container}
        side="top"
        align="end"
        className="w-64"
        aria-label={label}
        data-vaul-no-drag
      >
        <ComboboxInput
          showTrigger={false}
          inputMode="tel"
          aria-label={t("sms.newMessage.recipientHistorySearch")}
          placeholder={t("sms.newMessage.recipientHistorySearch")}
        />
        <ComboboxEmpty>
          {history.isPending
            ? t("common.action.loading")
            : history.isError
              ? t("sms.newMessage.recipientHistoryFailed")
              : recipients.length
                ? t("sms.newMessage.recipientHistoryNoMatch")
                : t("sms.newMessage.recipientHistoryEmpty")}
        </ComboboxEmpty>
        <ComboboxList>
          {(number: string) => (
            <ComboboxItem
              key={number}
              value={number}
              className="min-h-8 tabular-nums"
            >
              {number}
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  );
}

/**
 * Counts openings. The panel is keyed by it, so every opening starts with an empty form and no send job left over,
 * as before; closing keeps the same panel for the drawer's or dialog's exit animation.
 */
function useOpenCount(open: boolean) {
  const [count, setCount] = useState(0);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setCount(count + 1);
  }
  return count;
}

/** A message to any number; once sent, its conversation opens. A dialog on a wide screen, a bottom drawer on a phone. */
export function NewMessageDialog({
  onSent,
}: {
  onSent: (peer: string) => void;
}) {
  const [open, setOpen] = useAtom(composeOpenAtom);
  const session = useOpenCount(open);
  return (
    <NewMessagePanel
      key={session}
      open={open}
      onOpenChange={setOpen}
      onSent={(peer) => {
        setOpen(false);
        onSent(peer);
      }}
    />
  );
}

function NewMessagePanel({
  open,
  onOpenChange,
  onSent,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSent: (peer: string) => void;
}) {
  const locale = useLocale();
  const localName = t("sms.newMessage.localNumber");
  const regions = useMemo(
    () => [
      {
        region: "",
        code: "",
        name: localName,
        search: localName.toLowerCase(),
      },
      ...callingRegions(locale),
    ],
    [locale, localName],
  );
  const [region, setRegion] = useState("CN");
  const selected = regions.find((item) => item.region === region) ?? regions[0];
  const { form, submit, pending, job, onJobDone } = useSendForm(
    "",
    onSent,
    selected.code,
  );
  // Keep the search input inside the focus trap of the dialog or mobile drawer.
  const dialog = useRef<HTMLElement>(null);
  const { errors } = form.formState;
  const text = form.watch("text");
  const fullNumber = form.watch("number").trimStart().startsWith("+");
  const codeLabel = fullNumber
    ? t("sms.newMessage.fullNumber")
    : [selected.name, selected.code].filter(Boolean).join(" ");
  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("sms.newMessage.title")}
      description={t("sms.newMessage.description")}
      footer={
        <>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            {t("common.action.cancel")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            disabled={pending}
            aria-busy={pending}
          >
            <SendIcon data-icon="inline-start" />
            {pending ? (
              <SweepShine>{t("sms.composer.sending")}</SweepShine>
            ) : (
              t("sms.composer.send")
            )}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} onSubmit={submit} className="flex flex-col gap-4">
        <Field data-invalid={!!errors.number || !!errors.text}>
          <FieldLabel htmlFor="sms-text" className="sr-only">
            {t("sms.newMessage.text")}
          </FieldLabel>
          <InputGroup
            className="rounded-xl border border-border/60"
            ref={(element) => {
              dialog.current =
                element?.closest<HTMLElement>(
                  '[data-slot="dialog-content"], [data-slot="drawer-content"]',
                ) ?? null;
            }}
          >
            <InputGroupTextarea
              id="sms-text"
              rows={5}
              placeholder={t("sms.composer.placeholder")}
              className="min-h-32 max-h-64 px-3 py-2.5 text-base sm:text-sm"
              aria-invalid={!!errors.text}
              disabled={pending}
              {...form.register("text")}
            />
            <div
              data-align="block-end"
              className="flex w-full flex-wrap items-center gap-x-3 gap-y-1.5 border-t border-border/60 px-2.5 py-2"
            >
              <div className="flex min-w-0 flex-[1_0_12rem] items-center gap-1.5">
                <FieldLabel htmlFor="sms-number" className="sr-only">
                  {t("sms.newMessage.number")}
                </FieldLabel>
                <Combobox
                  items={regions}
                  value={selected}
                  disabled={pending || fullNumber}
                  autoHighlight
                  onValueChange={(item) => item && setRegion(item.region)}
                  itemToStringLabel={(item) => item.name}
                  itemToStringValue={(item) => item.region}
                  filter={(item, query) =>
                    item.search.includes(
                      query.normalize("NFKC").trim().toLowerCase(),
                    )
                  }
                >
                  <ComboboxTrigger
                    render={<Button variant="outline" type="button" />}
                    className="h-7 shrink-0 gap-1 tabular-nums"
                    tabIndex={0}
                    aria-label={t("sms.newMessage.callingCode", {
                      code: codeLabel,
                    })}
                    title={codeLabel}
                  >
                    {!fullNumber && <RecipientFlag region={selected.region} />}
                    {fullNumber
                      ? t("sms.newMessage.fullNumber")
                      : selected.code || localName}
                  </ComboboxTrigger>
                  <ComboboxContent
                    container={dialog}
                    side="top"
                    className="w-72"
                    aria-label={t("sms.newMessage.callingCodeSearch")}
                    data-vaul-no-drag
                  >
                    <ComboboxInput
                      showTrigger={false}
                      aria-label={t("sms.newMessage.callingCodeSearch")}
                      placeholder={t("sms.newMessage.callingCodeSearch")}
                    />
                    <ComboboxEmpty>
                      {t("sms.newMessage.callingCodeEmpty")}
                    </ComboboxEmpty>
                    <ComboboxList>
                      {(item: CallingRegion) => (
                        <ComboboxItem
                          key={item.region}
                          value={item}
                          className="min-h-8"
                        >
                          <RecipientFlag region={item.region} />
                          <span className="grow">{item.name}</span>
                          <span className="shrink-0 tabular-nums text-muted-foreground">
                            {item.code}
                          </span>
                        </ComboboxItem>
                      )}
                    </ComboboxList>
                  </ComboboxContent>
                </Combobox>
                <div className="flex min-w-0 flex-1 items-center rounded-md bg-background/65">
                  <Input
                    id="sms-number"
                    inputMode="tel"
                    autoComplete="tel-national"
                    placeholder={t("sms.newMessage.numberPlaceholder")}
                    className="h-7 min-w-0 flex-1 bg-transparent px-2 text-base tabular-nums sm:text-sm"
                    aria-invalid={!!errors.number}
                    disabled={pending}
                    {...form.register("number")}
                  />
                  <RecipientHistory
                    container={dialog}
                    disabled={pending}
                    onSelect={(number) => {
                      if (!number.startsWith("+")) setRegion("");
                      form.setValue("number", number, { shouldDirty: true });
                      form.clearErrors("number");
                    }}
                  />
                </div>
              </div>
              <SmsCounter
                text={text}
                className="ms-auto shrink-0 whitespace-nowrap text-[11px]"
              />
            </div>
          </InputGroup>
          <FieldError errors={[errors.number, errors.text]} />
        </Field>
        {job && (
          <JobProgress
            id={job.id}
            title={t("sms.composer.jobTitle")}
            onDone={onJobDone}
          />
        )}
      </form>
    </ResponsiveDialog>
  );
}
