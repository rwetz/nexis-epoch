import { getCurrentWindow } from "@tauri-apps/api/window";
import { IN_TAURI } from "@nexis/design";
import { mountEpoch } from "./embed";
import { standaloneHost } from "./host";

mountEpoch(document.getElementById("root")!, standaloneHost);

// The window starts hidden (tauri.conf.json "visible": false) and is shown
// after React has painted — never a blank or white frame. setTimeout, not
// requestAnimationFrame: rAF does not fire in a hidden window.
if (IN_TAURI) {
  const show = () => getCurrentWindow().show().catch(console.error);
  setTimeout(show, 50);
  setTimeout(show, 500);
}
