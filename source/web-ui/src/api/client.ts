import type {
  LoginRequest,
  LoginResponse,
  DeviceMeResponse,
  SetMyRuleRequest,
  SetMyRuleResponse,
  SystemStatusResponse,
} from "../types/api";

const BASE = "/api";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...init,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(body.error ?? res.statusText);
  }
  return res.json() as Promise<T>;
}

export function login(body: LoginRequest): Promise<LoginResponse> {
  return request("/auth/login", {
    method: "POST",
    body: JSON.stringify(body),
  });
}

export function getMyDevice(): Promise<DeviceMeResponse> {
  return request("/devices/me");
}

export function setMyRule(body: SetMyRuleRequest): Promise<SetMyRuleResponse> {
  return request("/devices/me/rule", {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export function getSystemStatus(): Promise<SystemStatusResponse> {
  return request("/system/status");
}
