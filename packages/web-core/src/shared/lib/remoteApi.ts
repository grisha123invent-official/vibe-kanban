import type {
  AttachmentUrlResponse,
  AttachmentWithBlob,
  CommitAttachmentsRequest,
  CommitAttachmentsResponse,
  ConfirmUploadRequest,
  InitUploadRequest,
  InitUploadResponse,
  ListRelayHostsResponse,
  RelayHost,
  UpdateIssueRequest,
  UpdateProjectRequest,
  UpdateProjectStatusRequest,
} from 'shared/remote-types';
import { getAuthRuntime } from '@/shared/lib/auth/runtime';
import { syncRelayApiBaseWithRemote } from '@/shared/lib/relayBackendApi';

const BUILD_TIME_API_BASE = import.meta.env.VITE_VK_SHARED_API_BASE || '';

// Mutable module-level variable — overridden at runtime by ConfigProvider
// when VK_SHARED_API_BASE is set (for self-hosting support)
let _remoteApiBase: string = BUILD_TIME_API_BASE;

/**
 * Set the remote API base URL at runtime.
 * Called by ConfigProvider when /api/info returns a shared_api_base value.
 * No-op if base is null/undefined/empty (preserves build-time fallback).
 */
export function setRemoteApiBase(base: string | null | undefined) {
  _remoteApiBase = base || BUILD_TIME_API_BASE;
  if (_remoteApiBase) {
    syncRelayApiBaseWithRemote(_remoteApiBase);
  }
}

/**
 * Get the current remote API base URL.
 * Returns the runtime value if set by ConfigProvider, otherwise the build-time default.
 */
export function getRemoteApiUrl(): string {
  return _remoteApiBase;
}

// Backward-compatible export — consumers should migrate to getRemoteApiUrl()
export const REMOTE_API_URL = BUILD_TIME_API_BASE;

// ─── Local-first helpers ──────────────────────────────────────────────────────

/**
 * Returns the active project path stored in localStorage.
 * This is the absolute directory path (e.g. /Users/.../my-app) that serves
 * as the project_id for local-first mode.
 */
export function getActiveProjectPath(): string | null {
  return localStorage.getItem('vk_active_project_path');
}

/**
 * Persist the active project path. All subsequent offline writes use this
 * path as the project_id instead of a generated UUID.
 */
export function setActiveProjectPath(absolutePath: string): void {
  localStorage.setItem('vk_active_project_path', absolutePath);
}

/**
 * Returns a deterministic localStorage key for tasks belonging to a project.
 * Key format: `vk_offline_tasks__<base64url(path)>`
 * Falls back to `vk_offline_tasks` for the legacy single-project scenario.
 */
function tasksKeyForProject(projectId: string): string {
  // Base64-URL encode to avoid problematic characters in localStorage keys
  const encoded = btoa(projectId)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');
  return `vk_offline_tasks__${encoded}`;
}

/**
 * Returns all registered project paths.
 * Stored as a JSON string array under `vk_registered_projects`.
 */
function getRegisteredProjects(): string[] {
  try {
    return JSON.parse(
      localStorage.getItem('vk_registered_projects') || '[]'
    ) as string[];
  } catch {
    return [];
  }
}

function ensureProjectRegistered(projectPath: string): void {
  const projects = getRegisteredProjects();
  if (!projects.includes(projectPath)) {
    projects.push(projectPath);
    localStorage.setItem('vk_registered_projects', JSON.stringify(projects));
  }
}

// ─── Tasks record type (minimal, for aggregation) ─────────────────────────────

interface OfflineTask {
  id: string;
  project_id: string;
  [key: string]: unknown;
}

// ─── Global-board aggregation ─────────────────────────────────────────────────

/**
 * Returns ALL tasks from ALL registered project buckets.
 * Used by GET /v1/tasks when no project_id filter is provided (Global Board).
 */
