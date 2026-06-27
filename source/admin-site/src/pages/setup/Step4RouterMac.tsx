import { useEffect, useRef, useState } from "react";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { useAdvanceWizard } from "@wardnet/web";
import { useDiscoverGatewayMac } from "@wardnet/web";

const MAC_RE = /^[0-9A-Fa-f]{2}([:-][0-9A-Fa-f]{2}){5}$/;

/**
 * Step 4 — discover the upstream router MAC.
 *
 * Auto-fires the ARP probe on first render. On success the operator
 * just confirms; on failure the manual-entry field surfaces and
 * submits back through the same endpoint with `{mac}` set so the
 * daemon persists either path identically.
 */
export default function Step4RouterMac() {
  const advance = useAdvanceWizard();
  const probe = useDiscoverGatewayMac();
  const [manualMac, setManualMac] = useState("");
  const triedRef = useRef(false);

  // Kick off the probe once on mount. A ref guard (rather than a
  // dependency on `probe`) keeps re-renders from re-firing the probe
  // and stomping on a manual entry the operator may have typed.
  useEffect(() => {
    if (triedRef.current) return;
    triedRef.current = true;
    probe.mutate({});
  }, [probe]);

  const probedMac = probe.data?.mac;
  const probedSource = probe.data?.source;
  const probeFailed = probe.isError;

  async function handleManualSubmit(values: { mac: string }) {
    await probe.mutateAsync({ mac: values.mac });
  }

  async function handleContinue() {
    await advance.mutateAsync({ to_step: "tunnel" });
  }

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h2 className="h-title">Router MAC</h2>
        <p className="h-sub">
          Wardnet uses the upstream router's MAC address for diagnostics and
          packet-capture filtering.
        </p>
      </div>

      {probe.isPending && !probedMac && (
        <Text as="p" size="sm" className="text-ink-3">
          Probing the gateway via ARP…
        </Text>
      )}

      {probedMac && (
        <Text
          as="div"
          size="sm"
          className="rounded-md border border-line bg-sunken p-4"
        >
          <Text as="p" weight="medium" className="text-ink">
            {probedSource === "arp" ? "Discovered via ARP" : "Recorded"}
          </Text>
          <p className="mono mt-1">{probedMac}</p>
        </Text>
      )}

      {!probe.isPending && !probedMac && probeFailed && (
        <Form
          values={{ mac: manualMac }}
          onSubmit={handleManualSubmit}
          className="flex flex-col gap-3"
        >
          <Field
            label="ARP probe failed — enter the router MAC manually"
            htmlFor="router-mac"
            name="mac"
          >
            <Input
              id="router-mac"
              value={manualMac}
              onChange={(e) => setManualMac(e.target.value)}
              placeholder="00:11:22:AA:BB:CC"
              className="font-mono"
            />
          </Field>
          <Validator
            name="mac"
            rule="required"
            message="MAC address is required."
          />
          <Validator
            name="mac"
            validate={(v) =>
              typeof v === "string" && v.length > 0 && !MAC_RE.test(v)
                ? "Must be a valid MAC address (e.g. 00:11:22:AA:BB:CC)."
                : null
            }
          />
          <div className="flex justify-end">
            <Button type="submit" variant="outline">
              Save MAC
            </Button>
          </div>
        </Form>
      )}

      <Button
        onClick={handleContinue}
        disabled={advance.isPending}
        data-testid="setup-router-mac-continue"
        className="w-full"
      >
        {advance.isPending ? "Saving…" : probedMac ? "Continue" : "Skip"}
      </Button>
    </div>
  );
}
