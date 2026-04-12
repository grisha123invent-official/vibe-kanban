/**
 * Agent Config API — connects the Profile/Settings UI to the
 * POST /api/agent-config/start backend route created by Worker B.
 *
 * Field mapping:
 *   Frontend (camelCase)        →  Rust (snake_case / SCREAMING_SNAKE_CASE)
 *   ─────────────────────────────────────────────────────────────────────
 *   agentFramework: 'antigravity'  →  framework: 'ANTIGRAVITY'
 *   agentFramework: 'claude_code'  →  framework: 'CLAUDE_CODE'
 *   geminiApiKey                   →  gemini_api_key
 *   claudeApiKey                   →  env_overrides.ANTHROPIC_API_KEY
 *   localLlmUrl                    →  local_llm_url
 *   customSkills                   →  env_overrides.VK_CUSTOM_SKILLS
 */

import { makeLocalApiRequest } from '@/shared/lib/localApiTransport';
import { handleApiResponse } from '@/shared/lib/api';

// ─── DTO types (mirror of Rust structs in crates/executors/src/harness.rs) ───

/** Mirrors `AgentFramework` Rust enum (serde: SCREAMING_SNAKE_CASE). */
export type AgentFramework = 'ANTIGRAVITY' | 'CLAUDE_CODE' | 'LOCAL_LLM';

/** Mirrors `AgentHarnessConfig` Rust struct. */
export interface AgentHarnessConfig {
  framework: AgentFramework;
  executable_override?: string;
  extra_args?: string[];
  gemini_api_key?: string;
  local_llm_url?: string;
  env_overrides?: Record<string, string>;
}

/** Mirrors `StartAgentRequest` Rust struct (crates/server/src/routes/agent_config.rs). */
export interface StartAgentRequest {
  harness: AgentHarnessConfig;
  prompt: string;
  working_dir?: string;
}

/** Mirrors `StartAgentResponse` Rust struct. */
export interface StartAgentResponse {
  pid: number | null;
  status: string;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Maps the frontend framework selector value to the Rust enum variant.
 * The frontend uses lowercase-with-underscores; Rust uses SCREAMING_SNAKE_CASE.
 */
export function toAgentFramework(
  value: 'antigravity' | 'claude_code' | 'local_llm' | string
): AgentFramework {
  switch (value) {
    case 'antigravity':
      return 'ANTIGRAVITY';
    case 'claude_code':
      return 'CLAUDE_CODE';
    case 'local_llm':
      return 'LOCAL_LLM';
    default:
      return 'ANTIGRAVITY';
  }
}

/**
 * Converts the flat Profile-form fields into an `AgentHarnessConfig` payload
 * suitable for the backend.
 */
export function buildHarnessConfig(params: {
  agentFramework: string;
  geminiApiKey: string;
  claudeApiKey: string;
  localLlmUrl: string;
  llmApiKey: string;
  customSkills: string;
}): AgentHarnessConfig {
  const {
    agentFramework,
    geminiApiKey,
    claudeApiKey,
    localLlmUrl,
    llmApiKey,
    customSkills,
  } = params;

  const env_overrides: Record<string, string> = {};

  if (claudeApiKey) {
    env_overrides['ANTHROPIC_API_KEY'] = claudeApiKey;
  }

  if (llmApiKey) {
    env_overrides['OPENAI_API_KEY'] = llmApiKey;
    env_overrides['LOCAL_LLM_API_KEY'] = llmApiKey;
  }

  if (customSkills) {
    env_overrides['VK_CUSTOM_SKILLS'] = customSkills;
  }

  const config: AgentHarnessConfig = {
    framework: toAgentFramework(agentFramework),
  };

  if (geminiApiKey) {
    config.gemini_api_key = geminiApiKey;
  }

  if (localLlmUrl) {
    config.local_llm_url = localLlmUrl;
  }

  if (Object.keys(env_overrides).length > 0) {
    config.env_overrides = env_overrides;
  }

  return config;
}

// ─── API client ───────────────────────────────────────────────────────────────

export const agentConfigApi = {
  /**
   * POST /api/agent-config/start
   *
   * Spawns a local CLI agent process with the given configuration.
   * Returns the PID and status of the spawned process.
   */
  start: async (req: StartAgentRequest): Promise<StartAgentResponse> => {
    const headers = new Headers();
    headers.set('Content-Type', 'application/json');

    const response = await makeLocalApiRequest('/api/agent-config/start', {
      method: 'POST',
      body: JSON.stringify(req),
      headers,
    });

    return handleApiResponse<StartAgentResponse>(response);
  },
};
