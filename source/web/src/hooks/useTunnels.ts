import { useEffect, useState } from "react";
import {
  useQuery,
  useQueries,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import type {
  CreateTunnelRequest,
  Job,
  TunnelSpeedTestResult,
} from "@wardnet/js";
import { isJobTerminal } from "@wardnet/js";
import { jobsService, tunnelService } from "../lib/sdk";

export function useTunnels() {
  return useQuery({
    queryKey: ["tunnels"],
    queryFn: () => tunnelService.list(),
    refetchInterval: 15_000,
  });
}

export function useTunnel(id: string) {
  return useQuery({
    queryKey: ["tunnels", id],
    queryFn: () => tunnelService.getById(id),
    enabled: !!id,
    refetchInterval: 15_000,
  });
}

export function useTunnelDevices(id: string) {
  return useQuery({
    queryKey: ["tunnels", id, "devices"],
    queryFn: () => tunnelService.listDevices(id),
    enabled: !!id,
    refetchInterval: 30_000,
  });
}

export function useCreateTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTunnelRequest) => tunnelService.create(body),
    onSuccess: (data) => {
      toast.success(data.message || "Tunnel created");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to create tunnel"),
  });
}

export function useDeleteTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => tunnelService.delete(id),
    onSuccess: (data) => {
      toast.success(data.message || "Tunnel deleted");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to delete tunnel"),
  });
}

export function useTestTunnel() {
  return useMutation({
    mutationFn: (id: string) => tunnelService.test(id),
    onError: (error) => {
      const message =
        error instanceof Error ? error.message : "Tunnel test failed";
      // 409 surfaces as "conflict" / "test already in progress" — show a
      // dedicated toast so a rapid double-click is obviously rate-limited
      // rather than a generic failure.
      if (/already|conflict/i.test(message)) {
        toast.error("Test already in progress");
      } else {
        toast.error(message);
      }
    },
  });
}

export function useRebuildTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => tunnelService.rebuild(id),
    onSuccess: () => {
      toast.success("Tunnel rebuild initiated");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to rebuild tunnel"),
  });
}

export function useSetTunnelDnsOverride() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, value }: { id: string; value: boolean }) =>
      tunnelService.setDnsOverride(id, { override_default_dns: value }),
    onSuccess: (_data, vars) => {
      toast.success(
        vars.value
          ? "Tunneled-device DNS will route through wardnet (filtered)"
          : "Tunneled-device DNS now uses the system upstream pool",
      );
      qc.invalidateQueries({ queryKey: ["tunnels"] });
      qc.invalidateQueries({ queryKey: ["tunnels", vars.id] });
    },
    onError: () => toast.error("Failed to update DNS override"),
  });
}

/** Shared query config for one tunnel's speed-test history, so the single
 *  (`useSpeedTestResults`) and list (`useSpeedTestResultsList`) hooks read the
 *  same cache entry and can't drift on key or fetch shape. */
function speedTestResultsQuery(id: string) {
  return {
    queryKey: ["tunnels", id, "speed-test"],
    queryFn: () => tunnelService.getSpeedTestResults(id),
  };
}

/** Recent speed test results for a tunnel (newest first). */
export function useSpeedTestResults(id: string, enabled = true) {
  return useQuery({ ...speedTestResultsQuery(id), enabled: !!id && enabled });
}

/**
 * Latest speed-test result for each tunnel in `ids`, keyed by tunnel id.
 *
 * The tunnels grid shows an inline speed-test comparison on every card the user
 * has run a test on, so the owning page needs the newest result for a set of
 * tunnels whose size changes as more cards are tested — a fixed number of
 * `useSpeedTestResults` calls can't cover that. `useQueries` fans the same
 * per-tunnel query out across `ids`; each entry maps to `results[0]` (or `null`
 * before any result lands), mirroring how `useSpeedTestResults` is read. Pass
 * only ids that still exist so a removed tunnel's query drops out instead of
 * refetching against a deleted tunnel.
 */
export function useSpeedTestResultsList(
  ids: string[],
): Record<string, TunnelSpeedTestResult | null> {
  const results = useQueries({
    queries: ids.map((id) => ({ ...speedTestResultsQuery(id), enabled: !!id })),
  });
  const latest: Record<string, TunnelSpeedTestResult | null> = {};
  ids.forEach((id, i) => {
    // eslint-disable-next-line security/detect-object-injection -- i is the map index into useQueries' output (ids order); id is the tunnel key being written
    latest[id] = results[i].data?.results[0] ?? null;
  });
  return latest;
}

/**
 * Start a speed test for a tunnel and track the background job to completion.
 *
 * The server dispatches a job and returns immediately with its id; this hook
 * polls the job (exposing `percentage` for an inline progress bar) and, on
 * success, invalidates the tunnel's speed-test history so the new result
 * renders. The owning page holds a single instance and passes `start` plus
 * `activeTunnelId`/`percentage` down to the cards, so one in-flight run is
 * tracked across the whole list. A 409 (run already in progress) surfaces as a
 * dedicated toast.
 *
 * Mirrors the job-polling shape of `useRefreshBlocklist`; kept inline rather
 * than extracting a shared poller so the existing blocklist hook is untouched.
 */
export function useStartSpeedTest() {
  const qc = useQueryClient();
  const [active, setActive] = useState<{
    jobId: string;
    tunnelId: string;
  } | null>(null);

  const dispatch = useMutation({
    mutationFn: async (tunnelId: string) => {
      const res = await tunnelService.startSpeedTest(tunnelId);
      return { tunnelId, jobId: res.job_id };
    },
    onSuccess: ({ tunnelId, jobId }) => setActive({ jobId, tunnelId }),
    onError: (error) => {
      const message =
        error instanceof Error ? error.message : "Speed test failed";
      if (/already|conflict/i.test(message)) {
        toast.error("Speed test already in progress");
      } else {
        toast.error(message);
      }
    },
  });

  const jobQuery = useQuery<Job>({
    queryKey: ["job", active?.jobId],
    queryFn: () => jobsService.get(active!.jobId),
    enabled: !!active,
    refetchInterval: (q) => {
      const s = q.state.data?.status;
      return s && isJobTerminal(s) ? false : 1000;
    },
  });

  useEffect(() => {
    const job = jobQuery.data;
    if (!job || !active) return;
    if (job.status === "SUCCEED") {
      qc.invalidateQueries({
        queryKey: ["tunnels", active.tunnelId, "speed-test"],
      });
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActive(null);
    } else if (job.status === "TERMINATED_WITH_ERRORS") {
      toast.error(job.error || "Speed test failed");
      setActive(null);
    }
  }, [jobQuery.data, active, qc]);

  return {
    start: dispatch.mutate,
    activeTunnelId: active?.tunnelId ?? null,
    percentage: jobQuery.data?.percentage_done ?? 0,
    isRunning: dispatch.isPending || !!active,
  };
}
