import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Spinner } from "@/components/ui/spinner";
import { SweepShine } from "@/components/ui/sweep-shine";
import { t } from "@/i18n";
import { logout } from "@/lib/ubus";
import { useQuery } from "@tanstack/react-query";
import { PowerOffIcon } from "lucide-react";
import type { PowerAction } from "./power-card";
import { deviceAnswers } from "../api";

const POLL_MS = 3000;
/** Up before it was seen down: still counts once this long has passed (the reboot may have been quicker than a poll). */
const TRUST_UP_AFTER_MS = 60_000;
/** After this long the page says the device may need a look. */
const SLOW_MS = 5 * 60_000;

function useElapsed(since: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return Math.max(0, Math.floor((now - since) / 1000));
}

function useBackToLogin() {
  const navigate = useNavigate();
  return () => {
    void logout().finally(() => navigate("/login", { replace: true }));
  };
}

/** After a reboot: polls until the device answers again, then back to the login. */
function RebootWait({ since }: { since: number }) {
  const elapsed = useElapsed(since);
  const toLogin = useBackToLogin();
  const seenDown = useRef(false);
  const up = useQuery({
    queryKey: ["system", "reboot-wait", since],
    queryFn: async () => {
      const answers = await deviceAnswers();
      if (!answers) seenDown.current = true;
      return (
        answers && (seenDown.current || Date.now() - since > TRUST_UP_AFTER_MS)
      );
    },
    refetchInterval: (q) => (q.state.data ? false : POLL_MS),
    refetchIntervalInBackground: true,
    retry: false,
    gcTime: 0,
  });
  const back = up.data === true;
  useEffect(() => {
    if (back) toLogin();
    // toLogin is recreated each render; run once when the device is back
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [back]);
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Spinner />
        </EmptyMedia>
        <EmptyTitle>
          <SweepShine>
            {back
              ? t("system.settings.wait.back")
              : t("system.settings.wait.waiting")}
          </SweepShine>
        </EmptyTitle>
        <EmptyDescription>
          {t("system.settings.wait.elapsed", { n: elapsed })}
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        {elapsed * 1000 > SLOW_MS && (
          <Alert variant="destructive">
            <AlertTitle>{t("system.settings.wait.slowTitle")}</AlertTitle>
            <AlertDescription>
              {t("system.settings.wait.slowDescription")}
            </AlertDescription>
          </Alert>
        )}
        <Button variant="outline" size="sm" onClick={toLogin}>
          {t("system.settings.wait.toLoginNow")}
        </Button>
      </EmptyContent>
    </Empty>
  );
}

function PoweroffDone() {
  const toLogin = useBackToLogin();
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <PowerOffIcon />
        </EmptyMedia>
        <EmptyTitle>{t("system.settings.wait.offTitle")}</EmptyTitle>
        <EmptyDescription>
          {t("system.settings.wait.offDescription")}
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" size="sm" onClick={toLogin}>
          {t("system.settings.wait.toLogin")}
        </Button>
      </EmptyContent>
    </Empty>
  );
}

export function PowerWait({
  action,
  since,
}: {
  action: PowerAction;
  since: number;
}) {
  return action === "reboot" ? <RebootWait since={since} /> : <PoweroffDone />;
}
