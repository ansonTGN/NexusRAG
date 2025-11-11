document.addEventListener('DOMContentLoaded', () => {
    // --- Elementos del DOM ---
    const dirForm = document.getElementById('dir-form');
    const dirPathInput = document.getElementById('dir-path');
    const fileTreeContainer = document.getElementById('file-tree-container');
    const ingestBtn = document.getElementById('ingest-btn');
    const ragForm = document.getElementById('rag-form');
    const questionInput = document.getElementById('question');
    const ragBtn = document.getElementById('rag-btn');
    const answerContainer = document.getElementById('answer-container');
    const copyBtn = document.getElementById('copy-btn');
    const statusText = document.getElementById('status-text');
    const spinner = document.getElementById('spinner');
    const shutdownBtn = document.getElementById('shutdown-btn');
    const dbStatusIndicator = document.getElementById('db-status-indicator');
    const progressBar = document.getElementById('progress-bar');
    const progressBarContainer = document.getElementById('progress-bar-container');
    const neo4jLink = document.getElementById('neo4j-link');
    const entityListContainer = document.getElementById('entity-list-container');
    const refreshEntitiesBtn = document.getElementById('refresh-entities-btn');
    const graphContainer = document.getElementById('graph-container');
    const refreshGraphBtn = document.getElementById('refresh-graph-btn');
    const keyEntitiesContainer = document.getElementById('key-entities-container');

    const API_BASE = '/api';
    let statusInterval;

    // --- Funciones de Utilidad ---
    function setBusy(isBusy, message) {
        ingestBtn.disabled = isBusy || !document.querySelector('.node-selected');
        ragBtn.disabled = isBusy;
        questionInput.disabled = isBusy;
        dirForm.querySelector('button').disabled = isBusy;
        shutdownBtn.disabled = isBusy;
        if (isBusy) {
            spinner.style.display = 'flex';
        } else {
            spinner.style.display = 'none';
        }
        statusText.textContent = message;
    }

    // --- Lógica del Árbol de Ficheros ---
    function createTreeNode(node) {
        const li = document.createElement('li');
        li.dataset.path = node.path;
        li.dataset.isDir = node.is_dir;
        li.dataset.loaded = node.children.length > 0;
        const content = document.createElement('div');
        content.className = 'node-content';
        const expander = document.createElement('span');
        expander.className = 'node-expander';
        if (node.is_dir) {
            expander.textContent = '▶';
            expander.addEventListener('click', (e) => { e.stopPropagation(); toggleExpand(li); });
        }
        const icon = document.createElement('span');
        icon.className = 'node-icon';
        icon.textContent = node.is_dir ? '📁' : '📄';
        const name = document.createElement('span');
        name.className = 'node-name';
        name.textContent = node.name;
        content.appendChild(expander);
        content.appendChild(icon);
        content.appendChild(name);
        li.appendChild(content);
        if (node.is_dir) {
            content.addEventListener('click', () => selectDirectory(li));
            const childrenUl = document.createElement('ul');
            childrenUl.className = 'node-children';
            li.appendChild(childrenUl);
            if (node.children.length > 0) {
                node.children.forEach(child => { childrenUl.appendChild(createTreeNode(child)); });
            }
        }
        return li;
    }
    async function toggleExpand(li) {
        const isLoaded = li.dataset.loaded === 'true';
        const path = li.dataset.path;
        const childrenUl = li.querySelector('.node-children');
        const expander = li.querySelector('.node-expander');
        const isExpanded = li.classList.toggle('expanded');
        expander.textContent = isExpanded ? '▼' : '▶';
        if (isExpanded && !isLoaded) {
            li.dataset.loaded = 'true';
            expander.textContent = '…';
            try {
                const response = await fetch(`${API_BASE}/list-directory`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ path }), });
                if (!response.ok) throw new Error('Fallo al cargar contenido.');
                const treeData = await response.json();
                childrenUl.innerHTML = '';
                treeData.children.forEach(child => { childrenUl.appendChild(createTreeNode(child)); });
                expander.textContent = '▼';
            } catch (error) {
                console.error(error);
                expander.textContent = '⚠️';
                li.classList.remove('expanded');
            }
        }
    }
    async function selectDirectory(li) {
        document.querySelectorAll('.node-selected').forEach(el => el.classList.remove('node-selected'));
        li.classList.add('node-selected');
        const path = li.dataset.path;
        setBusy(true, `Seleccionando ${path}...`);
        try {
            const response = await fetch(`${API_BASE}/select-directory`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ path }), });
            if (!response.ok) throw new Error((await response.json()).error || 'No se pudo seleccionar.');
            ingestBtn.disabled = false;
            setBusy(false, `Directorio listo para indexar: ${path}`);
        } catch (error) {
            console.error(error);
            setBusy(false, `Error: ${error.message}`);
            ingestBtn.disabled = true;
        }
    }
    async function loadDirectoryTree(path) {
        setBusy(true, `Cargando directorio: ${path || 'home'}...`);
        ingestBtn.disabled = true;
        try {
            const response = await fetch(`${API_BASE}/list-directory`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ path }), });
            if (!response.ok) throw new Error((await response.json()).error || 'Error al cargar.');
            const treeData = await response.json();
            fileTreeContainer.innerHTML = '';
            const treeRootUl = document.createElement('ul');
            treeRootUl.className = 'file-tree';
            treeRootUl.appendChild(createTreeNode(treeData));
            fileTreeContainer.appendChild(treeRootUl);
            dirPathInput.value = treeData.path;
            setBusy(false, `Navega y selecciona un directorio para procesar.`);
        } catch (error) {
            console.error(error);
            fileTreeContainer.innerHTML = `<div class="placeholder-tree">Error: ${error.message}</div>`;
            setBusy(false, `Error: ${error.message}`);
        }
    }

    // --- Lógica para Grafo y Entidades ---
    async function loadEntities() {
        entityListContainer.innerHTML = '<div class="placeholder-tree">Cargando entidades...</div>';
        try {
            const response = await fetch(`${API_BASE}/entities`);
            if (!response.ok) throw new Error('No se pudo cargar la lista de entidades.');
            const entities = await response.json();
            if (entities.length === 0) {
                entityListContainer.innerHTML = '<div class="placeholder-tree">No se han encontrado entidades.</div>';
                return;
            }
            entityListContainer.innerHTML = '';
            entities.forEach(entity => {
                const item = document.createElement('div');
                item.className = 'entity-item';
                item.innerHTML = `<span class="entity-name">${entity.id}</span><span class="entity-label">${entity.label}</span>`;
                item.addEventListener('click', () => {
                    questionInput.value = `¿Qué es "${entity.id}" y cómo se relaciona con otros conceptos?`;
                    ragForm.dispatchEvent(new Event('submit', { bubbles: true }));
                });
                entityListContainer.appendChild(item);
            });
        } catch (error) {
            console.error(error);
            entityListContainer.innerHTML = `<div class="placeholder-tree">Error: ${error.message}</div>`;
        }
    }

    async function loadGraphData() {
        graphContainer.innerHTML = '<div class="placeholder"><span>Cargando grafo...</span></div>';
        try {
            const response = await fetch(`${API_BASE}/graph-data`);
            if (!response.ok) throw new Error('No se pudo cargar los datos del grafo.');
            const data = await response.json();
            if (data.nodes.length === 0) {
                graphContainer.innerHTML = '<div class="placeholder"><span>No hay datos en el grafo para visualizar.</span></div>';
                return;
            }
            graphContainer.innerHTML = '';
            const elements = [
                ...data.nodes.map(node => ({ data: { id: node.id, label: node.label, group: node.group } })),
                ...data.edges.map(edge => ({ data: { source: edge.source, target: edge.target, label: edge.label } }))
            ];
            cytoscape({
                container: graphContainer,
                elements: elements,
                style: [
                    { selector: 'node', style: { 'background-color': '#2ea043', 'label': 'data(label)', 'color': '#c9d1d9', 'font-size': '10px', 'text-valign': 'bottom', 'text-halign': 'center', 'text-margin-y': '5px', 'width': '15px', 'height': '15px' } },
                    { selector: 'edge', style: { 'width': 1.5, 'line-color': '#30363d', 'target-arrow-color': '#30363d', 'target-arrow-shape': 'triangle', 'curve-style': 'bezier' } }
                ],
                layout: { name: 'cose', animate: false, idealEdgeLength: 100, nodeOverlap: 20, refresh: 20, fit: true, padding: 30, randomize: false, componentSpacing: 100, nodeRepulsion: 400000, edgeElasticity: 100, nestingFactor: 5, gravity: 80, numIter: 1000, initialTemp: 200, coolingFactor: 0.95, minTemp: 1.0 }
            });
        } catch (error) {
            console.error(error);
            graphContainer.innerHTML = `<div class="placeholder"><span>Error: ${error.message}</span></div>`;
        }
    }

    // --- Event Listeners ---
    dirForm.addEventListener('submit', (e) => { e.preventDefault(); const path = dirPathInput.value.trim(); if (path) loadDirectoryTree(path); });

    ingestBtn.addEventListener('click', async () => {
        setBusy(true, 'Iniciando indexación...');
        try {
            const response = await fetch(`${API_BASE}/ingest`, { method: 'POST' });
            if (!response.ok) throw new Error((await response.json()).error || 'No se pudo iniciar.');
            if (statusInterval) clearInterval(statusInterval);
            statusInterval = setInterval(fetchStatus, 500);
        } catch (error) {
            console.error(error);
            setBusy(false, `Error: ${error.message}`);
        }
    });

    ragForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const question = questionInput.value.trim();
        if (!question) return;

        setBusy(true, 'Enviando consulta RAG...');
        answerContainer.innerHTML = '<div class="placeholder"><span>Generando respuesta...</span></div>';
        keyEntitiesContainer.innerHTML = '';

        try {
            const response = await fetch(`${API_BASE}/rag-query`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ question }), });
            if (!response.ok) {
                const err = await response.json();
                throw new Error(err.error || 'Error en la consulta RAG.');
            }
            
            const { answer, key_entities } = await response.json();

            answerContainer.innerHTML = ''; 
            const paragraphs = answer.split(/\n\s*\n/); 

            paragraphs.forEach(pText => {
                const trimmedText = pText.trim();
                if (trimmedText) {
                    const p = document.createElement('p');
                    p.textContent = trimmedText;
                    answerContainer.appendChild(p);
                }
            });

            if (key_entities && key_entities.length > 0) {
                keyEntitiesContainer.innerHTML = '<h4>Entidades Clave en esta Respuesta:</h4>';
                const ul = document.createElement('ul');
                key_entities.forEach(entity => {
                    const li = document.createElement('li');
                    li.textContent = entity;
                    ul.appendChild(li);
                });
                keyEntitiesContainer.appendChild(ul);
            }
            setBusy(false, 'Consulta RAG completada.');

        } catch (error) {
            console.error(error);
            answerContainer.innerHTML = ''; 
            const errorP = document.createElement('p');
            errorP.className = 'error-message'; 
            errorP.textContent = `Error: ${error.message}`;
            answerContainer.appendChild(errorP);
            setBusy(false, `Error: ${error.message}`);
        }
    });

    copyBtn.addEventListener('click', () => {
        const answerText = answerContainer.innerText || answerContainer.textContent;
        navigator.clipboard.writeText(answerText).then(() => {
            const originalIcon = copyBtn.innerHTML;
            copyBtn.innerHTML = '✅';
            setTimeout(() => { copyBtn.innerHTML = originalIcon; }, 2000);
        });
    });

    shutdownBtn.addEventListener('click', async () => {
        if (confirm('¿Estás seguro de que quieres apagar el servidor?')) {
            try {
                setBusy(true, 'Apagando el servidor...');
                shutdownBtn.disabled = true;
                await fetch(`${API_BASE}/shutdown`, { method: 'POST' });
                statusText.textContent = 'El servidor se ha apagado. Ya puedes cerrar esta pestaña.';
            } catch (error) {
                statusText.textContent = 'El servidor se ha apagado. Ya puedes cerrar esta pestaña.';
            }
        }
    });

    refreshEntitiesBtn.addEventListener('click', loadEntities);
    refreshGraphBtn.addEventListener('click', loadGraphData);

    // --- Lógica de Estado y Polling ---
    async function fetchStatus() {
        try {
            const response = await fetch(`${API_BASE}/status`);
            if (!response.ok) throw new Error('Servidor no responde.');
            const data = await response.json();
            if (statusText.textContent !== data.message) statusText.textContent = data.message;
            if (data.progress > 0 && data.is_busy) {
                progressBarContainer.style.display = 'block';
                progressBar.style.width = `${data.progress * 100}%`;
            } else {
                progressBarContainer.style.display = 'none';
                progressBar.style.width = '0%';
            }
            if (!data.is_busy) {
                if(statusInterval) {
                    loadEntities();
                    loadGraphData();
                }
                clearInterval(statusInterval);
                statusInterval = null;
                setBusy(false, data.message);
            } else {
                setBusy(true, data.message);
            }
        } catch (error) {
            console.error('Error de estado:', error);
            if (statusInterval) clearInterval(statusInterval);
            statusInterval = null;
            setBusy(false, 'Error de conexión con el servidor.');
            dbStatusIndicator.classList.remove('status-ok');
            dbStatusIndicator.classList.add('status-error');
        }
    }

    async function checkDbStatus() {
        try {
            const response = await fetch(`${API_BASE}/neo4j-info`);
            if (!response.ok) throw new Error('Health check fallido');
            const data = await response.json();
            if (data.browser_url) neo4jLink.href = data.browser_url;
            dbStatusIndicator.classList.remove('status-error');
            dbStatusIndicator.classList.add('status-ok');
            neo4jLink.title = 'Conexión con Neo4j OK. Click para abrir.';
        } catch (error) {
            dbStatusIndicator.classList.remove('status-ok');
            dbStatusIndicator.classList.add('status-error');
            neo4jLink.title = 'Error de conexión con Neo4j.';
        }
    }

    // --- Cargas iniciales ---
    loadDirectoryTree('');
    checkDbStatus();
    setInterval(checkDbStatus, 15000);
    loadEntities();
    loadGraphData();
});