import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  /** List all cross-zone exceptions. */
  async list(): Promise<ListZoneExceptionsResponse> {
    return this.api.get("/network/zones/exceptions");
  }

  /** Fetch a single exception by id. */
  async getById(id: string): Promise<GetZoneExceptionResponse> {
    return this.api.get("/network/zones/exceptions/{id}", { path: { id } });
  }

  /** Create a new exception. */
  async create(body: CreateZoneExceptionRequest): Promise<CreateZoneExceptionResponse> {
    return this.api.post("/network/zones/exceptions", { body });
  }

  /** Partially update an exception. */
  async update(id: string, body: UpdateZoneExceptionRequest): Promise<UpdateZoneExceptionResponse> {
    return this.api.put("/network/zones/exceptions/{id}", { path: { id }, body });
  }

  /** Delete an exception (revokes the allowance live). */
  async delete(id: string): Promise<DeleteZoneExceptionResponse> {
    return this.api.del("/network/zones/exceptions/{id}", { path: { id } });
  }
}
