use crate::config::AppConfig;
use anyhow::Result;
use neo4rs::{query, Graph};
use tracing::info;
use url::Url;

pub async fn connect_from_config(cfg: &AppConfig) -> Result<Graph> {
    let url = Url::parse(&cfg.neo4j_uri)?;
    let host = url.host_str().unwrap_or("localhost");
    let port = url.port().unwrap_or(7687);
    let addr = format!("{host}:{port}");

    info!("Conectando a Neo4j en {addr}...");
    let graph = Graph::new(&addr, &cfg.neo4j_user, &cfg.neo4j_password).await?;
    info!("Conexión a Neo4j OK");
    Ok(graph)
}

/// Crea constraints básicos para las etiquetas usadas en el grafo:
/// :File, :Document, :Chunk, :Query, y el nuevo :Entity
pub async fn ensure_schema(graph: &Graph) -> Result<()> {
    let statements = [
        // File.id único
        "CREATE CONSTRAINT file_id IF NOT EXISTS
         FOR (f:File)
         REQUIRE f.id IS UNIQUE",
        // Document.id único
        "CREATE CONSTRAINT doc_id IF NOT EXISTS
         FOR (d:Document)
         REQUIRE d.id IS UNIQUE",
        // Chunk.id único
        "CREATE CONSTRAINT chunk_id IF NOT EXISTS
         FOR (c:Chunk)
         REQUIRE c.id IS UNIQUE",
        // Query.id único
        "CREATE CONSTRAINT query_id IF NOT EXISTS
         FOR (q:Query)
         REQUIRE q.id IS UNIQUE",
        // MEJORA: Constraint para los nodos de entidad extraídos.
        "CREATE CONSTRAINT entity_id IF NOT EXISTS
         FOR (e:Entity)
         REQUIRE e.id IS UNIQUE",
    ];

    for stmt in statements {
        graph.run(query(stmt)).await?;
    }

    info!("Esquema de Neo4j asegurado (constraints básicos creados).");
    Ok(())
}
