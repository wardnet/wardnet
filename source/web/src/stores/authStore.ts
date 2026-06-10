import { create } from "zustand";
import { WardnetApiError } from "@wardnet/js";
import { authService, systemService } from "../lib/sdk";

interface AuthState {
  isAdmin: boolean;
  isChecking: boolean;
  login: (
    username: string,
    password: string,
    rememberMe?: boolean,
  ) => Promise<void>;
  logout: () => void;
  checkAuth: () => Promise<void>;
  refresh: () => Promise<void>;
}

/**
 * Authentication state store.
 *
 * Tracks whether the current user has an admin session. Uses the system
 * status endpoint (admin-only) as a session probe on startup.
 */
export const useAuthStore = create<AuthState>((set) => ({
  isAdmin: false,
  isChecking: true,

  login: async (username, password, rememberMe = false) => {
    await authService.login({ username, password, rememberMe });
    set({ isAdmin: true });
  },

  logout: () => {
    set({ isAdmin: false });
  },

  checkAuth: async () => {
    try {
      await systemService.getStatus();
      set({ isAdmin: true, isChecking: false });
    } catch (e) {
      if (e instanceof WardnetApiError && e.status === 401) {
        set({ isAdmin: false, isChecking: false });
      } else {
        // Network error or daemon not running — treat as not admin.
        set({ isAdmin: false, isChecking: false });
      }
    }
  },

  refresh: async () => {
    try {
      await authService.refresh();
    } catch {
      // Refresh failure is non-fatal: session may already be valid,
      // or the user will be redirected to login by checkAuth.
    }
  },
}));
