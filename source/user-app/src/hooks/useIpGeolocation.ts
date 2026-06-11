import { useQuery } from "@tanstack/react-query";

export interface IpGeolocationResult {
  ip: string;
  country_code: string;
  country_name: string;
  city: string;
  org: string;
  latitude: number;
  longitude: number;
}

async function fetchIpGeolocation(): Promise<IpGeolocationResult> {
  const res = await fetch("https://ipapi.co/json/");
  if (!res.ok) throw new Error(`Geolocation fetch failed: ${res.status}`);
  return res.json();
}

export function useIpGeolocation() {
  return useQuery({
    queryKey: ["ip-geolocation"],
    queryFn: fetchIpGeolocation,
    staleTime: 5 * 60 * 1_000,
    retry: 1,
  });
}
