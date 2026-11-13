import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { t } from "@/i18n";
import { logout } from "@/lib/ubus";
import { ArrowRightLeftIcon } from "lucide-react";

/** After `f50.boot android`: the console will not come back, so this says how to return to Linux. */
export function AndroidDone() {
  const navigate = useNavigate();
  return (
    <Empty className="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <ArrowRightLeftIcon />
        </EmptyMedia>
        <EmptyTitle>{t("dashboard.quick.androidDoneTitle")}</EmptyTitle>
        <EmptyDescription>
          {t("dashboard.quick.androidDoneBody")}
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            void logout().finally(() => navigate("/login", { replace: true }))
          }
        >
          {t("dashboard.quick.toLogin")}
        </Button>
      </EmptyContent>
    </Empty>
  );
}
