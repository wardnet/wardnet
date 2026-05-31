import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import type { AdvanceWizardRequest } from "@wardnet/js";
import { setupService } from "../lib/sdk";

/** Check the current wizard state. */
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

/** Advance the wizard to a new step (and optionally record a mode). */
export function useAdvanceWizard() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (body: AdvanceWizardRequest) => setupService.advance(body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["setup", "status"] });
    },
  });
}
