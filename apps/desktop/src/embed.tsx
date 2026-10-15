/**
 * Epoch's whole UI, mountable by any host: the standalone app (main.tsx) and
 * the Epoch window inside Nexis, which imports this file into its own bundle
 * (Nexis `src/windows/epoch.tsx`). Both get the same shell, panels and
 * providers; only the `EpochHost` differs (host.ts).
 */
import "./styles.css";
import { domAnimation, LazyMotion } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider, Toaster, TooltipProvider, USE_CUSTOM_WINDOW_CONTROLS } from "@nexis/design";
import App from "./App";
import { type EpochHost, setHost } from "./host";

export type { EpochHost, EpochLink, NexisOpenArgs } from "./host";

export function mountEpoch(element: HTMLElement, host: EpochHost): void {
  setHost(host);
  // Windows/Linux: the OS hands us a transparent, undecorated window and the
  // stylesheet paints 12px corners onto #root under this flag. Nexis's tool
  // windows are made the same way.
  if (USE_CUSTOM_WINDOW_CONTROLS) document.documentElement.dataset.chrome = "borderless";

  createRoot(element).render(
    <StrictMode>
      <LazyMotion features={domAnimation} strict>
        {/* Same storage key in both hosts: the theme follows the person,
            not the window Epoch happens to be in. */}
        <ThemeProvider storageKey="nexis-epoch">
          <TooltipProvider delayDuration={300}>
            <App />
            <Toaster position="bottom-right" />
          </TooltipProvider>
        </ThemeProvider>
      </LazyMotion>
    </StrictMode>,
  );
}
