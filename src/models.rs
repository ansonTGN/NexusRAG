//! Modelos de dominio (metadatos de ficheros y nodos del grafo Neo4j).

use serde::Serialize;
use std::path::PathBuf;

/// Representa un nodo (:File) en Neo4j.
/// Contiene metadatos básicos del fichero en el sistema de archivos.
#[derive(Debug, Clone)]
pub struct FileNode {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub modified_at: String,
    pub mime_type: Option<String>,
}

/// Representa un nodo (:Document) en Neo4j.
/// Actúa como un contenedor lógico para los trozos de texto (chunks).
#[derive(Debug, Clone)]
pub struct DocumentNode {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub language: String,
    pub source: String,
}

/// Representa un nodo (:Chunk) en Neo4j.
/// Es un trozo de texto con su correspondiente embedding vectorial.
#[derive(Debug, Clone)]
pub struct ChunkNode {
    pub id: String,
    pub document_id: String,
    pub index: i64,
    pub text: String,
    pub embedding: Vec<f64>,
    pub tokens: i64,
    // pub section: Option<String>, // <-- LÍNEA ELIMINADA
}

/// Representa un nodo (:Query) para registrar las consultas RAG realizadas.
#[derive(Debug, Clone)]
pub struct QueryNode {
    pub id: String,
    pub question: String,
    pub created_at: String,
}

/// MEJORA: Representa un nodo de entidad (:Entity) extraído del texto.
#[derive(Debug, Clone)]
pub struct EntityNode {
    pub id: String,   // ej: "Ley de Moore"
    pub label: String, // ej: "Concept"
}

// Esta es la única definición de FileTreeNode.
#[derive(Debug, Clone, Serialize)]
pub struct FileTreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
}