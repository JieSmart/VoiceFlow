import { lazy, Suspense } from "react";
import { useAppStore } from "../store";
import { ContextMenuHost } from "../ui/ContextMenuHost";
import { pageMeta } from "./pageMeta";
import { Sidebar } from "./Sidebar";
import { Titlebar } from "./Titlebar";

const HomePage = lazy(() =>
  import("../pages/HomePage").then((module) => ({ default: module.HomePage })),
);
const HotwordsPage = lazy(() =>
  import("../pages/HotwordsPage").then((module) => ({ default: module.HotwordsPage })),
);
const RecordsPage = lazy(() =>
  import("../pages/RecordsPage").then((module) => ({ default: module.RecordsPage })),
);
const SettingsModal = lazy(() =>
  import("../settings/SettingsModal").then((module) => ({ default: module.SettingsModal })),
);

export function MainApp() {
  const route = useAppStore((s) => s.route),
    open = useAppStore((s) => s.settingsOpen);
  const [title, sub] = pageMeta[route];
  return (
    <ContextMenuHost className="app-shell">
      <Titlebar />
      <div className="body">
        <Sidebar />
        <main className="main">
          <div className="page-head">
            <h1>{title}</h1>
            <p>{sub}</p>
          </div>
          <Suspense fallback={<div className="page-loading">正在加载…</div>}>
            {route === "home" ? (
              <HomePage />
            ) : route === "hotwords" ? (
              <HotwordsPage />
            ) : (
              <RecordsPage />
            )}
          </Suspense>
        </main>
      </div>
      {open && (
        <Suspense fallback={null}>
          <SettingsModal />
        </Suspense>
      )}
    </ContextMenuHost>
  );
}
