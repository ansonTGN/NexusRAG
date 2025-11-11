//! Carga y gestión de configuración de la aplicación (Neo4j + LLM).

use std::env;
use anyhow::{anyhow, Result};

#[derive(Clone, Debug)]
pub enum LlmProvider {
    OpenAI,
    Gemini,
    Ollama,
}

impl LlmProvider {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "gemini" => Ok(Self::Gemini),
            "ollama" => Ok(Self::Ollama),
            other => Err(anyhow!("Proveedor LLM no soportado: {other}")),
        }
    }
}

/// Configuración completa de la aplicación.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub neo4j_uri: String,
    pub neo4j_user: String,
    pub neo4j_password: String,
    pub server_addr: String,

    pub llm_provider: LlmProvider,
    pub llm_embedding_model: String,
    pub llm_chat_model: String,
}

impl AppConfig {
    /// Carga la configuración desde variables de entorno (usando .env si existe).
    pub fn from_env() -> Result<Self> {
        let neo4j_uri = env::var("NEO4J_URI")
            .map_err(|_| anyhow!("Falta NEO4J_URI en el entorno"))?;
        let neo4j_user = env::var("NEO4J_USER")
            .map_err(|_| anyhow!("Falta NEO4J_USER en el entorno"))?;
        let neo4j_password = env::var("NEO4J_PASSWORD")
            .map_err(|_| anyhow!("Falta NEO4J_PASSWORD en el entorno"))?;

        let server_addr =
            env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:3322".to_string());

        let llm_provider_str =
            env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string());
        let llm_provider = LlmProvider::from_str(&llm_provider_str)?;

        let llm_embedding_model = env::var("LLM_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".to_string());
        let llm_chat_model =
            env::var("LLM_CHAT_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());

        Ok(Self {
            neo4j_uri,
            neo4j_user,
            neo4j_password,
            server_addr,
            llm_provider,
            llm_embedding_model,
            llm_chat_model,
        })
    }
}
