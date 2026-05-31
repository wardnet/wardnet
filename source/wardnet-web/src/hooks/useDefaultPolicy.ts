import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { systemService } from "../lib/sdk";

/** Read the current global default routing policy. */
export function useDefaultPolicy() {
  return useQuery({
    queryKey: ["system", "default-policy"],
    queryFn: () => systemService.getDefaultPolicy(),
  });
}

/**
 * Update the global default routing policy.
 *
 * `policy` is either `"direct"` or a tunnel UUID. The daemon validates
 * the value and rejects anything else with a 400.
 */
export function useSetDefaultPolicy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (policy: string) => systemService.setDefaultPolicy({ policy }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["system", "default-policy"] });
    },
  });
}
