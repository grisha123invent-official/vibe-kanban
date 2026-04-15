import type { BaseCodingAgent } from 'shared/types';

export interface ExecutorInstallInfo {
  displayName: string;
  description: string;
  /** URL to the official install/docs page */
  installUrl: string;
  /** Shell command the user can copy to install */
  installCommand?: string;
  /** If auth (login) is required after install */
  requiresLogin?: boolean;
  /** Short hint shown below the install command */
  installNote?: string;
}

export const EXECUTOR_INSTALL_INFO: Record<
  BaseCodingAgent,
  ExecutorInstallInfo
> = {
  CLAUDE_CODE: {
    displayName: 'Claude Code',
    description: 'AI coding assistant by Anthropic',
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code/getting-started',
    installCommand: 'npm install -g @anthropic-ai/claude-code',
    requiresLogin: true,
    installNote: 'After install run: claude (to authenticate)',
  },
  GEMINI: {
    displayName: 'Gemini CLI',
    description: 'AI coding assistant by Google',
    installUrl: 'https://github.com/google-gemini/gemini-cli',
    installCommand: 'npm install -g @google/gemini-cli',
    requiresLogin: true,
    installNote: 'After install run: gemini (to authenticate)',
  },
  CODEX: {
    displayName: 'Codex',
    description: 'AI coding assistant by OpenAI',
    installUrl: 'https://github.com/openai/codex',
    installCommand: 'npm install -g @openai/codex',
    requiresLogin: true,
    installNote: 'After install run: codex (to authenticate)',
  },
  OPENCODE: {
    displayName: 'OpenCode',
    description: 'Open-source AI coding assistant',
    installUrl: 'https://opencode.ai',
    installCommand: 'npm install -g opencode-ai',
    requiresLogin: false,
    installNote: 'Configure your API key in ~/.opencode/config.json',
  },
  CURSOR_AGENT: {
    displayName: 'Cursor',
    description: 'AI-first code editor with built-in agent',
    installUrl: 'https://cursor.com',
    requiresLogin: true,
    installNote: 'Download and install Cursor from cursor.com',
  },
  QWEN_CODE: {
    displayName: 'Qwen Code',
    description: 'AI coding assistant powered by Alibaba Qwen',
    installUrl: 'https://github.com/QwenLM/qwen-code',
    installCommand: 'npm install -g qwen-code',
    requiresLogin: false,
    installNote: 'Configure your Dashscope API key after install',
  },
  COPILOT: {
    displayName: 'GitHub Copilot',
    description: 'AI coding assistant by GitHub',
    installUrl: 'https://github.com/features/copilot',
    installCommand: 'npm install -g @github/copilot-language-server',
    requiresLogin: true,
    installNote: 'Requires an active GitHub Copilot subscription',
  },
  DROID: {
    displayName: 'Droid',
    description: 'AI coding assistant by Factory',
    installUrl: 'https://factory.ai',
    requiresLogin: true,
    installNote: 'Sign up at factory.ai and install the Droid CLI',
  },
  AMP: {
    displayName: 'Amp',
    description: 'AI coding assistant by Sourcegraph',
    installUrl: 'https://ampcode.com',
    requiresLogin: true,
    installNote: 'Sign up at ampcode.com and install the Amp CLI',
  },
};
