# NexusRAG: Inteligencia Aumentada sobre Grafos de Conocimiento

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.78+-orange.svg)](https://www.rust-lang.org/)
[![Neo4j](https://img.shields.io/badge/Neo4j-5-blue.svg)](https://neo4j.com/)
[![React](https://img.shields.io/badge/Frontend-Vanilla_JS-yellow.svg)](#)

**NexusRAG** es un sistema avanzado de **Recuperación Aumentada por Generación (RAG)** que va más allá de la simple búsqueda vectorial. En lugar de tratar los documentos como silos de información aislados, NexusRAG construye un **Grafo de Conocimiento** dinámico en Neo4j, extrayendo entidades y sus relaciones directamente del texto. Esto permite que las respuestas generadas por el LLM no solo se basen en fragmentos de texto relevantes, sino también en el contexto estructural de cómo se conectan los conceptos clave entre sí.

El proyecto está construido con un backend de alto rendimiento en **Rust (Axum)** y una interfaz web moderna e interactiva en Vanilla JS que permite explorar tanto los documentos como el grafo de conocimiento resultante.

  <!-- Reemplazar con una captura de pantalla real -->

---

## 核心概念：Del RAG tradicional al Graph-RAG

El enfoque de NexusRAG enriquece el proceso RAG tradicional añadiendo una capa de inteligencia estructural.

**Flujo de Ingesta:**
1.  **Análisis de Ficheros:** Se procesan ficheros locales (`.txt`, `.md`, `.pdf`, etc.).
2.  **División en Chunks:** Cada documento se divide en fragmentos de texto manejables (chunks).
3.  **Extracción de Conocimiento:** Un LLM (ej. GPT-4o-mini) analiza cada chunk para:
    *   Identificar **entidades** (Personas, Conceptos, Tecnologías...).
    *   Determinar las **relaciones** entre estas entidades.
4.  **Generación de Embeddings:** Se calculan embeddings vectoriales para cada chunk de texto para la búsqueda semántica.
5.  **Persistencia en Neo4j:** Se construye un grafo rico que modela:
    *   `(:File) -[:HAS_DOCUMENT]-> (:Document)`
    *   `(:Document) -[:HAS_CHUNK]-> (:Chunk)`
    *   `(:Chunk) -[:MENTIONS]-> (:Entity)`
    *   `(:Entity) -[:RELATED_TO]-> (:Entity)`
    *   El embedding se almacena como una propiedad en el nodo `:Chunk`.

**Flujo de Consulta (Graph-RAG):**
1.  **Búsqueda Vectorial:** La pregunta del usuario se convierte en un vector y se utiliza para encontrar los `:Chunk`s más relevantes en Neo4j.
2.  **Expansión del Grafo:** A partir de los chunks encontrados, la consulta se expande por el grafo para recopilar entidades (`:Entity`) y relaciones (`:RELATED_TO`) conectadas directamente.
3.  **Construcción de Contexto Aumentado:** El contexto que se envía al LLM contiene dos partes:
    *   El texto plano de los chunks relevantes.
    *   Una descripción textual del conocimiento extraído del grafo (ej. "Conceptos clave: Ley de Moore, IA. Relaciones: Ley de Moore IMPULSA IA").
4.  **Generación de Respuesta:** El LLM utiliza este contexto enriquecido para generar una respuesta mucho más completa y contextualizada.

## ✨ Características Principales

*   **Backend en Rust:** Asíncrono, seguro y de alto rendimiento con Axum y Tokio.
*   **Base de Datos de Grafo y Vectorial:** Utiliza Neo4j tanto para almacenar la estructura del grafo de conocimiento como para realizar búsquedas vectoriales nativas.
*   **Extracción Automática de Conocimiento:** Usa modelos de lenguaje avanzados para identificar y relacionar conceptos clave sin necesidad de anotación manual.
*   **Interfaz de Usuario Interactiva:**
    *   Explorador de archivos para seleccionar directorios locales.
    *   Monitorización del estado de la ingesta en tiempo real con barra de progreso.
    *   Chat para realizar consultas RAG.
    *   Visor de entidades descubiertas para explorar los conceptos del grafo.
    *   **Visualizador del grafo de conocimiento** interactivo (usando Cytoscape.js).
*   **Abstracción de LLM:** Integración sencilla con proveedores de LLM (actualmente OpenAI) a través de la librería `rig`.
*   **Configuración Sencilla:** Gestionado a través de un único fichero `.env`.

## 🛠️ Pila Tecnológica

*   **Backend:** Rust, Tokio, Axum
*   **Base de Datos:** Neo4j (con índices vectoriales)
*   **IA / LLM:** OpenAI (GPT-4o-mini, text-embedding-3-small) a través de `rig-core`
*   **Frontend:** Vanilla JavaScript (ESM), HTML5, CSS3
*   **Visualización de Grafos:** Cytoscape.js

## 🚀 Puesta en Marcha

Sigue estos pasos para ejecutar NexusRAG en tu máquina local.

### Prerrequisitos

*   **Rust:** Instala la toolchain de Rust desde [rustup.rs](https://rustup.rs/).
*   **Neo4j:** La forma más sencilla es usar Docker.
    ```bash
    docker run -d \
        --name neo4j-nexus \
        -p 7474:7474 -p 7687:7687 \
        -e NEO4J_AUTH=neo4j/tu_contraseña_segura \
        -e NEO4J_PLUGINS='["apoc", "graph-data-science"]' \
        neo4j:5-enterprise
    ```
    *(La edición Enterprise es necesaria para los índices vectoriales en algunas versiones. Puedes usarla gratis para desarrollo local)*.
*   **Clave de API de OpenAI:** Necesitas una cuenta de OpenAI con crédito disponible.

### Instalación y Configuración

1.  **Clona el repositorio:**
    ```bash
    git clone https://github.com/tu_usuario/NexusRAG.git
    cd NexusRAG
    ```

2.  **Crea el fichero de entorno:**
    Copia el contenido de `00-libro.txt` que corresponde a `.env` y crea un fichero llamado `.env` en la raíz del proyecto.

    ```dotenv
    # .env
    NEO4J_URI=neo4j://localhost:7687
    NEO4J_USER=neo4j
    NEO4J_PASSWORD=tu_contraseña_segura  # La que pusiste en el comando de Docker
    SERVER_ADDR=127.0.0.1:3322
    OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxx  # Tu clave real de OpenAI
    LLM_PROVIDER=openai
    LLM_EMBEDDING_MODEL=text-embedding-3-small
    LLM_CHAT_MODEL=gpt-4o-mini
    ```

3.  **Compila y ejecuta el proyecto:**
    El servidor se compilará y se iniciará. La primera vez puede tardar un poco.

    ```bash
    cargo run --release
    ```

4.  **Abre la aplicación:**
    Una vez que veas el mensaje `🚀 Servidor escuchando en http://127.0.0.1:3322`, tu navegador por defecto debería abrir la aplicación automáticamente. Si no lo hace, abre la URL manualmente.

## 📖 Guía de Uso

1.  **Selecciona un Directorio:** Pega la ruta a un directorio local que contenga los ficheros que quieres analizar (`.txt`, `.md`, `.pdf`...) y pulsa **"Cargar"**.
2.  **Navega y Fija el Directorio:** Haz clic sobre el nombre del directorio que quieres procesar en el árbol de archivos. El botón de ingesta se activará.
3.  **Inicia la Indexación:** Pulsa **"Iniciar Indexación en Neo4j"**. El sistema comenzará a procesar los ficheros. Puedes ver el progreso en la barra de estado inferior.
4.  **Explora el Conocimiento:**
    *   Una vez finalizada la ingesta, la lista de **"Entidades Descubiertas"** y el **"Explorador del Grafo"** se poblarán. Puedes refrescarlos manualmente con el botón 🔄.
    *   Haz clic en una entidad para auto-rellenar una pregunta sobre ella.
5.  **Realiza una Consulta RAG:** Escribe tu pregunta en el área de texto y pulsa **"Enviar Consulta"**. La respuesta generada, junto con las entidades clave identificadas en el texto, aparecerá en el panel de resultados.

## 📂 Estructura del Proyecto

```
/
├── frontend/             # Ficheros estáticos de la interfaz web
│   ├── css/styles.css
│   ├── js/main.js
│   └── index.html
├── src/                  # Código fuente del backend en Rust
│   ├── api.rs            # Endpoints de la API (Axum)
│   ├── app_state.rs      # Estructura del estado compartido
│   ├── config.rs         # Carga y gestión de la configuración
│   ├── ingest.rs         # Lógica de ingesta y procesamiento de ficheros
│   ├── llm.rs            # Abstracción para interactuar con LLMs
│   ├── models.rs         # Modelos de datos del dominio (nodos del grafo)
│   ├── neo4j_client.rs   # Conexión y gestión del esquema de Neo4j
│   ├── rag.rs            # Lógica principal del Graph-RAG
│   ├── vector_store.rs   # Funciones para el índice vectorial de Neo4j
│   └── main.rs           # Punto de entrada de la aplicación
├── .env                  # Fichero de configuración (NO incluir en git)
├── Cargo.toml            # Manifiesto del proyecto Rust
└── README.md             # Este fichero```

## 📄 Licencia

Este proyecto está bajo la Licencia MIT. Ver el fichero `LICENSE` para más detalles.

---
*Diseñado por Ángel A. Urbina Sánchez*