function getAllTasksGlobal(): OfflineTask[] {
  const projects = getRegisteredProjects();
  const allTasks: OfflineTask[] = [];

  for (const projectPath of projects) {
    const key = tasksKeyForProject(projectPath);
    try {
      const tasks = JSON.parse(
        localStorage.getItem(key) || '[]'
      ) as OfflineTask[];
      allTasks.push(...tasks);
    } catch {
      // Skip corrupt buckets
    }
  }

  // Also include legacy single-bucket tasks
  try {
    const legacy = JSON.parse(
      localStorage.getItem('vk_offline_tasks') || '[]'
    ) as OfflineTask[];
    for (const t of legacy) {
      if (!allTasks.some((existing) => existing.id === t.id)) {
        allTasks.push(t);
      }
    }
  } catch {
    // ignore
  }

  return allTasks;
}

// ─── Offline mock backend ─────────────────────────────────────────────────────

// === OFFLINE MOCK BACKEND ===
function handleLocalOfflineRequest(
  path: string,
  options: RequestInit
): Response {
  const url = new URL(path, 'http://localhost');
  const pathname = url.pathname; // e.g., /v1/fallback/projects or /v1/projects
  const method = (options.method || 'GET').toUpperCase();

  let table = '';
  const fallbackMatch = pathname.match(/^\/v1\/fallback\/([a-zA-Z0-9_]+)/);
  const crudMatch = pathname.match(/^\/v1\/([a-zA-Z0-9_]+)(\/.*)?/);

  if (fallbackMatch && method === 'GET') {
    table = fallbackMatch[1];
    const data = JSON.parse(
      localStorage.getItem(`vk_offline_${table}`) || '[]'
    );
    return new Response(JSON.stringify({ [table]: data }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  } else if (crudMatch) {
    table = crudMatch[1];
    const recordId = crudMatch[2] ? crudMatch[2].slice(1) : undefined;

    // ─── Special handling: projects ────────────────────────────────────────────
    if (table === 'projects') {
      let projects: any[] = JSON.parse(
        localStorage.getItem('vk_offline_projects') || '[]'
      );

      if (method === 'GET') {
        return new Response(JSON.stringify({ projects }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }

      if (method === 'POST') {
        const payload = JSON.parse(options.body as string);
        // 🔑 project_id = absolute path from localStorage (NOT a UUID)
        const activePath = getActiveProjectPath();
        if (!payload.id && activePath) {
          payload.id = activePath;
        } else if (!payload.id) {
          // Fallback only if no path stored yet
          payload.id = crypto.randomUUID();
        }
        // Register the path so Global Board can find its tasks
        ensureProjectRegistered(payload.id);
        projects.push(payload);
        localStorage.setItem('vk_offline_projects', JSON.stringify(projects));
        return new Response(
          JSON.stringify({ txid: Date.now(), id: payload.id }),
          { status: 200 }
        );
      }

      if (method === 'PATCH' && recordId) {
        const payload = JSON.parse(options.body as string);
        const idx = projects.findIndex(
          (d) => String(d.id) === String(recordId)
        );
        if (idx !== -1) {
          projects[idx] = { ...projects[idx], ...payload };
          localStorage.setItem('vk_offline_projects', JSON.stringify(projects));
        }
        return new Response(JSON.stringify({ txid: Date.now() }), {
          status: 200,
        });
      }

      if (method === 'DELETE' && recordId) {
        projects = projects.filter((d) => String(d.id) !== String(recordId));
        localStorage.setItem('vk_offline_projects', JSON.stringify(projects));
        return new Response(JSON.stringify({ txid: Date.now() }), {
          status: 200,
        });
      }
    }

    // ─── Special handling: tasks ────────────────────────────────────────────────
    if (table === 'tasks') {
      const queryProjectId = url.searchParams.get('project_id') || undefined;

      // GET /v1/tasks — with or without project_id filter
      if (method === 'GET' && !recordId) {
        if (!queryProjectId) {
          // Global Board — aggregate ALL tasks from all projects
          const allTasks = getAllTasksGlobal();
          return new Response(JSON.stringify({ tasks: allTasks }), {
            status: 200,
            headers: { 'Content-Type': 'application/json' },
          });
        }
        // Scoped to a single project
        const key = tasksKeyForProject(queryProjectId);
        const tasks = JSON.parse(
          localStorage.getItem(key) || '[]'
        ) as OfflineTask[];
        return new Response(JSON.stringify({ tasks }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      }

      // POST /v1/tasks — create task in the correct project bucket
      if (method === 'POST' && !recordId) {
        const payload = JSON.parse(options.body as string) as OfflineTask;
        if (!payload.id) payload.id = crypto.randomUUID();
        // Resolve project_id: payload > active path > fallback legacy
        const resolvedProjectId =
          payload.project_id ||
          queryProjectId ||
          getActiveProjectPath() ||
          'default';
        payload.project_id = resolvedProjectId;
        ensureProjectRegistered(resolvedProjectId);
        const key = tasksKeyForProject(resolvedProjectId);
        const tasks = JSON.parse(
          localStorage.getItem(key) || '[]'
        ) as OfflineTask[];
        tasks.push(payload);
        localStorage.setItem(key, JSON.stringify(tasks));
        return new Response(
          JSON.stringify({ txid: Date.now(), id: payload.id }),
          { status: 200 }
        );
      }

      // PATCH /v1/tasks/:id — update across all buckets
      if (method === 'PATCH' && recordId) {
        const payload = JSON.parse(
          options.body as string
        ) as Partial<OfflineTask>;
        const projects = getRegisteredProjects();
        for (const projectPath of projects) {
          const key = tasksKeyForProject(projectPath);
          const tasks = JSON.parse(
            localStorage.getItem(key) || '[]'
          ) as OfflineTask[];
          const idx = tasks.findIndex((d) => String(d.id) === String(recordId));
          if (idx !== -1) {
            tasks[idx] = { ...tasks[idx], ...payload };
            localStorage.setItem(key, JSON.stringify(tasks));
            break;
          }
        }
        return new Response(JSON.stringify({ txid: Date.now() }), {
          status: 200,
        });
      }

      // DELETE /v1/tasks/:id — remove from all buckets
      if (method === 'DELETE' && recordId) {
        const projects = getRegisteredProjects();
        for (const projectPath of projects) {
          const key = tasksKeyForProject(projectPath);
          let tasks = JSON.parse(
            localStorage.getItem(key) || '[]'
          ) as OfflineTask[];
          const before = tasks.length;
          tasks = tasks.filter((d) => String(d.id) !== String(recordId));
          if (tasks.length !== before) {
            localStorage.setItem(key, JSON.stringify(tasks));
            break;
          }
        }
        return new Response(JSON.stringify({ txid: Date.now() }), {
          status: 200,
        });
      }
    }

    // ─── Generic CRUD for all other tables ─────────────────────────────────────
    let data: any[] = JSON.parse(
      localStorage.getItem(`vk_offline_${table}`) || '[]'
    );

    if (method === 'POST') {
      if (recordId === 'bulk') {
        const body = JSON.parse(options.body as string);
        const updates = body.updates as any[];
        for (const update of updates) {
          const idx = data.findIndex((d) => d.id === update.id);
          if (idx !== -1) {
            data[idx] = { ...data[idx], ...update.changes };
          }
        }
      } else {
        const payload = JSON.parse(options.body as string);
        if (!payload.id) payload.id = crypto.randomUUID();
        // Just append to fake db
        data.push(payload);
      }
      localStorage.setItem(`vk_offline_${table}`, JSON.stringify(data));
      return new Response(JSON.stringify({ txid: Date.now() }), {
        status: 200,
      });
    } else if (method === 'PATCH') {
      const payload = JSON.parse(options.body as string);
      const idx = data.findIndex((d) => String(d.id) === String(recordId));
      if (idx !== -1) {
        data[idx] = { ...data[idx], ...payload };
        localStorage.setItem(`vk_offline_${table}`, JSON.stringify(data));
      }
      return new Response(JSON.stringify({ txid: Date.now() }), {
        status: 200,
      });
    } else if (method === 'DELETE') {
      data = data.filter((d) => String(d.id) !== String(recordId));
      localStorage.setItem(`vk_offline_${table}`, JSON.stringify(data));
      return new Response(JSON.stringify({ txid: Date.now() }), {
        status: 200,
      });
    }
  }

  // Not mocked, will probably fail but shouldn't block the UI
  return new Response(
    JSON.stringify({ error: 'Offline endpoint not mocked' }),
    { status: 404 }
  );
}

export const makeRequest = async (
  path: string,
  options: RequestInit = {},
  retryOn401 = true
): Promise<Response> => {
  // Intercept all requests assuming local-first mode
  if (path.startsWith('/v1/')) {
    return handleLocalOfflineRequest(path, options);
  }
  return makeAuthenticatedRequest(getRemoteApiUrl(), path, options, retryOn401);
};

async function makeAuthenticatedRequest(
  baseUrl: string,
  path: string,
  options: RequestInit = {},
  retryOn401 = true
): Promise<Response> {
  const authRuntime = getAuthRuntime();
  const token = await authRuntime.getToken();
  if (!token) {
    throw new Error('Not authenticated');
  }

  const headers = new Headers(options.headers ?? {});
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  headers.set('Authorization', `Bearer ${token}`);
  headers.set('X-Client-Version', __APP_VERSION__);
  headers.set('X-Client-Type', 'frontend');

  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers,
    credentials: 'include',
  });

  // Handle 401 - token may have expired
  if (response.status === 401 && retryOn401) {
    const newToken = await authRuntime.triggerRefresh();
    if (newToken) {
      // Retry the request with the new token
      headers.set('Authorization', `Bearer ${newToken}`);
      return fetch(`${baseUrl}${path}`, {
        ...options,
        headers,
        credentials: 'include',
      });
    }
    // Refresh failed, throw an auth error
    throw new Error('Session expired. Please log in again.');
  }

  return response;
}

export interface BulkUpdateIssueItem {
  id: string;
  changes: Partial<UpdateIssueRequest>;
}

export interface BulkUpdateProjectItem {
  id: string;
  changes: Partial<UpdateProjectRequest>;
}

export async function bulkUpdateProjects(
  updates: BulkUpdateProjectItem[]
): Promise<void> {
  const response = await makeRequest('/v1/projects/bulk', {
    method: 'POST',
    body: JSON.stringify({
      updates: updates.map((u) => ({ id: u.id, ...u.changes })),
    }),
  });
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.message || 'Failed to bulk update projects');
  }
}

export async function bulkUpdateIssues(
  updates: BulkUpdateIssueItem[]
): Promise<void> {
  const response = await makeRequest('/v1/issues/bulk', {
    method: 'POST',
    body: JSON.stringify({
      updates: updates.map((u) => ({ id: u.id, ...u.changes })),
    }),
  });
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.message || 'Failed to bulk update issues');
  }
}

