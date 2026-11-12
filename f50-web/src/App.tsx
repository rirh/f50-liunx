import { Link, Route, Routes } from "react-router-dom";
import { HomePage } from "@/views/home";
import { AboutPage } from "@/views/about";
import { AppLayout } from "@/layout";
import { Button } from "@/components/ui/button";

export function App() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route path="/" element={<HomePage />} />
        <Route path="/about" element={<AboutPage />} />
        <Route
          path="*"
          element={
            <section className="flex flex-col items-center gap-6 py-16">
              <h1 className="text-3xl font-semibold">404 · 页面不存在</h1>
              <Button asChild>
                <Link to="/">返回首页</Link>
              </Button>
            </section>
          }
        />
      </Route>
    </Routes>
  );
}
