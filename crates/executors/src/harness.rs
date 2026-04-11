//! # LocalProcessHarness
//!
//! Управляет запуском локальных CLI-агентов (Antigravity / Claude Code / Local LLM) как
//! дочерних OS-процессов.  
//!
//! ## Жизненный цикл
//! 1. Фронтенд (или тест) отправляет [`AgentHarnessConfig`] на POST `/api/agent-config/start`.
//! 2. Роут вызывает [`LocalProcessHarness::spawn`].
//! 3. `spawn` формирует команду, пробрасывает ENV и возвращает [`SpawnedChild`].
//! 4. Остальной pipeline (логи, ACP) продолжает работу без изменений.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use command_group::AsyncCommandGroup;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use ts_rs::TS;

use crate::{
    env::ExecutionEnv,
    executors::{ExecutorError, SpawnedChild},
};

// ─── Ошибки ─────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("Executable not found: {0}")]
    ExecutableNotFound(String),

    #[error("Spawn failed: {0}")]
    Spawn(#[from] std::io::Error),

    #[error("Invalid harness config: {0}")]
    InvalidConfig(String),
}

impl From<HarnessError> for ExecutorError {
    fn from(e: HarnessError) -> Self {
        ExecutorError::Io(match e {
            HarnessError::Spawn(io) => io,
            HarnessError::ExecutableNotFound(prog) => std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Executable not found: {prog}"),
            ),
            HarnessError::InvalidConfig(msg) => {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
            }
        })
    }
}

// ─── Конфигурация агента ────────────────────────────────────────────────────

/// Тип фреймворка агента.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentFramework {
    /// Google Antigravity (gemini-cli / Antigravity CLI)
    Antigravity,
    /// Anthropic Claude Code CLI
    ClaudeCode,
    /// Произвольный локальный LLM сервер (совместимый с OpenAI API)
    LocalLlm,
}

impl AgentFramework {
    /// Возвращает путь к бинарнику по умолчанию для данного фреймворка.
    pub fn default_executable(&self) -> &'static str {
        match self {
            AgentFramework::Antigravity => "antigravity",
            AgentFramework::ClaudeCode => "claude",
            AgentFramework::LocalLlm => "ollama",
        }
    }

    /// Базовые аргументы CLI для неинтерактивного запуска.
    pub fn base_args(&self) -> Vec<&'static str> {
        match self {
            AgentFramework::Antigravity => vec!["run", "--no-interactive"],
            AgentFramework::ClaudeCode => vec!["--output-format", "stream-json", "--print"],
            AgentFramework::LocalLlm => vec!["run", "--format", "plain"],
        }
    }
}

/// Конфигурация, принимаемая с фронтенда или хранящаяся в профиле.
///
/// Все поля опциональны: незаполненные берутся из дефолтов фреймворка.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct AgentHarnessConfig {
    /// Тип фреймворка
    pub framework: AgentFramework,

    /// Переопределение пути к бинарнику (напр. `/usr/local/bin/my-claude`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_override: Option<String>,

    /// Дополнительные CLI-аргументы (добавляются после базовых)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,

    /// Gemini / Antigravity API ключ
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_api_key: Option<String>,

    /// URL локального LLM сервера (напр. `http://localhost:11434`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_llm_url: Option<String>,

    /// Дополнительные переменные окружения (key → value).
    /// Эти значения имеют приоритет над [`ExecutionEnv::vars`].
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env_overrides: HashMap<String, String>,
}

impl AgentHarnessConfig {
    /// Строит карту ENV-переменных, которые нужно выставить для процесса.
    pub fn build_env(&self) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = HashMap::new();

        // Gemini / Antigravity API key
        if let Some(ref key) = self.gemini_api_key {
            env.insert("GEMINI_API_KEY".to_string(), key.clone());
            env.insert("GOOGLE_API_KEY".to_string(), key.clone());
        }

        // Local LLM base URL
        if let Some(ref url) = self.local_llm_url {
            env.insert("OPENAI_BASE_URL".to_string(), url.clone());
            env.insert("LOCAL_LLM_URL".to_string(), url.clone());
        }

        // Framework identifier — полезно для самих агентов и логгирования
        env.insert(
            "VK_AGENT_FRAMEWORK".to_string(),
            format!("{:?}", self.framework),
        );

        // User-supplied overrides have the highest priority
        env.extend(self.env_overrides.clone());

        env
    }
}

// ─── Harness ────────────────────────────────────────────────────────────────

/// Менеджер запуска локальных CLI-агентов.
///
/// Создаётся per-task и не хранит долгоживущего состояния.
#[derive(Debug)]
pub struct LocalProcessHarness {
    config: AgentHarnessConfig,
}

impl LocalProcessHarness {
    pub fn new(config: AgentHarnessConfig) -> Self {
        Self { config }
    }

    /// Резолвит путь к исполняемому файлу агента.
    ///
    /// Приоритет: `config.executable_override` → `which(default_executable)`.
    fn resolve_executable(&self) -> Result<PathBuf, HarnessError> {
        if let Some(ref override_path) = self.config.executable_override {
            let p = PathBuf::from(override_path);
            if p.is_absolute() {
                return Ok(p);
            }
            // Попробуем найти через PATH
            return which(override_path)
                .ok_or_else(|| HarnessError::ExecutableNotFound(override_path.clone()));
        }

        let default_bin = self.config.framework.default_executable();
        which(default_bin).ok_or_else(|| HarnessError::ExecutableNotFound(default_bin.to_string()))
    }

