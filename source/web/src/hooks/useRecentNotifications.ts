import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import type { NotificationItem } from "@wardnet/js";
import { pushService } from "../lib/sdk";
import { useAuth } from "./useAuth";

/**
 * The admin notification feed (issue #482): recently pushed admin-audience
 * notifications, newest first. Admin-gated server-side, so the query only
 * runs with an admin session. Low-urgency — 30s poll plus the default
 * refetch-on-focus (which fires right after a notification click opens the
 * app).
 */
export function useRecentNotifications(limit = 50) {
  const { isAdmin } = useAuth();
  return useQuery<NotificationItem[]>({
    queryKey: ["notifications", "recent"],
    queryFn: () => pushService.listNotifications(limit),
    enabled: isAdmin,
    refetchInterval: 30_000,
  });
}

/**
 * Clear the admin notification feed. The feed is shared across admin
 * accounts, so this clears it for every admin.
 */
export function useClearNotifications() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => pushService.clearNotifications(),
    onSuccess: () => {
      toast.success("Notifications cleared");
      qc.invalidateQueries({ queryKey: ["notifications"] });
    },
    onError: () => toast.error("Failed to clear notifications"),
  });
}
