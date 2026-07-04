import { useQuery } from "@tanstack/react-query";
import { authService } from "../lib/sdk";
import { useAuthStore } from "../stores/authStore";

/** Convenience hook for accessing auth state and actions. */
export function useAuth() {
  return useAuthStore();
}

/** Fetch the authenticated admin's identity (GET /api/users/me). */
export function useMe() {
  return useQuery({
    queryKey: ["users", "me"],
    queryFn: () => authService.me(),
  });
}
