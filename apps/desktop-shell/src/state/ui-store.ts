import { create } from "zustand";

export interface UiState {
  sidebarOpen: boolean;
  sidebarWidth: number;
  commandPaletteOpen: boolean;
  toggleSidebar: () => void;
  setSidebarOpen: (open: boolean) => void;
  setSidebarWidth: (width: number) => void;
  setCommandPaletteOpen: (open: boolean) => void;
}

export const initialState = {
  sidebarOpen: true,
  sidebarWidth: 240,
  commandPaletteOpen: false,
} satisfies Pick<
  UiState,
  "sidebarOpen" | "sidebarWidth" | "commandPaletteOpen"
>;

export const useUiStore = create<UiState>((set) => ({
  ...initialState,
  toggleSidebar: () =>
    set((state) => ({
      sidebarOpen: !state.sidebarOpen,
    })),
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  setSidebarWidth: (sidebarWidth) => set({ sidebarWidth }),
  setCommandPaletteOpen: (commandPaletteOpen) =>
    set({ commandPaletteOpen }),
}));
