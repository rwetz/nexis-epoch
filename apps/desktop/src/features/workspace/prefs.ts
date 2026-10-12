import { create } from "zustand";

type Navigation = "focus" | "free";
const KEY = "nexis-epoch:workspace-navigation";

function read(): Navigation {
  try {
    return localStorage.getItem(KEY) === "free" ? "free" : "focus";
  } catch {
    return "focus";
  }
}

/**
 * Per-device workspace preferences. `focus`: double-click a tab bar to zoom
 * a panel. `free`: also pinch and Ctrl/Cmd+Alt gestures to zoom through
 * nested groups.
 */
export const useWorkspacePrefs = create<{ navigation: Navigation; setNavigation(n: Navigation): void }>((set) => ({
  navigation: read(),
  setNavigation(navigation) {
    try {
      localStorage.setItem(KEY, navigation);
    } catch {
      // per-session only
    }
    set({ navigation });
  },
}));