export interface BulkUpdateProjectStatusItem {
  id: string;
  changes: Partial<UpdateProjectStatusRequest>;
}

export async function bulkUpdateProjectStatuses(
  updates: BulkUpdateProjectStatusItem[]
): Promise<void> {
  const response = await makeRequest('/v1/project_statuses/bulk', {
    method: 'POST',
    body: JSON.stringify({
      updates: updates.map((u) => ({ id: u.id, ...u.changes })),
    }),
  });
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.message || 'Failed to bulk update project statuses');
  }
}

// ---------------------------------------------------------------------------
// Relay host API functions (served by remote backend)
// ---------------------------------------------------------------------------

export async function listRelayHosts(): Promise<RelayHost[]> {
  const response = await makeRequest('/v1/hosts', { method: 'GET' });
  if (!response.ok) {
    throw await parseErrorResponse(response, 'Failed to list relay hosts');
  }

  const body = (await response.json()) as ListRelayHostsResponse;
  return body.hosts;
}

// ---------------------------------------------------------------------------
// SAS URL cache with TTL — SAS URLs expire after 5 minutes, cache for 4
// ---------------------------------------------------------------------------

const SAS_URL_TTL_MS = 4 * 60 * 1000;

interface CachedSasUrl {
  url: string;
  expiresAt: number;
}

