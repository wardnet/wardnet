/** Supported authentication methods for a VPN provider. */
export type ProviderAuthMethod = "credentials" | "token";

/** Metadata about a registered VPN provider. */
export interface ProviderInfo {
  id: string;
  name: string;
  auth_methods: ProviderAuthMethod[];
  icon_url?: string;
  website_url?: string;
  credentials_hint?: string;
}

/** Credentials submitted by the admin for provider operations. */
export type ProviderCredentials =
  | { type: "credentials"; username: string; password: string }
  | { type: "token"; token: string };

/** Filters for server listing. */
export interface ServerFilter {
  country?: string;
  max_load?: number;
}

/** A country available from a VPN provider. */
export interface CountryInfo {
  code: string;
  name: string;
}

/** Information about a single VPN server. */
export interface ServerInfo {
  id: string;
  name: string;
  country_code: string;
  city?: string;
  hostname: string;
  load: number;
}
