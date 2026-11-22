import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { FlashProvider } from "../components/Flash";
import { Header } from "../components/Header";
import { useApplyTheme } from "../hooks/useTheme";
import { Placeholder } from "../pages/Placeholder";
import { SystemPage } from "../pages/SystemPage";
import { ImportPage } from "../pages/import/ImportPage";
import { ActivityDetailPage } from "../pages/activities/ActivityDetailPage";
import { ActivityListPage } from "../pages/activities/ActivityListPage";
import { DashboardPage } from "../pages/dashboard/DashboardPage";
import { SegmentDetailPage } from "../pages/segments/SegmentDetailPage";
import { SegmentListPage } from "../pages/segments/SegmentListPage";
import { SettingsPage } from "../pages/settings/SettingsPage";
import { AccountPage } from "../pages/account/AccountPage";
import { StatisticsPage } from "../pages/statistics/StatisticsPage";

// Cache nur im Arbeitsspeicher, keine Persistenz (RFC-0006, RFC-0011).
const queryClient = new QueryClient({ defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: true } } });

export function App() {
  useApplyTheme();
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <FlashProvider>
          <Header />
          <main className="mx-auto max-w-[1400px] p-4">
            <Routes>
              <Route path="/" element={<DashboardPage />} />
              <Route path="/aktivitaeten" element={<ActivityListPage />} />
              <Route path="/aktivitaeten/:id" element={<ActivityDetailPage />} />
              <Route path="/segmente" element={<SegmentListPage key="segment" kind="segment" />} />
              <Route path="/segmente/:id" element={<SegmentDetailPage />} />
              <Route path="/strecken" element={<SegmentListPage key="route" kind="route" />} />
              <Route path="/strecken/:id" element={<SegmentDetailPage />} />
              <Route path="/statistik" element={<StatisticsPage />} />
              <Route path="/import" element={<ImportPage />} />
              <Route path="/sicherung" element={<Placeholder titleKey="nav.backup" milestone="M5" />} />
              <Route path="/system" element={<SystemPage />} />
              <Route path="/einstellungen" element={<SettingsPage />} />
              <Route path="/einstellungen/:section" element={<SettingsPage />} />
              <Route path="/konto" element={<AccountPage />} />
              <Route path="*" element={<DashboardPage />} />
            </Routes>
          </main>
        </FlashProvider>
      </BrowserRouter>
    </QueryClientProvider>
  );
}