const sasUrlCache = new Map<string, CachedSasUrl>();

// ---------------------------------------------------------------------------
// Utility: SHA-256 file hash
// ---------------------------------------------------------------------------

export async function computeFileHash(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const hash = await crypto.subtle.digest('SHA-256', buffer);
  return Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

// ---------------------------------------------------------------------------
// Utility: Upload to Azure Blob Storage with progress
// ---------------------------------------------------------------------------

export function uploadToAzure(
  uploadUrl: string,
  file: File,
  onProgress?: (pct: number) => void
): Promise<void> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open('PUT', uploadUrl, true);
    xhr.setRequestHeader('x-ms-blob-type', 'BlockBlob');
    xhr.setRequestHeader('Content-Type', file.type);

    if (onProgress) {
      xhr.upload.addEventListener('progress', (e) => {
        if (e.lengthComputable) {
          onProgress(Math.round((e.loaded / e.total) * 100));
        }
      });
    }

    xhr.onload = () => {
      if (xhr.status === 201) {
        resolve();
      } else {
        reject(
          new Error(
            `Azure upload failed with status ${xhr.status}: ${xhr.statusText}`
          )
        );
      }
    };

    xhr.onerror = () => {
      reject(new Error('Azure upload failed: network error'));
    };

    xhr.send(file);
  });
}

