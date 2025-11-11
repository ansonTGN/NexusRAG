//! Ingesta de un directorio del sistema de archivos en Neo4j, generando el
//! grafo File → Document → Chunk con embeddings y entidades extraídas.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use mime_guess::MimeGuess;
use neo4rs::{query, Graph, Txn};
use pdf_extract;
use tracing::{error, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::{
    app_state::Status,
    llm::{ExtractionResult, LlmManager},
    models::{ChunkNode, DocumentNode, FileNode},
};

/// Resumen de los resultados de una operación de ingesta.
#[derive(Debug, Default)]
pub struct IngestionSummary {
    pub files_scanned: u32,
    pub files_ingested: u32,
    pub files_skipped: u32,
    pub chunks_created: usize,
    pub entities_created: usize,
    pub relations_created: usize,
}

/// Implementa cómo se mostrará el resumen como texto.
impl std::fmt::Display for IngestionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Resumen: {} ficheros escaneados, {} ingeridos, {} omitidos. {} chunks, {} entidades y {} relaciones creadas.",
            self.files_scanned, self.files_ingested, self.files_skipped, self.chunks_created, self.entities_created, self.relations_created
        )
    }
}

/// Recorre recursivamente un directorio, leyendo ficheros de texto,
/// generando documentos y chunks con embeddings y persistiendo la
/// estructura en Neo4j.
pub async fn ingest_directory(
    graph: &Graph,
    llm: &LlmManager,
    root: &Path,
    status_arc: Arc<Mutex<Status>>,
) -> Result<IngestionSummary> {
    if !root.is_dir() {
        return Err(anyhow!(
            "La ruta no es un directorio: {}",
            root.display()
        ));
    }

    let mut summary = IngestionSummary::default();
    let file_entries: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    let total_files = file_entries.len() as f32;

    for (index, entry) in file_entries.iter().enumerate() {
        summary.files_scanned += 1;
        let path = entry.path().to_path_buf();
        let filename_str = path.file_name().unwrap_or_default().to_string_lossy();
        
        let progress = (index + 1) as f32 / total_files;

        {
            let mut status = status_arc.lock().unwrap();
            status.message = format!(
                "[{}/{}] Procesando: {}...",
                index + 1,
                total_files as u32,
                filename_str
            );
            status.progress = progress;
        }

        match ingest_file(graph, llm, &path, status_arc.clone()).await {
            Ok(Some((chunks_count, entities_count, relations_count))) => {
                summary.files_ingested += 1;
                summary.chunks_created += chunks_count;
                summary.entities_created += entities_count;
                summary.relations_created += relations_count;
            }
            Ok(None) => {
                summary.files_skipped += 1;
                 let mut status = status_arc.lock().unwrap();
                 status.message = format!(
                     "[{}/{}] Omitido: {}",
                     index + 1,
                     total_files as u32,
                     filename_str
                 );
                 status.progress = progress;
            }
            Err(err) => {
                summary.files_skipped += 1;
                let error_message = format!("ERROR en {}: {}", path.display(), err);
                error!("Error ingiriendo {}: {err}", path.display());
                {
                    let mut status = status_arc.lock().unwrap();
                    status.message = error_message;
                    status.progress = progress;
                }
            }
        }
    }

    Ok(summary)
}


async fn ingest_file(
    graph: &Graph,
    llm: &LlmManager,
    path: &Path,
    status_arc: Arc<Mutex<Status>>,
) -> Result<Option<(usize, usize, usize)>> {
    let metadata = fs::metadata(path)?;
    let extension = path.extension().and_then(std::ffi::OsStr::to_str).unwrap_or("");

    let text = match extension.to_lowercase().as_str() {
        "pdf" => match pdf_extract::extract_text(path) {
            Ok(content) => content,
            Err(e) => {
                warn!("No se pudo extraer texto del PDF {}: {}. Saltando fichero.", path.display(), e);
                return Ok(None);
            }
        },
        "txt" | "md" | "rs" | "toml" | "log" | "html" | "css" | "js" => match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => {
                warn!("Saltando fichero no-texto o no-UTF8: {}", path.display());
                return Ok(None);
            }
        },
        _ => {
            info!("Saltando fichero con extensión no soportada ('.{}'): {}", extension, path.display());
            return Ok(None);
        }
    };

    let modified: DateTime<Utc> = metadata.modified().ok().map(DateTime::<Utc>::from).unwrap_or_else(Utc::now);
    let path_str = path.to_string_lossy().to_string();
    let filename = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path_str.clone());
    let mime: MimeGuess = MimeGuess::from_path(path);
    let mime_type = mime.first().map(|m| m.to_string());

    let file_node = FileNode {
        id: path_str.clone(),
        path: path_str.clone(),
        filename: filename.clone(),
        size_bytes: metadata.len() as i64,
        modified_at: modified.to_rfc3339(),
        mime_type,
    };

    let doc_node = DocumentNode {
        id: Uuid::new_v4().to_string(),
        title: filename.clone(),
        doc_type: "file".to_string(),
        language: "es".to_string(),
        source: path_str.clone(),
    };

    let raw_chunks = split_into_chunks(&text, 1200);

    if raw_chunks.is_empty() {
        warn!("Fichero vacío o sin texto útil: {}", path.display());
        return Ok(None);
    }
    
    // --- Fase 1: Embeddings ---
    let chunk_pairs: Vec<(String, String)> = raw_chunks.into_iter().map(|txt| (Uuid::new_v4().to_string(), txt)).collect();
    let embedded = llm.embed_chunks(&chunk_pairs).await?;
    let chunk_nodes: Vec<ChunkNode> = embedded.into_iter().enumerate().map(|(idx, emb)| ChunkNode {
            id: emb.id,
            document_id: doc_node.id.clone(),
            index: idx as i64,
            text: emb.text,
            embedding: emb.vector,
            tokens: 0,
    }).collect();
    let chunks_count = chunk_nodes.len();

    // --- MEJORA: Fase 2: Extracción de Entidades y Relaciones ---
    let mut all_extractions = Vec::new();
    for (i, chunk) in chunk_nodes.iter().enumerate() {
        {
            let mut status = status_arc.lock().unwrap();
            status.message = format!("Fichero '{}': Extrayendo conocimiento del chunk {}/{}...", filename, i + 1, chunks_count);
        }
        let extraction = llm.extract_entities_and_relations(&chunk.text).await?;
        all_extractions.push((chunk.id.clone(), extraction));
    }

    let tx = graph.start_txn().await?;
    
    let (entities_count, relations_count) = upsert_graph_data(&tx, &file_node, &doc_node, &chunk_nodes, &all_extractions).await?;

    tx.commit().await?;

    info!("Ingerido {} con {} chunks, {} entidades y {} relaciones.", path.display(), chunks_count, entities_count, relations_count);
    Ok(Some((chunks_count, entities_count, relations_count)))
}

