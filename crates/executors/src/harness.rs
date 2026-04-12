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

// ─── PM Orchestrator: Skill Injector ────────────────────────────────────────

/// Reads the `VK_CUSTOM_SKILLS` env variable (colon- or semicolon-separated
/// list of directory paths) and returns a list of skill names discovered by
/// scanning each directory for `SKILL.md` files.
///
/// A `SKILL.md` file is expected to start with a YAML frontmatter block:
/// ```yaml
/// ---
/// name: my-skill-name
/// description: ...
/// ---
/// ```
/// The `name:` field is extracted from the frontmatter. If no `name:` is found,
/// the parent directory name is used as a fallback.
pub fn collect_skill_names_from_env(env_overrides: &HashMap<String, String>) -> Vec<String> {
    // Priority: API-supplied env_overrides > system environment variable.
    // This allows users to set VK_CUSTOM_SKILLS in their shell / .env file
    // without having to pass it explicitly through the agent config API.
    let skills_path_str = env_overrides
        .get("VK_CUSTOM_SKILLS")
        .filter(|v| !v.is_empty())
        .cloned()
        .or_else(|| std::env::var("VK_CUSTOM_SKILLS").ok())
        .unwrap_or_default();

    if skills_path_str.is_empty() {
        return Vec::new();
    }

    // Accept both `:` (Unix-like PATH) and `;` (Windows-style / user preference)
    let search_dirs: Vec<&str> = skills_path_str
        .split(|c| c == ':' || c == ';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let mut skill_names: Vec<String> = Vec::new();

    for dir_str in search_dirs {
        let dir = PathBuf::from(dir_str);
        if !dir.is_dir() {
            tracing::debug!(path = %dir.display(), "VK_CUSTOM_SKILLS: path not found or not a dir, skipping");
            continue;
        }
        scan_skills_dir(&dir, &mut skill_names);
    }

    skill_names
}

/// Recursively scans `dir` for `SKILL.md` files and appends discovered skill
/// names to `out`. Depth is limited to 3 levels to avoid runaway traversal.
fn scan_skills_dir(dir: &Path, out: &mut Vec<String>) {
    scan_skills_dir_depth(dir, 0, out);
}

fn scan_skills_dir_depth(dir: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 3 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(path = %dir.display(), error = %e, "Failed to read skills dir");
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_skills_dir_depth(&path, depth + 1, out);
        } else if path.file_name().map(|f| f == "SKILL.md").unwrap_or(false) {
            if let Some(name) = extract_skill_name_from_file(&path) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
}

/// Extracts the `name:` value from the YAML frontmatter of a `SKILL.md` file.
/// Returns `None` if parsing fails; caller may use the directory name as fallback.
fn extract_skill_name_from_file(skill_md_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(skill_md_path).ok()?;

    // The file must start with `---` to have valid frontmatter.
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        // No frontmatter — use the parent directory name
        return skill_md_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|f| f.to_string_lossy().into_owned());
    }

    // Find the closing `---`
    let after_open = &trimmed[3..];
    let (frontmatter, _) = after_open.split_once("---")?;

    // Simple line-by-line scan for `name:` field (no full YAML parser dependency)
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.starts_with("name:") {
            let value = line["name:".len()..]
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    // Fallback: parent dir name
    skill_md_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|f| f.to_string_lossy().into_owned())
}

/// Builds the skills preamble to inject into the system prompt.
///
/// Example output:
/// ```text
/// # Available Skills (@)
/// Use these skills for specialised tasks by mentioning them as @skill-name:
/// - @backend-architect
/// - @typescript-pro
/// ...
/// ---
/// ```
pub fn build_skills_preamble(skill_names: &[String]) -> String {
    if skill_names.is_empty() {
        return String::new();
    }
    let mut preamble = String::from(
        "# Available Skills (@)\nUse these skills for specialised tasks by mentioning them as @skill-name:\n",
    );
    for name in skill_names {
        preamble.push_str(&format!("- @{name}\n"));
    }
    preamble.push_str("---\n\n");
    preamble
}

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

        // ── PM Orchestrator: inject available skills into the prompt ───────────
        // Scan VK_CUSTOM_SKILLS directories for SKILL.md files and prepend a
        // structured "Available Skills" block to the prompt so the Orchestrator
        // knows which @skills are available before calling the LLM.
        let skill_names = collect_skill_names_from_env(&self.config.env_overrides);
        let skills_preamble = build_skills_preamble(&skill_names);

        let enriched_prompt: String = if skills_preamble.is_empty() {
            prompt.to_string()
        } else {
            tracing::info!(
                skills_count = skill_names.len(),
                skills = ?skill_names,
                "LocalProcessHarness: injecting skills into system prompt"
            );
            format!("{skills_preamble}{prompt}")
        };
        // ─────────────────────────────────────────────────────────────────────

        let mut cmd = Command::new(&executable);

        // 1. Базовые аргументы фреймворка
        cmd.args(self.config.framework.base_args());

        // 2. Дополнительные пользовательские аргументы
        if !self.config.extra_args.is_empty() {
            cmd.args(&self.config.extra_args);
        }

        // 3. Последний аргумент — обогащённый промпт со скиллами
        cmd.arg(enriched_prompt.as_str());

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
            skills_injected = skill_names.len(),
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

    // ── Skill injector tests ─────────────────────────────────────────────────

    #[test]
    fn no_skills_env_returns_empty() {
        let env: HashMap<String, String> = HashMap::new();
        assert!(collect_skill_names_from_env(&env).is_empty());
    }

    #[test]
    fn empty_skills_path_returns_empty() {
        let mut env = HashMap::new();
        env.insert("VK_CUSTOM_SKILLS".to_string(), String::new());
        assert!(collect_skill_names_from_env(&env).is_empty());
    }

    #[test]
    fn build_skills_preamble_empty_list() {
        assert_eq!(build_skills_preamble(&[]), "");
    }

    #[test]
    fn build_skills_preamble_formats_correctly() {
        let names = vec![
            "backend-architect".to_string(),
            "typescript-pro".to_string(),
        ];
        let preamble = build_skills_preamble(&names);
        assert!(preamble.contains("@backend-architect"));
        assert!(preamble.contains("@typescript-pro"));
        assert!(preamble.starts_with("# Available Skills"));
    }

    #[test]
    fn extract_skill_name_from_frontmatter() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("vk_test_skill_extractor");
        std::fs::create_dir_all(&dir).unwrap();
        let skill_path = dir.join("SKILL.md");
        let mut f = std::fs::File::create(&skill_path).unwrap();
        f.write_all(b"---\nname: my-awesome-skill\ndescription: A test skill\n---\n# Content")
            .unwrap();
        let name = extract_skill_name_from_file(&skill_path);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(name, Some("my-awesome-skill".to_string()));
    }

    #[test]
    fn extract_skill_name_falls_back_to_dir_name() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("vk_test_no_frontmatter");
        std::fs::create_dir_all(&dir).unwrap();
        let skill_path = dir.join("SKILL.md");
        let mut f = std::fs::File::create(&skill_path).unwrap();
        f.write_all(b"# Just a plain markdown file with no frontmatter")
            .unwrap();
        let name = extract_skill_name_from_file(&skill_path);
        std::fs::remove_dir_all(&dir).unwrap();
        // Fallback: parent dir name
        assert_eq!(name, Some("vk_test_no_frontmatter".to_string()));
    }
}
