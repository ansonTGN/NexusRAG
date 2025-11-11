//! Abstracción sobre Rig para trabajar con distintos proveedores de LLM.
//! De momento se implementa OpenAI; Gemini/Ollama quedan preparados para el futuro.

use crate::config::{AppConfig, LlmProvider};
use anyhow::{anyhow, Result};
use rig::completion::Prompt;
use rig::embeddings::EmbeddingModel; // <- para .embed_texts
use serde::Deserialize;
use tracing::warn;

/// Resultado de un embedding de un chunk.
#[derive(Debug, Clone)]
pub struct EmbeddedChunk {
    pub id: String,
    pub text: String,
    pub vector: Vec<f64>,
}

// --- MEJORA: Estructuras para la extracción de entidades y relaciones ---

#[derive(Debug, Clone, Deserialize)]
pub struct JsonExtractedEntity {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonExtractedRelation {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ExtractionResult {
    pub entities: Vec<JsonExtractedEntity>,
    pub relations: Vec<JsonExtractedRelation>,
}


/// Gestor de LLMs y embeddings.
#[derive(Debug, Clone)]
pub struct LlmManager {
    pub provider: LlmProvider,
    pub embedding_model: String,
    pub chat_model: String,
}

impl LlmManager {
    /// Construye el manager a partir de la configuración.
    pub fn from_config(cfg: &AppConfig) -> Result<Self> {
        Ok(Self {
            provider: cfg.llm_provider.clone(),
            embedding_model: cfg.llm_embedding_model.clone(),
            chat_model: cfg.llm_chat_model.clone(),
        })
    }

    // ---------------------------------------------------------------------
    // EMBEDDINGS
    // ---------------------------------------------------------------------

    /// Calcula embeddings para una lista de (id, texto).
    ///
    /// Nota: sólo implementado para OpenAI. Para otros proveedores
    /// se podrían añadir ramas adicionales al `match`.
    pub async fn embed_chunks(
        &self,
        chunks: &[(String, String)],
    ) -> Result<Vec<EmbeddedChunk>> {
        match self.provider {
            LlmProvider::OpenAI => self.embed_with_openai(chunks).await,
            ref other => Err(anyhow!(
                "Proveedor LLM {:?} aún no implementado para embeddings",
                other
            )),
        }
    }

    async fn embed_with_openai(
        &self,
        chunks: &[(String, String)],
    ) -> Result<Vec<EmbeddedChunk>> {
        use rig::providers::openai::{self, TEXT_EMBEDDING_3_SMALL};
        // Trait para client.embedding_model(...)
        use rig::client::EmbeddingsClient as _;

        // Cliente OpenAI de Rig
        let client = openai::Client::from_env();

        // Modelo de embeddings: config o default
        let model_name = if self.embedding_model.is_empty() {
            TEXT_EMBEDDING_3_SMALL
        } else {
            self.embedding_model.as_str()
        };

        let embedding_model = client.embedding_model(model_name);

        // Extraemos sólo los textos
        let texts: Vec<String> = chunks.iter().map(|(_, text)| text.clone()).collect();

        // Embeddings en bloque (.embed_texts viene de EmbeddingModel)
        let embeddings = embedding_model.embed_texts(texts.clone()).await?;

        if embeddings.len() != chunks.len() {
            return Err(anyhow!(
                "Número de embeddings ({}) distinto al número de chunks ({})",
                embeddings.len(),
                chunks.len()
            ));
        }

        // Reconstruimos EmbeddedChunk con id + texto + vector
        let mut result = Vec::new();
        for ((id, text), emb) in chunks.iter().zip(embeddings.iter()) {
            result.push(EmbeddedChunk {
                id: id.clone(),
                text: text.clone(),
                vector: emb.vec.clone(),
            });
        }

        Ok(result)
    }

    // ---------------------------------------------------------------------
    // CHAT / COMPLETION
    // ---------------------------------------------------------------------

    /// Genera una respuesta a partir de una pregunta y un contexto
    /// (concatenación de chunks relevantes).
    pub async fn answer_with_context(
        &self,
        question: &str,
        context: &str,
    ) -> Result<String> {
        match self.provider {
            LlmProvider::OpenAI => self.answer_with_openai(question, context).await,
            ref other => Err(anyhow!(
                "Proveedor LLM {:?} aún no implementado para chat",
                other
            )),
        }
    }

    async fn answer_with_openai(
        &self,
        question: &str,
        context: &str,
    ) -> Result<String> {
        use rig::providers::openai;
        // Trait para client.agent(...)
        use rig::client::CompletionClient as _;

        const SYSTEM_PROMPT: &str = r#"
Eres un asistente experto en RAG.
Respondes en español, de forma clara y concisa.
Sólo puedes usar la información suministrada en el contexto. El contexto puede contener texto de documentos y hechos extraídos de un grafo de conocimiento.
Si el contexto no contiene la respuesta, di explícitamente que no la sabes.
"#;

        let client = openai::Client::from_env();

        // Modelo de chat por defecto si no se ha configurado otro
        let model_name = if self.chat_model.is_empty() {
            "gpt-4o-mini"
        } else {
            self.chat_model.as_str()
        };

        let full_context = format!(
            "Contexto:\n{}\n\nPregunta del usuario:\n{}",
            context, question
        );

        let agent = client
            .agent(model_name)
            .preamble(SYSTEM_PROMPT)
            .context(&full_context)
            .build();

        let answer = agent.prompt(question).await?;
        Ok(answer)
    }

    // --- MEJORA: Extracción de Entidades y Relaciones ---
    
    pub async fn extract_entities_and_relations(&self, text: &str) -> Result<ExtractionResult> {
        use rig::providers::openai;
        use rig::client::CompletionClient as _;

        const EXTRACTION_PROMPT: &str = r#"
Tu tarea es analizar el texto y extraer entidades y relaciones para un grafo de conocimiento.
- Identifica y clasifica entidades en una de estas categorías: 'Person', 'Organization', 'Concept', 'Technology'.
- Identifica relaciones entre esas entidades como una tripleta (sujeto, predicado, objeto). El predicado debe ser un identificador conciso en mayúsculas (ej: 'IS_A', 'PART_OF', 'CEO_OF').

La salida DEBE ser un único objeto JSON válido con dos claves: "entities" y "relations".
- "entities": una lista de objetos, cada uno con "id" (nombre de la entidad) y "label".
- "relations": una lista de objetos, cada uno con "subject", "predicate" y "object".

Si no encuentras nada, devuelve listas vacías. No incluyas explicaciones, solo el JSON.
"#;
        let client = openai::Client::from_env();
        let model_name = if self.chat_model.is_empty() { "gpt-4o-mini" } else { self.chat_model.as_str() };

        let agent = client
            .agent(model_name)
            .preamble(EXTRACTION_PROMPT)
            .build();

        let response = agent.prompt(text).await?;
        
        // Limpiar la respuesta del LLM para asegurar que solo contenga el JSON
        let json_response = response
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim();
            
        match serde_json::from_str::<ExtractionResult>(json_response) {
            Ok(result) => Ok(result),
            Err(e) => {
                warn!("No se pudo parsear el JSON de extracción de entidades para un chunk. Error: {}. Respuesta LLM: '{}'", e, response);
                // Devolvemos un resultado vacío en caso de error para no detener la ingesta.
                Ok(ExtractionResult::default())
            }
        }
    }
}
