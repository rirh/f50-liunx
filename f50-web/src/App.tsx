import { lazy } from "react";
import { Link, Route, Routes } from "react-router-dom";
import { Button } from "@/components/ui/button";
import { t } from "@/i18n";
import { AppLayout } from "@/layout";
import LoginPage from "@/views/login";

// one lazily loaded chunk per page (the sidebar in layout/nav.ts lists the same paths)
const V0 = lazy(() => import("@/views/dashboard"));
const V1 = lazy(() => import("@/views/cellular/signal"));
const V2 = lazy(() => import("@/views/cellular/sim"));
const V3 = lazy(() => import("@/views/cellular/locks"));
const V4 = lazy(() => import("@/views/cellular/data"));
const V5 = lazy(() => import("@/views/cellular/traffic"));
const V6 = lazy(() => import("@/views/sms"));
const V7 = lazy(() => import("@/views/network/wifi"));
const V8 = lazy(() => import("@/views/network/wifi-client"));
const V9 = lazy(() => import("@/views/network/lan"));
const V10 = lazy(() => import("@/views/network/usb"));
const V11 = lazy(() => import("@/views/system/overview"));
const V12 = lazy(() => import("@/views/system/settings"));
const V13 = lazy(() => import("@/views/system/cpu"));
const V14 = lazy(() => import("@/views/system/leds"));
const V15 = lazy(() => import("@/views/system/update"));
const V16 = lazy(() => import("@/views/system/boot"));
const V17 = lazy(() => import("@/views/system/logs"));
const V18 = lazy(() => import("@/views/tools/at"));
const V19 = lazy(() => import("@/views/tools/jobs"));

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route element={<AppLayout />}>
        <Route index element={<V0 />} />
        <Route path="cellular/signal" element={<V1 />} />
        <Route path="cellular/sim" element={<V2 />} />
        <Route path="cellular/locks" element={<V3 />} />
        <Route path="cellular/data" element={<V4 />} />
        <Route path="cellular/traffic" element={<V5 />} />
        <Route path="sms" element={<V6 />} />
        <Route path="network/wifi" element={<V7 />} />
        <Route path="network/wifi-client" element={<V8 />} />
        <Route path="network/lan" element={<V9 />} />
        <Route path="network/usb" element={<V10 />} />
        <Route path="system/overview" element={<V11 />} />
        <Route path="system/settings" element={<V12 />} />
        <Route path="system/cpu" element={<V13 />} />
        <Route path="system/leds" element={<V14 />} />
        <Route path="system/update" element={<V15 />} />
        <Route path="system/boot" element={<V16 />} />
        <Route path="system/logs" element={<V17 />} />
        <Route path="tools/at" element={<V18 />} />
        <Route path="tools/jobs" element={<V19 />} />
        <Route
          path="*"
          element={
            <section className="flex flex-col items-center gap-6 py-16">
              <h1 className="text-3xl font-semibold">
                {t("app.notFound.title")}
              </h1>
              <Button asChild>
                <Link to="/">{t("app.notFound.back")}</Link>
              </Button>
            </section>
          }
        />
      </Route>
    </Routes>
  );
}
