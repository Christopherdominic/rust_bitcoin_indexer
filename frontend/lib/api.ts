import type {
  AddressOutput,
  AddressSummary,
  BlockDetail,
  BlockSummary,
  IndexerStatus,
  SearchResult,
  TransactionDetail,
  Utxo,
  WatchActivity,
  WatchedAddress,
} from "./types";

export const API_URL = process.env.NEXT_PUBLIC_API_URL?.replace(/\/+$/, "");

export type ApiErrorKind =
  | "config"
  | "unreachable"
  | "not_found"
  | "http"
  | "malformed";

export class ApiError extends Error {
  constructor(
    readonly kind: ApiErrorKind,
    message: string,
    readonly status?: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export function isApiError(error: unknown, kind?: ApiErrorKind): error is ApiError {
  return error instanceof ApiError && (kind === undefined || error.kind === kind);
}

type Guard = (value: unknown) => boolean;

const isObject: Guard = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
const isArray: Guard = (v) => Array.isArray(v);

async function api<T>(path: string, guard: Guard | null, init?: RequestInit): Promise<T> {
  if (!API_URL) {
    throw new ApiError("config", "NEXT_PUBLIC_API_URL is not configured");
  }

  let response: Response;
  try {
    response = await fetch(`${API_URL}${path}`, { ...init, cache: "no-store" });
  } catch {
    throw new ApiError("unreachable", `Unable to reach the indexer API at ${API_URL}`);
  }

  if (response.status === 404) {
    throw new ApiError("not_found", "Resource not found", 404);
  }
  if (!response.ok) {
    throw new ApiError("http", `API request failed with status ${response.status}`, response.status);
  }
  if (response.status === 204 || guard === null) {
    return undefined as T;
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    throw new ApiError("malformed", `Invalid JSON returned by ${path}`, response.status);
  }
  if (!guard(body)) {
    throw new ApiError("malformed", `Unexpected response shape from ${path}`, response.status);
  }
  return body as T;
}

const seg = encodeURIComponent;

// ---------------------------------------------------------------- indexer

export const getStatus = () => api<IndexerStatus>("/api/status", isObject);

// ---------------------------------------------------------------- blocks

export const getBlocks = (limit = 20, offset = 0) =>
  api<BlockSummary[]>(`/api/blocks?limit=${limit}&offset=${offset}`, isArray);

export const getBlock = (height: number) =>
  api<BlockDetail>(`/api/blocks/${height}`, isObject);

export const getBlockByHash = (hash: string) =>
  api<BlockDetail>(`/api/blocks/hash/${seg(hash)}`, isObject);

// ---------------------------------------------------------------- transactions

export const getTransaction = (txid: string) =>
  api<TransactionDetail>(`/api/transactions/${seg(txid)}`, isObject);

export const getUtxos = () => api<Utxo[]>("/api/utxos", isArray);

// ---------------------------------------------------------------- addresses

export const getAddress = (address: string) =>
  api<AddressSummary>(`/api/addresses/${seg(address)}`, isObject);

export const getAddressUtxos = (address: string) =>
  api<Utxo[]>(`/api/addresses/${seg(address)}/utxos`, isArray);

export const getAddressTransactions = (address: string) =>
  api<AddressOutput[]>(`/api/addresses/${seg(address)}/transactions`, isArray);

// ---------------------------------------------------------------- search

export const search = (query: string) =>
  api<SearchResult>(`/api/search/${seg(query)}`, isObject);

// ---------------------------------------------------------------- watch list

export const getWatchedAddresses = () => api<WatchedAddress[]>("/api/watch", isArray);

export const watchAddress = (address: string) =>
  api<WatchedAddress>(`/api/watch/${seg(address)}`, isObject, { method: "POST" });

export const getWatchActivity = (address: string) =>
  api<WatchActivity>(`/api/watch/${seg(address)}`, isObject);

export const unwatchAddress = (address: string) =>
  api<void>(`/api/watch/${seg(address)}`, null, { method: "DELETE" });
