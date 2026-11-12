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
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
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
            {back ? "设备已恢复，正在返回登录页" : "正在等待设备重启"}
          </SweepShine>
        </EmptyTitle>
        <EmptyDescription>
          已等待 {elapsed} 秒，每 3 秒检测一次。设备恢复响应后将自动返回登录页。
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        {elapsed * 1000 > SLOW_MS && (
          <Alert variant="destructive">
            <AlertTitle>设备长时间没有响应</AlertTitle>
            <AlertDescription>
              请检查设备指示灯和连接；如果设备回退到了 Android
              系统，网页将无法访问。
            </AlertDescription>
          </Alert>
        )}
        <Button variant="outline" size="sm" onClick={toLogin}>
          直接返回登录页
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
        <EmptyTitle>设备正在关机</EmptyTitle>
        <EmptyDescription>
          设备将在几秒内断电。需要时请按电源键重新开机，再重新登录。
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" size="sm" onClick={toLogin}>
          返回登录页
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
