import { defineConfig, devices } from "@playwright/test";

// Drives the UI in Chromium against the mock backend (src/dev/mockBackend.ts).
// The real app's backend is covered by `cargo test` with the fake UCI engine.
export default defineConfig({
  testDir: "tests/e2e",
  timeout: 45_000,
  fullyParallel: true,
  // The Vite dev server and the browsers share the machine; more workers
  // than this starves both and makes timings flaky.
  workers: process.env.CI ? 2 : 3,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:1430",
    viewport: { width: 1360, height: 860 },
    colorScheme: "dark",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1360, height: 860 } } }],
  webServer: {
    command: "pnpm exec vite --port 1430 --strictPort",
    url: "http://localhost:1430",
    reuseExistingServer: true,
    env: { VITE_MOCK_TAURI: "1" },
  },
});
