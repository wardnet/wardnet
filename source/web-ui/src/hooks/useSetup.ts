import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { setupService } from "@/lib/sdk";

/** Check if initial setup has been completed. */
export function useSetupStatus() {
  return useQuery({
    queryKey: ["setup", "status"],
    queryFn: () => setupService.getStatus(),
  });
}

/** Create the first admin account. */
export function useSetup() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: { username: string; password: string }) => setupService.setup(body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["setup", "status"] });
    },
  });
}
