use std::path::{Path, PathBuf};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use neo4rs::{query, Node, Relation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::spawn;
use tracing::{error, info};
use url::Url;

use crate::{
    app_state::{AppState, Status},
    ingest, models::FileTreeNode, rag,
};

// --- Payloads y Respuestas de la API (MODIFICADO) ---

#[derive(Deserialize)]
pub struct SelectDirPayload {
    path: String,
}

#[derive(Deserialize)]
pub struct RagQueryPayload {
    question: String,
}

// MEJORA: La respuesta ahora incluye la respuesta y las entidades clave.
#[derive(Serialize)]
pub struct RagQueryResponse {
    answer: String,
    key_entities: Vec<String>,
}

// MEJORA: Estructura para la lista de entidades.
#[derive(Serialize, Deserialize)]
pub struct EntityInfo {
    id: String,
    label: String,
}

// MEJORA: Estructuras para la visualización del grafo.
#[derive(Serialize, Clone)]
pub struct GraphNode {
    id: String,
    label: String,
    group: String,
}

#[derive(Serialize)]
pub struct GraphEdge {
    source: String,
    target: String,
    label: String,
}

#[derive(Serialize)]
pub struct GraphData {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}


// --- Router ---

pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/api/list-directory", post(list_directory_handler))
        .route("/api/select-directory", post(select_directory_handler))
        .route("/api/ingest", post(ingest_handler))
        .route("/api/rag-query", post(rag_query_handler))
        .route("/api/status", get(status_handler))
        .route("/api/neo4j-info", get(neo4j_info_handler))
        .route("/api/shutdown", post(shutdown_handler))
        // MEJORA: Nuevos endpoints para el frontend interactivo.
        .route("/api/entities", get(list_entities_handler))
        .route("/api/graph-data", get(graph_data_handler))
        .with_state(app_state)
}

// --- Handlers ---

// ... (list_directory_handler, select_directory_handler, ingest_handler no cambian) ...

#[axum::debug_handler]
async fn list_directory_handler(
    Json(payload): Json<SelectDirPayload>,
) -> Result<Json<FileTreeNode>, (StatusCode, Json<serde_json::Value>)> {
    let path = if payload.path.is_empty() {
        dirs::home_dir().ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "No se pudo determinar el directorio home del usuario."})),
            )
        })?
    } else {
        PathBuf::from(&payload.path)
    };

    if !path.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "La ruta proporcionada no es un directorio válido."})),
        ));
    }

    match build_file_tree(&path) {
        Ok(tree) => Ok(Json(tree)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Error al leer el directorio: {}", e)})),
        )),
    }
}

#[axum::debug_handler]
async fn select_directory_handler(
    State(state): State<AppState>,
    Json(payload): Json<SelectDirPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let path = PathBuf::from(&payload.path);
    if !path.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "La ruta proporcionada no es un directorio válido."})),
        ));
    }

    *state.current_dir.lock().unwrap() = Some(path);
    Ok((StatusCode::OK, Json(json!({ "message": "Directorio fijado para la ingesta." }))))
}

#[axum::debug_handler]
async fn ingest_handler(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let root_dir = match state.current_dir.lock().unwrap().clone() {
        Some(dir) => dir,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "Primero debe seleccionar un directorio."})),
            ));
        }
    };
    
    spawn(async move {
        {
            let mut status = state.status.lock().unwrap();
            status.is_busy = true;
            status.message = "Iniciando indexación...".to_string();
            status.progress = 0.0;
        }

        let result = ingest::ingest_directory(
            &state.graph,
            &state.llm_manager,
            &root_dir,
            state.status.clone(),
        ).await;

        let mut status = state.status.lock().unwrap();
        status.is_busy = false;
        status.progress = 0.0;
        match result {
            Ok(summary) => {
                status.message = format!("¡Indexación completada! {}", summary);
            }
            Err(err) => {
                status.message = format!("Error en la indexación: {}", err);
                error!("Error de ingesta: {}", err);
            }
        }
    });

    Ok(StatusCode::ACCEPTED)
}

