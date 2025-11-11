//! Consulta RAG contra Neo4j usando rig-neo4j como vector store.
//!
//! Flujo Mejorado (Graph-RAG):
//!   1. Búsqueda vectorial sobre :Chunk(embedding) para encontrar puntos de entrada.
//!   2. Expansión en el grafo desde los chunks recuperados para encontrar entidades
//!      y relaciones relevantes.
//!   3. Construcción de un contexto aumentado (texto de chunks + conocimiento del grafo).
//!   4. El LLM responde usando este contexto enriquecido.
//!   5. Se registra la consulta en el grafo.

use anyhow::Result;
use chrono::Utc;
use neo4rs::{query, Graph};
use std::collections::HashSet;
use uuid::Uuid;

use crate::{
    config::AppConfig,
    llm::LlmManager,
    models::QueryNode,
    vector_store::{self},
};

/// Lanza una consulta RAG:
/// - Usa `rig-neo4j` para recuperar los `top_k` chunks más relevantes.
/// - Llama al LLM con el contexto concatenado.
/// - Registra la consulta en Neo4j.
/// - MODIFICADO: Devuelve la respuesta y una lista de entidades clave.
pub async fn rag_query(
    graph: &Graph,
    llm: &LlmManager,
    cfg: &AppConfig,
    question: &str,
    top_k: usize,
) -> Result<(String, Vec<String>)> {
    // 1) Buscar top_k chunks vía vector store (puntos de entrada al grafo)
    let results = vector_store::search_top_chunks(cfg, question, top_k).await?;

    if results.is_empty() {
        return Ok((
            "No se encontró información relevante en los documentos para responder a esta pregunta.".to_string(),
            Vec::new()
        ));
    }

    let mut chunk_texts = Vec::new();
    let mut chunk_ids = Vec::new();
    let mut matches: Vec<(String, f64)> = Vec::new();

    for (score, id, doc) in results {
        chunk_texts.push(doc.text);
        chunk_ids.push(id.clone());
        matches.push((id, score));
    }
    
    let raw_text_context = chunk_texts.join("\n\n---\n\n");

    // MEJORA: 2) Expansión en el grafo y construcción de contexto aumentado.
    let (graph_context, key_entities) = build_context_from_graph(graph, &chunk_ids).await?;
    
    let full_context = if graph_context.is_empty() {
        raw_text_context
    } else {
        format!(
            "**Información de Documentos:**\n{}\n\n**Conocimiento Relevante del Grafo:**\n{}",
            raw_text_context,
            graph_context
        )
    };

    // 3) Registrar Query y relaciones MATCHED_CHUNK
    let query_id = Uuid::new_v4().to_string();
    let query_node = QueryNode {
        id: query_id.clone(),
        question: question.to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    log_query(graph, &query_node, &matches).await?;

    // 4) Preguntar al LLM con contexto aumentado
    let answer = llm.answer_with_context(question, &full_context).await?;
    
    // 5) Devolver la respuesta y las entidades encontradas
    let entities_vec = key_entities.into_iter().collect();
    Ok((answer, entities_vec))
}

/// MEJORA: A partir de un conjunto de IDs de chunks, explora el grafo de conocimiento
/// para encontrar entidades y relaciones conectadas, y lo formatea como texto.
/// MODIFICADO: Ahora devuelve el contexto y el conjunto de entidades encontradas.
async fn build_context_from_graph(graph: &Graph, chunk_ids: &[String]) -> Result<(String, HashSet<String>)> {
    let mut cursor = graph.execute(query(
        "MATCH (chunk:Chunk) WHERE elementId(chunk) IN $chunk_ids
         WITH chunk
         OPTIONAL MATCH (chunk)-[:MENTIONS]->(e1:Entity)
         WITH collect(DISTINCT e1) as entities
         UNWIND entities as e1
         OPTIONAL MATCH (e1)-[r:RELATED_TO]-(e2:Entity)
         WHERE e2 in entities
         RETURN e1.id as entity1, r.type as rel_type, e2.id as entity2"
    ).param("chunk_ids", chunk_ids.to_vec())).await?;

    let mut entities = HashSet::new();
    let mut relations = HashSet::new();

    while let Some(row) = cursor.next().await? {
        if let Some(e1) = row.get::<String>("entity1") {
            entities.insert(e1);
        }
        
        if let (Some(e1), Some(rel), Some(e2)) = (
            row.get::<String>("entity1"),
            row.get::<String>("rel_type"),
            row.get::<String>("entity2"),
        ) {
            if e1 < e2 {
                relations.insert(format!("- {} {} {}", e1, rel, e2));
            } else {
                relations.insert(format!("- {} {} {}", e2, rel, e1));
            }
        }
    }

    let mut context = String::new();
    if !entities.is_empty() {
        context.push_str("Se han identificado los siguientes conceptos clave: ");
        let entity_list: Vec<String> = entities.iter().cloned().collect();
        context.push_str(&entity_list.join(", "));
        context.push_str(".\n");
    }

    if !relations.is_empty() {
        context.push_str("\nSe han encontrado estas relaciones entre ellos:\n");
        let relation_list: Vec<String> = relations.into_iter().collect();
        context.push_str(&relation_list.join("\n"));
    }

    Ok((context, entities))
}

async fn log_query(
    graph: &Graph,
    query_node: &QueryNode,
    matches: &[(String, f64)],
) -> Result<()> {
    // Crear nodo :Query
    graph.run(
        query("MERGE (q:Query {id: $id}) SET q.question = $question, q.created_at = datetime($created_at)")
        .param("id", query_node.id.clone())
        .param("question", query_node.question.clone())
        .param("created_at", query_node.created_at.clone()),
    ).await?;

    // Crear relaciones :MATCHED_CHUNK
    for (chunk_id, score) in matches {
        graph.run(
            query("MATCH (q:Query {id: $qid}), (c:Chunk) WHERE elementId(c) = $cid
                   MERGE (q)-[r:MATCHED_CHUNK]->(c) SET r.score = $score")
            .param("qid", query_node.id.clone())
            .param("cid", chunk_id.clone())
            .param("score", *score),
        ).await?;
    }

    Ok(())
}