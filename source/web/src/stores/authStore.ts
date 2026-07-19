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
  /**
   * Sign out. Clears local auth state synchronously, then revokes the
   * session server-side. Resolves `true` when the daemon confirmed the
   * revocation, `false` when the network call failed (the HttpOnly cookie
   * may still be alive — callers can warn the user).
   */
  logout: () => Promise<boolean>;
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

  logout: async () => {
    // Sign out locally before the network round-trip: the UI reacts
    // instantly, and a slow request that settles after a subsequent re-login
    // can no longer clobber the fresh session's state.
    set({ isAdmin: false });
    try {
      // Revoke the session server-side and clear the HttpOnly cookie — the
      // cookie is unreachable from JS, so only the daemon can drop it.
      await authService.logout();
      return true;
    } catch (e) {
      // The UI is already signed out locally, but the server session may
      // have survived — surface that instead of failing silently, since a
      // live cookie on a shared device re-authenticates the next user.
      console.warn("logout: server-side session revocation failed", e);
      return false;
    }
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
