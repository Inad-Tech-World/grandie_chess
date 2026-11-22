import React from "react";
import ReactDOM from "react-dom/client";
import "@mantine/core/styles.css";
import "@mantine/notifications/styles.css";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.brown.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";
import "./styles/global.css";
import "./styles/boards.css";
import "./styles/pieces.css";
import App from "./App";
import { ErrorBoundary, logUiError } from "./layout/ErrorBoundary";
import { isMac } from "./lib/platform";

// Lets the stylesheets adjust for the window frame of each platform.
document.documentElement.dataset.platform = isMac ? "mac" : "windows";

async function boot() {
  // Browser previews and Playwright tests run against a mock backend.
  if (import.meta.env.VITE_MOCK_TAURI === "1") {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
  }
  // Errors no screen caught still go to the error log.
  window.addEventListener("error", (e) => logUiError("window", e.error ?? e.message));
  window.addEventListener("unhandledrejection", (e) => logUiError("unhandled promise", e.reason));
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <ErrorBoundary name="app" plain>
        <App />
      </ErrorBoundary>
    </React.StrictMode>,
  );
}

void boot();
