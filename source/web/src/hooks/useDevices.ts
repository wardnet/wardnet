import { useRef } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import type {
  RoutingTarget,
  UpdateDeviceRequest,
  DnsCaptureSettingsRequest,
} from "@wardnet/js";
import { deviceService } from "../lib/sdk";

/** HTTP status carried by a `WardnetApiError`, if the rejection has one. */
function httpStatus(err: unknown): number | undefined {
  return (err as { status?: number } | null)?.status;
}

export function useDevices() {
  return useQuery({
    queryKey: ["devices"],
    queryFn: () => deviceService.list(),
    refetchInterval: 10_000,
  });
}

export function useDevice(id: string) {
  return useQuery({
    queryKey: ["devices", id],
    queryFn: () => deviceService.getById(id),
    enabled: !!id,
  });
}

export function useMyDevice() {
  return useQuery({
    queryKey: ["devices", "me"],
    queryFn: () => deviceService.getMe(),
    refetchInterval: 10_000,
  });
}

export function useSetMyRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (target: RoutingTarget) => deviceService.setMyRule(target),
    onSuccess: () => {
      toast.success("Routing updated");
      qc.invalidateQueries({ queryKey: ["devices", "me"] });
    },
    onError: () => toast.error("Failed to update routing"),
  });
}

/**
 * Self-service DNS capture toggle for the calling device. Flips only the
 * `enabled` flag; retention caps stay admin-only. Refreshes `["devices","me"]`
 * so the toggle state stays in sync with the device record.
 */
export function useSetMyCaptureEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) =>
      deviceService.setMyCaptureEnabled(enabled),
    onSuccess: (_data, enabled) => {
      toast.success(enabled ? "DNS capture enabled" : "DNS capture disabled");
      qc.invalidateQueries({ queryKey: ["devices", "me"] });
    },
    onError: () => toast.error("Failed to update DNS capture"),
  });
}

export function useUpdateDevice(options?: { successMessage?: string }) {
  const qc = useQueryClient();
  const optionsRef = useRef(options);
  // Latest ref: capture the freshest options in render so the async
  // onSuccess callback below reads the current successMessage.
  // eslint-disable-next-line react-hooks/refs
  optionsRef.current = options;
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateDeviceRequest }) =>
      deviceService.update(id, body),
    onSuccess: (_, variables) => {
      toast.success(optionsRef.current?.successMessage ?? "Device updated");
      qc.invalidateQueries({ queryKey: ["devices"], exact: true });
      qc.invalidateQueries({
        queryKey: ["devices", variables.id],
        exact: true,
      });
    },
    onError: () => toast.error("Failed to update device"),
  });
}

/**
 * Admin-triggered identification probe for one device (issue #1116).
 *
 * The success toast reports what the probe found, including the empty case:
 * "no known ports answered" is a real result, and a silent success would make
 * a probe that found nothing look like a broken button.
 */
export function useIdentifyDevice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deviceService.identify(id),
    onSuccess: (data, id) => {
      const answered = data.answering_ports.length;
      toast.success(
        answered === 0
          ? `No known ports answered (tried ${data.ports_probed.length})`
          : `Identified: ${answered === 1 ? "1 port" : `${answered} ports`} answered`,
      );
      qc.invalidateQueries({ queryKey: ["devices"], exact: true });
      qc.invalidateQueries({ queryKey: ["devices", id], exact: true });
    },
    // A 409 is the daemon refusing because the device left the network. It is
    // reachable in normal use — the button's own enabled/disabled state is
    // computed from `last_seen` at render time and the device detail query does
    // not poll — so name the actual reason rather than reporting a bare
    // failure for something the admin can act on by waking the device.
    onError: (error) =>
      toast.error(
        httpStatus(error) === 409
          ? "This device is no longer on the network, so Wardnet did not contact it"
          : "Failed to identify device",
      ),
  });
}

export function useDnsCaptureSettings(id: string) {
  return useQuery({
    queryKey: ["devices", id, "dns-capture"],
    queryFn: () => deviceService.getDnsCaptureSettings(id),
  });
}

export function useUpdateDnsCaptureSettings(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: DnsCaptureSettingsRequest) =>
      deviceService.updateDnsCaptureSettings(id, body),
    onSuccess: () => {
      toast.success("DNS capture settings saved");
      qc.invalidateQueries({ queryKey: ["devices", id, "dns-capture"] });
    },
    onError: () => toast.error("Failed to save DNS capture settings"),
  });
}
