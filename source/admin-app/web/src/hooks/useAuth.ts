import { useAuthStore } from "@/stores/authStore";

/** Convenience hook for accessing auth state and actions. */
export function useAuth() {
  return useAuthStore();
}