    /// Запускает дочерний процесс агента.
    ///
    /// # Параметры
    /// - `working_dir` — рабочая директория (обычно workspace root задачи).
    /// - `prompt`      — начальная инструкция/задача для агента.
    /// - `env`         — окружение из pipeline (содержит VK_PROJECT_NAME, и т.д.).
    ///
    /// # Как устроен spawn
    /// ```text
    /// Command::new(<executable>)
    ///   .args(<base_args> + <extra_args>)
    ///   .arg(<prompt>)           // последним аргументом — промпт
    ///   .current_dir(<working_dir>)
    ///   .kill_on_drop(true)
    ///   .envs(<merged_env>)
    ///   .group_spawn()           // command-group → процессная группа для kill
    /// ```
    pub async fn spawn(
        &self,
        working_dir: &Path,
        prompt: &str,
        env: &ExecutionEnv,
    ) -> Result<SpawnedChild, HarnessError> {
        let executable = self.resolve_executable()?;

        let mut cmd = Command::new(&executable);

        // 1. Базовые аргументы фреймворка
        cmd.args(self.config.framework.base_args());

        // 2. Дополнительные пользовательские аргументы
        if !self.config.extra_args.is_empty() {
            cmd.args(&self.config.extra_args);
        }

        // 3. Последний аргумент — промпт (большинство агентов принимает его так)
        cmd.arg(prompt);

        // 4. Рабочая директория
        cmd.current_dir(working_dir);

        // 5. ENV: сначала pipeline-переменные, затем harness-переменные (перезаписывают)
        env.apply_to_command(&mut cmd);
        cmd.envs(self.config.build_env());

        // 6. Не наследуем stdin чтобы агент не блокировался
        cmd.stdin(std::process::Stdio::null());

        // stdout/stderr — пайп (parent будет читать)
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // kill_on_drop обеспечивает clean-up если SpawnedChild дропнется
        cmd.kill_on_drop(true);

        tracing::info!(
            executable = %executable.display(),
            working_dir = %working_dir.display(),
            framework = ?self.config.framework,
            "LocalProcessHarness: spawning agent process"
        );

        // group_spawn() — запускаем в процессной группе, чтобы kill дошёл до дочерних
        let child = cmd.group_spawn().map_err(HarnessError::Spawn)?;

        Ok(SpawnedChild::from(child))
    }
}

// ─── Вспомогательный `which` ────────────────────────────────────────────────

/// Ищет `program` в PATH и возвращает полный путь, если найден.
fn which(program: &str) -> Option<PathBuf> {
    // Используем tokio blocking-pool если нужно sync → async, но тут простой sync вызов
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|path| {
                path.is_file() && {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        path.metadata()
                            .map(|m| m.permissions().mode() & 0o111 != 0)
                            .unwrap_or(false)
                    }
                    #[cfg(not(unix))]
                    {
                        true
                    }
                }
            })
    })
}

// ─── Тесты ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(framework: AgentFramework) -> AgentHarnessConfig {
        AgentHarnessConfig {
            framework,
            executable_override: None,
            extra_args: vec![],
            gemini_api_key: None,
            local_llm_url: None,
            env_overrides: HashMap::new(),
        }
    }

    #[test]
    fn env_contains_gemini_key() {
        let mut cfg = make_config(AgentFramework::Antigravity);
        cfg.gemini_api_key = Some("test-key-123".to_string());
        let env = cfg.build_env();
        assert_eq!(env.get("GEMINI_API_KEY").unwrap(), "test-key-123");
        assert_eq!(env.get("GOOGLE_API_KEY").unwrap(), "test-key-123");
    }

    #[test]
    fn env_contains_local_llm_url() {
        let mut cfg = make_config(AgentFramework::LocalLlm);
        cfg.local_llm_url = Some("http://localhost:11434".to_string());
        let env = cfg.build_env();
        assert_eq!(
            env.get("OPENAI_BASE_URL").unwrap(),
            "http://localhost:11434"
        );
        assert_eq!(env.get("LOCAL_LLM_URL").unwrap(), "http://localhost:11434");
    }

    #[test]
    fn env_overrides_have_priority() {
        let mut cfg = make_config(AgentFramework::Antigravity);
        cfg.gemini_api_key = Some("default-key".to_string());
        cfg.env_overrides
            .insert("GEMINI_API_KEY".to_string(), "custom-key".to_string());
        let env = cfg.build_env();
        assert_eq!(env.get("GEMINI_API_KEY").unwrap(), "custom-key");
    }

    #[test]
    fn framework_defaults() {
        assert_eq!(
            AgentFramework::Antigravity.default_executable(),
            "antigravity"
        );
        assert_eq!(AgentFramework::ClaudeCode.default_executable(), "claude");
        assert_eq!(AgentFramework::LocalLlm.default_executable(), "ollama");
    }
}
