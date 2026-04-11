//! POST /api/agent-config/start — запуск задачи через [`LocalProcessHarness`].
//!
//! Этот роут принимает конфигурацию агента (API-ключ, URL local LLM, тип фреймворка)
//! и немедленно спавнит дочерний процесс, возвращая PID и статус.

use axum::{Json, Router, extract::State, routing::post};
use executors::{
    env::{ExecutionEnv, RepoContext},
    harness::{AgentHarnessConfig, LocalProcessHarness},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utils::response::ApiResponse;

use crate::{DeploymentImpl, error::ApiError};

pub fn router() -> Router<DeploymentImpl> {
    Router::new().route("/start", post(start_agent))
}

/// Тело запроса на запуск агента.
#[derive(Debug, Deserialize, TS)]
pub struct StartAgentRequest {
    /// Полная конфигурация harness (фреймворк, ключи, env-переменные)
    pub harness: AgentHarnessConfig,

    /// Промпт / задача для агента
    pub prompt: String,

    /// Рабочая директория (абсолютный путь).
    /// Если не указана — берётся текущая директория сервера.
    #[serde(default)]
    pub working_dir: Option<String>,
}

/// Данные ответа при успешном спавне.
#[derive(Debug, Serialize, TS)]
pub struct StartAgentResponse {
    /// PID дочернего процесса
    pub pid: Option<u32>,
    pub status: String,
}

/// POST /api/agent-config/start
///
/// 1. Читает [`AgentHarnessConfig`] из тела запроса.
/// 2. Создаёт минимальный [`ExecutionEnv`].
/// 3. Вызывает [`LocalProcessHarness::spawn`] — запускает OS-процесс агента.
/// 4. Возвращает PID + статус клиенту.
///
/// Долгоживущий stdout агента в данном POC не стримится обратно —
/// для полноценного pipeline нужно передать `SpawnedChild` в контейнер задачи.
#[axum::debug_handler]
async fn start_agent(
    State(_deployment): State<DeploymentImpl>,
    Json(req): Json<StartAgentRequest>,
) -> Result<Json<ApiResponse<StartAgentResponse>>, ApiError> {
    let working_dir = match &req.working_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    };

    // Минимальный ExecutionEnv (без git-контекста, подходит для POC)
    let env = ExecutionEnv::new(RepoContext::default(), false, String::new());

    let harness = LocalProcessHarness::new(req.harness);

    match harness.spawn(&working_dir, &req.prompt, &env).await {
        Ok(mut spawned) => {
            // Получаем PID из дочернего процесса (до его передачи в pipeline)
            // inner() даёт доступ к tokio::process::Child, id() возвращает Option<u32>
            let pid = spawned.child.inner().id();

            tracing::info!(pid = ?pid, "Agent process spawned successfully");

            Ok(Json(ApiResponse::success(StartAgentResponse {
                pid,
                status: "spawned".to_string(),
            })))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to spawn agent process");
            Ok(Json(ApiResponse::error(&format!(
                "Failed to spawn agent: {}",
                e
            ))))
        }
    }
}
