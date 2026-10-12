import "./styles.css";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { domAnimation, LazyMotion } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { IN_TAURI, ThemeProvider, Toaster, TooltipProvider, USE_CUSTOM_WINDOW_CONTROLS } from "@nexis/design";
import App from "./App";

// Windows/Linux: the OS hands us a transparent, undecorated window and the
// stylesheet paints 12px corners onto #root under this flag.
if (USE_CUSTOM_WINDOW_CONTROLS) document.documentElement.dataset.chrome = "borderless";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <LazyMotion features={domAnimation} strict>
      <ThemeProvider storageKey="nexis-epoch">
        <TooltipProvider delayDuration={300}>
          <App />
          <Toaster position="bottom-right" />
        </TooltipProvider>
      </ThemeProvider>
    </LazyMotion>
  </StrictMode>,
);

// The window starts hidden (tauri.conf.json "visible": false) and is shown
// after React has painted — never a blank or white frame. setTimeout, not
// requestAnimationFrame: rAF does not fire in a hidden window.
if (IN_TAURI) {
  const show = () => getCurrentWindow().show().catch(console.error);
  setTimeout(show, 50);
  setTimeout(show, 500);
}