/// Persiste el grafo completo, incluyendo entidades y relaciones.
async fn upsert_graph_data(
    tx: &Txn,
    file: &FileNode,
    doc: &DocumentNode,
    chunks: &[ChunkNode],
    extractions: &[(String, ExtractionResult)],
) -> Result<(usize, usize)> {
    // 1) File
    tx.run(
        query(
            "MERGE (f:File {id: $id})
             SET f.path = $path, f.filename = $filename, f.size_bytes = $size_bytes,
                 f.modified_at = datetime($modified_at), f.mime_type = $mime_type"
        )
        .param("id", file.id.clone()).param("path", file.path.clone())
        .param("filename", file.filename.clone()).param("size_bytes", file.size_bytes)
        .param("modified_at", file.modified_at.clone()).param("mime_type", file.mime_type.clone().unwrap_or_default()),
    ).await?;

    // 2) Document
    tx.run(
        query(
            "MERGE (d:Document {id: $id})
             SET d.title = $title, d.doc_type = $doc_type, d.language = $language, d.source = $source
             WITH d MATCH (f:File {id: $file_id}) MERGE (f)-[:HAS_DOCUMENT]->(d)"
        )
        .param("id", doc.id.clone()).param("title", doc.title.clone())
        .param("doc_type", doc.doc_type.clone()).param("language", doc.language.clone())
        .param("source", doc.source.clone()).param("file_id", file.id.clone()),
    ).await?;

    // 3) Chunks y relaciones NEXT_CHUNK
    let mut prev_chunk_id: Option<String> = None;
    for chunk in chunks {
        tx.run(
            query(
                "MERGE (c:Chunk {id: $id})
                 SET c.index = $index, c.text = $text, c.embedding = $embedding, c.tokens = $tokens
                 WITH c MATCH (d:Document {id: $doc_id}) MERGE (d)-[:HAS_CHUNK]->(c)"
            )
            .param("id", chunk.id.clone()).param("index", chunk.index)
            .param("text", chunk.text.clone()).param("embedding", chunk.embedding.clone())
            .param("tokens", chunk.tokens).param("doc_id", chunk.document_id.clone()),
        ).await?;

        if let Some(prev_id) = &prev_chunk_id {
            tx.run(
                query("MATCH (c1:Chunk {id: $prev_id}), (c2:Chunk {id: $id}) MERGE (c1)-[:NEXT_CHUNK]->(c2)")
                .param("prev_id", prev_id.clone()).param("id", chunk.id.clone()),
            ).await?;
        }
        prev_chunk_id = Some(chunk.id.clone());
    }

    // --- Persistir entidades, menciones y relaciones ---
    let mut unique_entities = HashMap::new();
    let mut unique_relations = HashSet::new();

    for (_, extraction) in extractions {
        for entity in &extraction.entities {
            unique_entities.insert(entity.id.clone(), entity.label.clone());
        }
        for rel in &extraction.relations {
            unique_relations.insert((rel.subject.clone(), rel.predicate.clone(), rel.object.clone()));
        }
    }

    // 4) Crear nodos de Entidad
    for (id, label) in &unique_entities {
        let cypher = format!("MERGE (e:Entity:`{}` {{id: $id}})", label);
        tx.run(query(&cypher).param("id", id.clone())).await?;
    }

    // 5) Crear relaciones (Chunk)-[:MENTIONS]->(Entity)
    for (chunk_id, extraction) in extractions {
        for entity in &extraction.entities {
            tx.run(
                query("MATCH (c:Chunk {id: $cid}), (e:Entity {id: $eid}) MERGE (c)-[:MENTIONS]->(e)")
                .param("cid", chunk_id.clone())
                .param("eid", entity.id.clone()),
            ).await?;
        }
    }

    // 6) Crear relaciones (Entity)-[:RELATED_TO {type}]->(Entity)
    for (subject, predicate, object) in &unique_relations {
        tx.run(
            query("MATCH (s:Entity {id: $subj}), (o:Entity {id: $obj}) MERGE (s)-[r:RELATED_TO {type: $pred}]->(o)")
            .param("subj", subject.clone())
            .param("obj", object.clone())
            .param("pred", predicate.clone()),
        ).await?;
    }

    Ok((unique_entities.len(), unique_relations.len()))
}


fn split_into_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let paragraphs: Vec<&str> = text.split("\n\n").collect();

    for paragraph in paragraphs {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() { continue; }
        if current.len() + paragraph.len() + 2 > max_chars && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }
        if !current.is_empty() { current.push_str("\n\n"); }
        current.push_str(paragraph);
    }
    if !current.is_empty() { chunks.push(current); }
    chunks
}