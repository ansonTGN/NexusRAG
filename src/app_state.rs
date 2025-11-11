use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use neo4rs::Graph;
use tokio::sync::oneshot;
use crate::{config::AppConfig, llm::LlmManager};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub graph: Arc<Graph>,
    pub llm_manager: LlmManager,
    pub status: Arc<Mutex<Status>>,
    pub current_dir: Arc<Mutex<Option<PathBuf>>>,
    pub shutdown_sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

// MODIFICADO: Añadido el campo 'progress'.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Status {
    pub is_busy: bool,
    pub message: String,
    pub progress: f32, // Valor entre 0.0 y 1.0
}