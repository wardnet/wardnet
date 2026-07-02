import type { WardnetClient } from "../client.js";
import type {
  CreateZoneExceptionRequest,
  CreateZoneExceptionResponse,
  DeleteZoneExceptionResponse,
  GetZoneExceptionResponse,
  ListZoneExceptionsResponse,
  UpdateZoneExceptionRequest,
  UpdateZoneExceptionResponse,
} from "../types/api.js";

/**
 * Cross-zone exception management (epic #244, issue #737).
 *
 * Exceptions re-open a specific flow across the cross-subnet default-deny — e.g.
 * a phone casting to a TV in another zone. All operations are admin-only.
 */
export class ZoneExceptionsService {
  constructor(private readonly client: WardnetClient) {}

  /** List all cross-zone exceptions. */
  async list(): Promise<ListZoneExceptionsResponse> {
    return this.client.request<ListZoneExceptionsResponse>("/network/zones/exceptions");
  }

  /** Fetch a single exception by id. */
  async getById(id: string): Promise<GetZoneExceptionResponse> {
    return this.client.request<GetZoneExceptionResponse>(`/network/zones/exceptions/${id}`);
  }

  /** Create a new exception. */
  async create(body: CreateZoneExceptionRequest): Promise<CreateZoneExceptionResponse> {
    return this.client.request<CreateZoneExceptionResponse>("/network/zones/exceptions", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Partially update an exception. */
  async update(id: string, body: UpdateZoneExceptionRequest): Promise<UpdateZoneExceptionResponse> {
    return this.client.request<UpdateZoneExceptionResponse>(`/network/zones/exceptions/${id}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete an exception (revokes the allowance live). */
  async delete(id: string): Promise<DeleteZoneExceptionResponse> {
    return this.client.request<DeleteZoneExceptionResponse>(`/network/zones/exceptions/${id}`, {
      method: "DELETE",
    });
  }
}
