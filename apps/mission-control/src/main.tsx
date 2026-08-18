import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { AppErrorBoundary } from "./ui/AppErrorBoundary";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppErrorBoundary
      scope="global"
      title="Mission Control hit a global runtime error."
      subtitle="Use retry or reload to recover the app shell."
    >
      <App />
    </AppErrorBoundary>
  </StrictMode>
);
