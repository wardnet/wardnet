import { Field, Input, Text } from "@wardnet/web";
import { isReservedName } from "@/lib/suggestName";
import type { Availability, WardnetStep } from "@/lib/wardnet-enrollment";

/** A single selectable provider row (radio + label + description). */
export function ProviderOption({
  label,
  description,
  selected,
  onSelect,
}: {
  label: string;
  description: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-md border border-line p-3 hover:bg-sunken">
      <input
        type="radio"
        name="ddns-provider"
        checked={selected}
        onChange={onSelect}
        className="mt-1"
      />
      <span className="flex flex-col gap-0.5">
        <Text as="span" size="sm" weight="medium" className="text-ink">
          {label}
        </Text>
        <Text as="span" size="xs" className="text-ink-3">
          {description}
        </Text>
      </span>
    </label>
  );
}

/** The Cloudflare BYOD fields (domain + API token). */
export function CloudflareFields({
  domain,
  onDomainChange,
  token,
  onTokenChange,
}: {
  domain: string;
  onDomainChange: (v: string) => void;
  token: string;
  onTokenChange: (v: string) => void;
}) {
  return (
    <div className="flex flex-col gap-4">
      <Field label="Domain" htmlFor="cf-domain" name="cf-domain">
        <Input
          id="cf-domain"
          value={domain}
          onChange={(e) => onDomainChange(e.target.value.toLowerCase())}
          placeholder="home.example.com"
          autoComplete="off"
        />
      </Field>
      <Field label="Cloudflare API token" htmlFor="cf-token" name="cf-token">
        <Input
          id="cf-token"
          type="password"
          value={token}
          onChange={(e) => onTokenChange(e.target.value)}
          placeholder="DNS:Edit token for the zone"
          autoComplete="off"
        />
      </Field>
    </div>
  );
}

/** The wardnet enrollment fields, switching by step (email → code → slug). */
export function WardnetFields({
  step,
  email,
  onEmailChange,
  code,
  onCodeChange,
  slug,
  onSlugChange,
  availability,
  onSuggest,
  onChangeEmail,
}: {
  step: WardnetStep;
  email: string;
  onEmailChange: (v: string) => void;
  code: string;
  onCodeChange: (v: string) => void;
  slug: string;
  onSlugChange: (v: string) => void;
  availability: Availability;
  onSuggest: () => void;
  onChangeEmail: () => void;
}) {
  if (step === "email") {
    return (
      <Field
        label="Wardnet account email"
        htmlFor="wardnet-email"
        name="wardnet-email"
      >
        <Input
          id="wardnet-email"
          type="email"
          value={email}
          onChange={(e) => onEmailChange(e.target.value.trim())}
          placeholder="you@example.com"
          autoComplete="email"
        />
      </Field>
    );
  }

  if (step === "code") {
    return (
      <div className="flex flex-col gap-2">
        <Field
          label="Enrollment code"
          htmlFor="wardnet-code"
          name="wardnet-code"
        >
          <Input
            id="wardnet-code"
            value={code}
            onChange={(e) => onCodeChange(e.target.value.trim())}
            placeholder="Code from your email"
            autoComplete="one-time-code"
          />
        </Field>
        <Text as="div" size="xs" className="flex items-center justify-between">
          <span className="text-ink-3">
            We emailed a one-time code to {email}.
          </span>
          <button
            type="button"
            className="text-accent hover:underline"
            onClick={onChangeEmail}
          >
            Change email
          </button>
        </Text>
      </div>
    );
  }

  // step === "slug"
  return (
    <div className="flex flex-col gap-2">
      <Field label="Hostname" htmlFor="ddns-slug" name="ddns-slug">
        <Input
          id="ddns-slug"
          value={slug}
          onChange={(e) => onSlugChange(e.target.value.toLowerCase())}
          placeholder="happy-einstein"
          autoComplete="off"
        />
      </Field>
      <Text as="div" size="xs" className="flex items-center justify-between">
        <AvailabilityHint availability={availability} slug={slug} />
        <button
          type="button"
          className="text-accent hover:underline"
          onClick={onSuggest}
        >
          Suggest another
        </button>
      </Text>
    </div>
  );
}

/** The live availability feedback line under the slug field. */
export function AvailabilityHint({
  availability,
  slug,
}: {
  availability: Availability;
  slug: string;
}) {
  switch (availability) {
    case "invalid":
      return (
        <span className="text-ink-3">
          {slug.length < 3
            ? "At least 3 characters."
            : isReservedName(slug)
              ? "This name is reserved - try another."
              : "Use lowercase letters, digits, and hyphens only."}
        </span>
      );
    case "checking":
      return <span className="text-ink-3">Checking availability…</span>;
    case "available":
      return <span className="text-ink-2">✓ {slug} is available</span>;
    case "taken":
      return <span className="text-danger">{slug} is taken - try another</span>;
    case "error":
      return (
        <span className="text-ink-3">
          Couldn't check availability - you can still continue or try again.
        </span>
      );
    default:
      return <span className="text-ink-3">&nbsp;</span>;
  }
}
