import { useState } from "react";
import { Button } from "@wardnet/forge-web/button";
import { Field } from "@wardnet/forge-web/field";
import { Form, Validator } from "@wardnet/forge-web/form";
import { Input } from "@wardnet/forge-web/input";
import { Textarea } from "@wardnet/forge-web/textarea";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useCreateTunnel } from "@wardnet/wardnet-web";

interface ManualTunnelTabProps {
  onSuccess: () => void;
}

interface ManualTunnelValues extends Record<string, unknown> {
  label: string;
  countryCode: string;
  provider: string;
  config: string;
}

/** Manual WireGuard config paste form for creating a tunnel. */
export function ManualTunnelTab({ onSuccess }: ManualTunnelTabProps) {
  const [label, setLabel] = useState("");
  const [countryCode, setCountryCode] = useState("");
  const [provider, setProvider] = useState("");
  const [config, setConfig] = useState("");
  const createTunnel = useCreateTunnel();

  function reset() {
    setLabel("");
    setCountryCode("");
    setProvider("");
    setConfig("");
  }

  async function handleSubmit(values: ManualTunnelValues) {
    await createTunnel.mutateAsync({
      label: values.label,
      country_code: values.countryCode,
      provider: values.provider || undefined,
      config: values.config,
    });
    reset();
    onSuccess();
  }

  return (
    <Form
      values={{ label, countryCode, provider, config }}
      onSubmit={handleSubmit}
      className="flex flex-col gap-4"
    >
      <div className="grid grid-cols-1 gap-x-6 gap-y-4 md:grid-cols-2">
        <Field label="Label" htmlFor="tunnel-label" name="label">
          <Input
            id="tunnel-label"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="US West"
          />
        </Field>
        <Validator name="label" rule="required" message="Label is required." />

        <Field label="Country code" htmlFor="tunnel-country" name="countryCode">
          <Input
            id="tunnel-country"
            value={countryCode}
            onChange={(e) => setCountryCode(e.target.value)}
            placeholder="US"
            maxLength={2}
          />
        </Field>
        <Validator name="countryCode" rule="required" message="Country code is required." />
        <Validator
          name="countryCode"
          validate={(v) =>
            typeof v === "string" && v.length > 0 && !/^[A-Za-z]{2}$/.test(v)
              ? "Must be a 2-letter ISO country code."
              : null
          }
        />

        <Field label="Provider" htmlFor="tunnel-provider" name="provider" className="md:col-span-2">
          <Input
            id="tunnel-provider"
            value={provider}
            onChange={(e) => setProvider(e.target.value)}
            placeholder="Mullvad"
          />
        </Field>

        <Field
          label="WireGuard config"
          htmlFor="tunnel-config"
          name="config"
          className="md:col-span-2"
        >
          <Textarea
            id="tunnel-config"
            value={config}
            onChange={(e) => setConfig(e.target.value)}
            placeholder="Paste your .conf file contents here…"
            rows={10}
            className="font-mono"
          />
        </Field>
        <Validator name="config" rule="required" message="WireGuard config is required." />
      </div>
      {createTunnel.isError && (
        <ApiErrorAlert error={createTunnel.error} fallback="Failed to create tunnel" />
      )}
      <div className="flex justify-end">
        <Button type="submit" disabled={createTunnel.isPending}>
          {createTunnel.isPending ? "Creating…" : "Create tunnel"}
        </Button>
      </div>
    </Form>
  );
}
