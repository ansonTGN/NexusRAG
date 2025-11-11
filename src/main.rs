// Módulos de la aplicación
mod api;
mod app_state;
mod config;
mod ingest;
mod llm;
mod models;
mod neo4j_client;
mod rag;
mod vector_store;

use crate::app_state::{AppState, Status};
use axum::Router;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // 1. Cargar .env e inicializar logging
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // 2. Cargar configuración
    let cfg = config::AppConfig::from_env().expect("Error al cargar la configuración");

    // 3. Conectar a Neo4j y asegurar esquemas
    let graph = neo4j_client::connect_from_config(&cfg)
        .await
        .expect("Error conectando a Neo4j");
    neo4j_client::ensure_schema(&graph)
        .await
        .expect("Error asegurando el esquema de Neo4j");
    vector_store::ensure_chunk_vector_index(&cfg)
        .await
        .expect("Error asegurando el índice vectorial");

    // 4. Inicializar gestor de LLMs
    let llm_manager = llm::LlmManager::from_config(&cfg).expect("Error inicializando LLM Manager");

    // Crear canal para la señal de apagado.
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // 5. Crear estado compartido de la aplicación
    let app_state = AppState {
        config: cfg.clone(),
        graph: Arc::new(graph),
        llm_manager,
        status: Arc::new(Mutex::new(Status {
            is_busy: false,
            message: "Servidor listo.".to_string(),
            progress: 0.0, // MODIFICADO AQUÍ: Añadido el campo faltante.
        })),
        current_dir: Arc::new(Mutex::new(None)),
        shutdown_sender: Arc::new(Mutex::new(Some(shutdown_tx))),
    };

    // 6. Configurar el router de la API y el servicio de ficheros estáticos
    let app = Router::new()
        .nest("/", api::create_router(app_state.clone()))
        .fallback_service(ServeDir::new("frontend"))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // 7. Iniciar el servidor
    let server_addr = &app_state.config.server_addr;
    let listener = tokio::net::TcpListener::bind(server_addr)
        .await
        .unwrap();
    let server_url = format!("http://{}", server_addr);
    info!("🚀 Servidor escuchando en {}", &server_url);

    // Abrir el frontend en el navegador por defecto
    if webbrowser::open(&server_url).is_err() {
        info!("No se pudo abrir el navegador. Por favor, accede a {} manualmente.", server_url);
    }
    
    // Configurar el apagado ordenado.
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
            info!("Señal de apagado recibida, iniciando cierre del servidor.");
        })
        .await
        .unwrap();

    info!("✅ Servidor cerrado correctamente.");
}
