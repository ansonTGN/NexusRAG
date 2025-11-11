//! Integración con Neo4j como vector store para los `:Chunk`.
//!
//! API pública:
//!   - `ensure_chunk_vector_index(&AppConfig)`
//!   - `search_top_chunks(&AppConfig, &str, usize)`.

use anyhow::{anyhow, Result};
use neo4rs::query;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::config::AppConfig;
use crate::neo4j_client;

/// Documento mínimo que representa un :Chunk con texto y vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDoc {
    // CORREGIDO: El ID que se devuelve es el elementId, no una propiedad.
    // pub id: String, 
    pub text: String,
    pub embedding: Vec<f64>,
}

/// Garantiza que el índice vectorial sobre `:Chunk(embedding)` exista.
pub async fn ensure_chunk_vector_index(cfg: &AppConfig) -> Result<()> {
    let graph = neo4j_client::connect_from_config(cfg).await?;
    let index_name = "chunkEmbeddingIndex";

    // ¿Ya existe el índice? Usamos la sintaxis moderna SHOW VECTOR INDEXES.
    let mut cursor = graph
        .execute(
            query("SHOW VECTOR INDEXES YIELD name WHERE name = $name RETURN name")
            .param("name", index_name),
        )
        .await?;

    if cursor.next().await?.is_some() {
        info!("Índice vectorial '{index_name}' ya existe.");
        return Ok(());
    }

    // Crear índice vectorial para :Chunk(embedding)
    let cypher = format!(
        "\
CREATE VECTOR INDEX {index_name}
FOR (c:Chunk)
ON (c.embedding)
OPTIONS {{
  indexConfig: {{
    `vector.dimensions`: 1536,
    `vector.similarity_function`: 'cosine'
  }}
}}",
        index_name = index_name
    );

    graph.run(query(&cypher)).await?;
    info!("Índice vectorial '{index_name}' creado.");

    Ok(())
}

/// Realiza una búsqueda vectorial (semantic search) sobre los embeddings
/// almacenados en `:Chunk(embedding)`.
pub async fn search_top_chunks(
    cfg: &AppConfig,
    query_text: &str,
    top_k: usize,
) -> Result<Vec<(f64, String, ChunkDoc)>> {
    use rig::providers::openai::{self, TEXT_EMBEDDING_3_SMALL};
    use rig::client::EmbeddingsClient as _;
    use rig::embeddings::EmbeddingModel as _;

    if !matches!(cfg.llm_provider, crate::config::LlmProvider::OpenAI) {
        return Err(anyhow!( "search_top_chunks sólo está implementado para OpenAI por ahora"));
    }

    // 1) Embedding de la query
    let client = openai::Client::from_env();
    let model_name = if cfg.llm_embedding_model.is_empty() { TEXT_EMBEDDING_3_SMALL } else { cfg.llm_embedding_model.as_str() };
    let embedding_model = client.embedding_model(model_name);
    let embeddings = embedding_model.embed_texts(vec![query_text.to_string()]).await?;
    let query_vec = embeddings.get(0).map(|e| e.vec.clone()).ok_or_else(|| anyhow!("No se pudo generar embedding de la query"))?;

    // 2) Vector search en Neo4j
    let graph = neo4j_client::connect_from_config(cfg).await?;
    let mut cursor = graph.execute(
        query(
            "CALL db.index.vector.queryNodes($index_name, $k, $embedding)
             YIELD node, score
             RETURN elementId(node) AS id, score, node.text AS text, node.embedding AS embedding
             ORDER BY score DESC"
        )
        .param("index_name", "chunkEmbeddingIndex")
        .param("k", top_k as i64)
        .param("embedding", query_vec.clone()),
    ).await?;

    // 3) Convertir resultados a (score, id, ChunkDoc)
    let mut output = Vec::new();
    while let Some(row) = cursor.next().await? {
        let id: String = row.get("id").ok_or_else(|| anyhow!("Falta campo 'id' en resultado de Neo4j"))?;
        let score: f64 = row.get("score").ok_or_else(|| anyhow!("Falta campo 'score' en resultado de Neo4j"))?;
        let text: String = row.get("text").ok_or_else(|| anyhow!("Falta campo 'text' en resultado de Neo4j"))?;
        let embedding: Vec<f64> = row.get("embedding").ok_or_else(|| anyhow!("Falta campo 'embedding' en resultado de Neo4j"))?;

        let doc = ChunkDoc { text, embedding };
        output.push((score, id, doc));
    }

    Ok(output)
}