// ---------------------------------------------------------------------------
// Utility: safe error response parsing (handles non-JSON error bodies)
// ---------------------------------------------------------------------------

async function parseErrorResponse(
  response: Response,
  fallbackMessage: string
): Promise<Error> {
  try {
    const body = await response.json();
    const message = body.error || body.message || fallbackMessage;
    return new Error(`${message} (${response.status} ${response.statusText})`);
  } catch {
    return new Error(
      `${fallbackMessage} (${response.status} ${response.statusText})`
    );
  }
}

// ---------------------------------------------------------------------------
// Attachment API functions
// ---------------------------------------------------------------------------

export async function initAttachmentUpload(
  params: InitUploadRequest
): Promise<InitUploadResponse> {
  const response = await makeRequest('/v1/attachments/init', {
    method: 'POST',
    body: JSON.stringify(params),
  });
  if (!response.ok) {
    throw await parseErrorResponse(
      response,
      'Failed to init attachment upload'
    );
  }
  return response.json();
}

export async function confirmAttachmentUpload(
  params: ConfirmUploadRequest
): Promise<AttachmentWithBlob> {
  const response = await makeRequest('/v1/attachments/confirm', {
    method: 'POST',
    body: JSON.stringify(params),
  });
  if (!response.ok) {
    throw await parseErrorResponse(
      response,
      'Failed to confirm attachment upload'
    );
  }
  return response.json();
}

export async function commitIssueAttachments(
  issueId: string,
  request: CommitAttachmentsRequest
): Promise<CommitAttachmentsResponse> {
  const response = await makeRequest(
    `/v1/issues/${issueId}/attachments/commit`,
    {
      method: 'POST',
      body: JSON.stringify(request),
    }
  );
  if (!response.ok) {
    throw await parseErrorResponse(
      response,
      'Failed to commit issue attachments'
    );
  }
  return response.json();
}

export async function commitCommentAttachments(
  commentId: string,
  request: CommitAttachmentsRequest
): Promise<CommitAttachmentsResponse> {
  const response = await makeRequest(
    `/v1/comments/${commentId}/attachments/commit`,
    {
      method: 'POST',
      body: JSON.stringify(request),
    }
  );
  if (!response.ok) {
    throw await parseErrorResponse(
      response,
      'Failed to commit comment attachments'
    );
  }
  return response.json();
}

export async function deleteAttachment(attachmentId: string): Promise<void> {
  const response = await makeRequest(`/v1/attachments/${attachmentId}`, {
    method: 'DELETE',
  });
  if (!response.ok) {
    throw await parseErrorResponse(response, 'Failed to delete attachment');
  }
}

export async function fetchAttachmentSasUrl(
  attachmentId: string,
  type: 'file' | 'thumbnail'
): Promise<string> {
  const cacheKey = `${attachmentId}:${type}`;
  const cached = sasUrlCache.get(cacheKey);
  if (cached && Date.now() < cached.expiresAt) {
    return cached.url;
  }

  const response = await makeRequest(`/v1/attachments/${attachmentId}/${type}`);
  if (!response.ok) {
    throw new Error(
      `Failed to fetch attachment ${type}: ${response.statusText}`
    );
  }

  const data: AttachmentUrlResponse = await response.json();
  sasUrlCache.set(cacheKey, {
    url: data.url,
    expiresAt: Date.now() + SAS_URL_TTL_MS,
  });
  return data.url;
}