// MODIFICADO: Adaptado para devolver la nueva estructura RagQueryResponse.
#[axum::debug_handler]
async fn rag_query_handler(
    State(state): State<AppState>,
    Json(payload): Json<RagQueryPayload>,
) -> Result<Json<RagQueryResponse>, (StatusCode, Json<serde_json::Value>)> {
    let rag_result = rag::rag_query(
        &state.graph,
        &state.llm_manager,
        &state.config,
        &payload.question,
        5,
    )
    .await;

    match rag_result {
        Ok((answer, key_entities)) => Ok(Json(RagQueryResponse {
            answer,
            key_entities,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Error al procesar la consulta RAG: {}", e)})),
        )),
    }
}

#[axum::debug_handler]
async fn status_handler(State(state): State<AppState>) -> Json<Status> {
    Json(state.status.lock().unwrap().clone())
}

#[axum::debug_handler]
async fn neo4j_info_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let browser_url = match Url::parse(&state.config.neo4j_uri) {
        Ok(mut url) => {
            let _ = url.set_scheme("http");
            let _ = url.set_port(Some(7474));
            url.to_string()
        }
        Err(_) => "http://localhost:7474".to_string(),
    };

    match state.graph.run(query("RETURN 1")).await {
        Ok(_) => Ok(Json(json!({ "status": "ok", "browser_url": browser_url }))),
        Err(e) => {
            error!("Error en el health check de Neo4j: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// --- MEJORA: Nuevos Handlers para el Grafo de Conocimiento ---

#[axum::debug_handler]
async fn list_entities_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<EntityInfo>>, StatusCode> {
    let mut cursor = state.graph.execute(
        query("MATCH (e:Entity) RETURN DISTINCT e.id AS id, labels(e)[1] AS label ORDER BY id")
    ).await.map_err(|e| {
        error!("Error consultando entidades: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut entities = Vec::new();
    while let Some(row) = cursor.next().await.map_err(|e| {
        error!("Error iterando sobre entidades: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        if let (Some(id), Some(label)) = (row.get("id"), row.get("label")) {
            entities.push(EntityInfo { id, label });
        }
    }
    Ok(Json(entities))
}

#[axum::debug_handler]
async fn graph_data_handler(
    State(state): State<AppState>,
) -> Result<Json<GraphData>, StatusCode> {
    let mut cursor = state.graph.execute(
        query("MATCH (e1:Entity)-[r:RELATED_TO]->(e2:Entity) RETURN e1, r, e2 LIMIT 50")
    ).await.map_err(|e| {
        error!("Error consultando datos del grafo: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut nodes = std::collections::HashMap::new();
    let mut edges = Vec::new();

    while let Some(row) = cursor.next().await.map_err(|e| {
        error!("Error iterando sobre datos del grafo: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })? {
        if let (Some(e1_node), Some(r_rel), Some(e2_node)) = (row.get::<Node>("e1"), row.get::<Relation>("r"), row.get::<Node>("e2")) {
            let e1_id: String = e1_node.get("id").unwrap_or_default();
            let e2_id: String = e2_node.get("id").unwrap_or_default();
            
            if !nodes.contains_key(&e1_id) {
                let e1_label: String = e1_node.labels().get(1).cloned().unwrap_or_else(|| "Entity".to_string());
                nodes.insert(e1_id.clone(), GraphNode { id: e1_id.clone(), label: e1_id.clone(), group: e1_label });
            }

            if !nodes.contains_key(&e2_id) {
                 let e2_label: String = e2_node.labels().get(1).cloned().unwrap_or_else(|| "Entity".to_string());
                nodes.insert(e2_id.clone(), GraphNode { id: e2_id.clone(), label: e2_id.clone(), group: e2_label });
            }

            edges.push(GraphEdge {
                source: e1_id,
                target: e2_id,
                label: r_rel.typ(),
            });
        }
    }

    let graph_data = GraphData {
        nodes: nodes.into_values().collect(),
        edges,
    };

    Ok(Json(graph_data))
}


// --- Handler de Apagado y Utilidades ---

#[axum::debug_handler]
async fn shutdown_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("Petición de apagado recibida.");
    if let Some(sender) = state.shutdown_sender.lock().unwrap().take() {
        let _ = sender.send(());
    }
    StatusCode::OK
}

fn build_file_tree(path: &Path) -> std::io::Result<FileTreeNode> {
    let metadata = std::fs::metadata(path)?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    
    let is_dir = metadata.is_dir();
    let mut children = Vec::new();

    if is_dir {
        let mut entries: Vec<_> = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .collect();

        entries.sort_by(|a, b| {
            let a_is_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let b_is_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            b_is_dir.cmp(&a_is_dir).then_with(|| a.file_name().cmp(&b.file_name()))
        });

        for entry in entries {
            if let Ok(entry_meta) = entry.metadata() {
                children.push(FileTreeNode {
                    path: entry.path(),
                    name: entry.file_name().to_string_lossy().to_string(),
                    is_dir: entry_meta.is_dir(),
                    children: Vec::new(),
                });
            }
        }
    }

    Ok(FileTreeNode {
        path: path.to_path_buf(),
        name,
        is_dir,
        children,
    })
}