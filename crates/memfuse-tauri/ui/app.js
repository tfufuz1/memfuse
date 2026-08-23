const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;
const { listen } = window.__TAURI__.event;

let activeCollection = null;
let dbOpen = false;

const dbStatusEl = document.getElementById('db-status');
const collectionsListEl = document.getElementById('collections-list');
const activeTitleEl = document.getElementById('active-collection-title');
const chatLog = document.getElementById('chat-log');
const modelSelect = document.getElementById('model-select');

// ── Datenbank öffnen ─────────────────────────────────────────────────────
document.getElementById('open-db-btn').addEventListener('click', async () => {
    const selected = await open({
        directory: true,
        multiple: false,
        title: 'Datenbankordner wählen oder neu erstellen',
    });
    if (!selected) return;

    try {
        await invoke('open_database', { path: selected });
        dbOpen = true;
        dbStatusEl.textContent = `✅ Verbunden: ${selected}`;
        dbStatusEl.classList.add('connected');
        await refreshCollections();
        await refreshModels();
    } catch (e) {
        dbStatusEl.textContent = `❌ Fehler: ${e}`;
    }
});

// ── Collections laden und anzeigen ──────────────────────────────────────
async function refreshCollections() {
    if (!dbOpen) return;
    try {
        const collections = await invoke('list_collections');
        collectionsListEl.innerHTML = '';
        for (const col of collections) {
            const item = document.createElement('div');
            item.className = 'collection-item' + (col.name === activeCollection ? ' active' : '');
            item.innerHTML = `
                <span>${col.name} <small style="opacity:0.6">(${col.document_count})</small></span>
                <button class="delete-btn" data-name="${col.name}">✕</button>
            `;
            item.querySelector('span').addEventListener('click', () => selectCollection(col.name));
            item.querySelector('.delete-btn').addEventListener('click', async (ev) => {
                ev.stopPropagation();
                if (confirm(`Collection "${col.name}" wirklich löschen? Alle Dokumente gehen verloren.`)) {
                    await invoke('drop_collection', { name: col.name });
                    if (activeCollection === col.name) activeCollection = null;
                    await refreshCollections();
                }
            });
            collectionsListEl.appendChild(item);
        }
    } catch (e) {
        console.error('Collections laden fehlgeschlagen:', e);
    }
}

function selectCollection(name) {
    activeCollection = name;
    activeTitleEl.textContent = `📚 ${name}`;
    refreshCollections();
}

// ── Neue Collection erstellen ────────────────────────────────────────────
document.getElementById('create-collection-btn').addEventListener('click', async () => {
    const input = document.getElementById('new-collection-input');
    const name = input.value.trim();
    if (!name || !dbOpen) return;

    try {
        await invoke('create_collection', { name });
        input.value = '';
        await refreshCollections();
        selectCollection(name);
    } catch (e) {
        alert(`Collection konnte nicht erstellt werden: ${e}`);
    }
});

// ── Dokumenten-Import ─────────────────────────────────────────────────────
document.getElementById('import-file-btn').addEventListener('click', async () => {
    if (!checkCollectionSelected()) return;

    const selected = await open({
        directory: false,
        multiple: false,
        filters: [
            { name: 'Unterstützte Dokumente', extensions: ['pdf', 'docx', 'md', 'txt', 'eml'] }
        ],
    });
    if (!selected) return;

    await runImport(async () => {
        return await invoke('ingest_file', {
            filePath: selected,
            collectionName: activeCollection,
        });
    }, [selected]);
});

document.getElementById('import-folder-btn').addEventListener('click', async () => {
    if (!checkCollectionSelected()) return;

    const selected = await open({
        directory: true,
        multiple: false,
        title: 'Ordner mit Dokumenten wählen',
    });
    if (!selected) return;

    await runImport(async () => {
        return await invoke('ingest_folder', {
            folderPath: selected,
            collectionName: activeCollection,
        });
    }, null, true);
});

listen('ingest-progress', (event) => {
    const report = event.payload;
    if (!report) return;
    const logEl = document.getElementById('import-log');
    if (!logEl) return;

    const errCount = (report.errors || []).length;
    const line = document.createElement('div');
    const fileName = report.file_path ? report.file_path.split(/[/\\]/).pop() : 'Datei';

    if (errCount > 0) {
        line.innerHTML = `⚠️ ${fileName}: ${report.chunks_created || 0} Abschnitte, ${errCount} Fehler`;
        line.title = report.errors.join('\n');
    } else {
        line.innerHTML = `✅ ${fileName}: ${report.chunks_created || 0} Abschnitte`;
    }
    logEl.appendChild(line);
    logEl.scrollTop = logEl.scrollHeight;
});

function checkCollectionSelected() {
    if (!activeCollection) {
        alert('Bitte wählen Sie zuerst eine Collection aus, in die importiert werden soll.');
        return false;
    }
    return true;
}

async function runImport(importFn, filePaths, isFolder = false) {
    const progressEl = document.getElementById('import-progress');
    const statusEl = document.getElementById('import-status');
    const logEl = document.getElementById('import-log');

    progressEl.style.display = 'block';
    statusEl.innerHTML = (isFolder ? 'Ordner wird importiert... ' : 'Datei wird importiert... ') + '<span class="spinner"></span>';
    logEl.innerHTML = '';

    try {
        const result = await importFn();
        const reports = Array.isArray(result) ? result : [result];

        let totalChunks = 0;
        let totalErrors = 0;

        // Falls es sich um eine Einzeldatei handelt oder die Progress Events nicht genutzt wurden
        if (!isFolder) {
            logEl.innerHTML = '';
            for (const report of reports) {
                totalChunks += report.chunks_created || 0;
                const errCount = (report.errors || []).length;
                totalErrors += errCount;

                const line = document.createElement('div');
                const fileName = report.file_path.split(/[/\\]/).pop();
                if (errCount > 0) {
                    line.innerHTML = `⚠️ ${fileName}: ${report.chunks_created} Abschnitte, ${errCount} Fehler`;
                    line.title = report.errors.join('\n');
                } else {
                    line.innerHTML = `✅ ${fileName}: ${report.chunks_created} Abschnitte`;
                }
                logEl.appendChild(line);
            }
        } else {
            for (const report of reports) {
                totalChunks += report.chunks_created || 0;
                totalErrors += (report.errors || []).length;
            }
        }

        statusEl.textContent = `Fertig: ${totalChunks} Abschnitte importiert` +
            (totalErrors > 0 ? `, ${totalErrors} Fehler` : '');

        await refreshCollections();  // Dokumentenzähler aktualisieren
    } catch (e) {
        statusEl.textContent = `❌ Import fehlgeschlagen: ${e}`;
    }
}

// ── Ollama-Modelle laden ─────────────────────────────────────────────────
async function refreshModels() {
    try {
        const models = await invoke('list_ollama_models');
        modelSelect.innerHTML = '';
        if (models.length === 0) {
            modelSelect.innerHTML = '<option>Kein Ollama-Modell gefunden</option>';
            return;
        }
        for (const m of models) {
            const opt = document.createElement('option');
            opt.value = m;
            opt.textContent = m;
            modelSelect.appendChild(opt);
        }
    } catch (e) {
        modelSelect.innerHTML = '<option>⚠️ Ollama nicht erreichbar</option>';
    }
}

// ── Mode Handling & Chat/Search ──────────────────────────────────────────
let currentMode = 'chat';
document.querySelectorAll('input[name="mode"]').forEach(radio => {
    radio.addEventListener('change', (e) => { currentMode = e.target.value; });
});

let currentResponseEl = null;
listen('chat-token', (event) => {
    if (currentResponseEl) {
        currentResponseEl.textContent += event.payload;
    }
});

document.getElementById('send-btn').addEventListener('click', sendMessage);
document.getElementById('query-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') sendMessage();
});

async function sendMessage() {
    if (currentMode === 'search') {
        return runDirectSearch();
    }

    const input = document.getElementById('query-input');
    const message = input.value.trim();
    if (!message) return;

    if (!activeCollection) {
        alert('Bitte wählen Sie zuerst eine Collection aus der Seitenleiste.');
        return;
    }

    chatLog.innerHTML += `<p><strong>Sie:</strong> ${escapeHtml(message)}</p>`;
    currentResponseEl = document.createElement('p');
    currentResponseEl.innerHTML = '<strong>Assistent:</strong> ';
    chatLog.appendChild(currentResponseEl);

    const sourcesEl = document.createElement('div');
    sourcesEl.className = 'sources-box';
    sourcesEl.style.cssText = 'font-size: 0.8rem; opacity: 0.75; margin: 0.3rem 0 1rem 1rem; border-left: 2px solid #444; padding-left: 0.6rem;';
    chatLog.appendChild(sourcesEl);

    chatLog.scrollTop = chatLog.scrollHeight;
    input.value = '';

    const model = modelSelect.value || 'llama3.2';

    try {
        const response = await invoke('chat_with_rag', {
            message,
            collectionName: activeCollection,
            model,
        });

        // response ist jetzt { answer, sources } statt reinem String
        if (response.sources && response.sources.length > 0) {
            sourcesEl.innerHTML = '<strong>📎 Quellen:</strong><br>' +
                response.sources.map(s =>
                    `• ${escapeHtml(s.source)} <span style="opacity:0.6">(Relevanz: ${(s.score * 100).toFixed(0)}%)</span>`
                ).join('<br>');
        }
    } catch (e) {
        currentResponseEl.textContent += `\n⚠️ Fehler: ${e}`;
    }
}

async function runDirectSearch() {
    const input = document.getElementById('query-input');
    const query = input.value.trim();
    if (!query || !activeCollection) return;

    chatLog.innerHTML += `<p><strong>Suche:</strong> ${escapeHtml(query)}</p>`;
    input.value = '';

    try {
        const results = await invoke('hybrid_search', {
            query,
            collectionName: activeCollection,
            k: 8,
        });

        const resultsEl = document.createElement('div');
        resultsEl.style.cssText = 'margin: 0.5rem 0 1rem 0;';
        if (results.length === 0) {
            resultsEl.textContent = 'Keine Treffer gefunden.';
        } else {
            resultsEl.innerHTML = results.map(r => `
                <div style="border: 1px solid #ddd; border-radius: 6px; padding: 0.6rem; margin-bottom: 0.4rem;">
                    <div style="font-size: 0.8rem; opacity: 0.7;">
                        📄 ${escapeHtml(r.source)} — Relevanz: ${(r.score * 100).toFixed(0)}%
                    </div>
                    <div style="margin-top: 0.3rem;">${escapeHtml(r.text_preview)}...</div>
                </div>
            `).join('');
        }
        chatLog.appendChild(resultsEl);
        chatLog.scrollTop = chatLog.scrollHeight;
    } catch (e) {
        chatLog.innerHTML += `<p>⚠️ Fehler: ${e}</p>`;
    }
}

function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

// ── Onboarding-Flow ────────────────────────────────────────────────────
const onboardingOverlay = document.getElementById('onboarding-overlay');

async function checkOnboardingNeeded() {
    // Onboarding zeigen, solange keine Datenbank geöffnet ist
    if (!dbOpen) {
        onboardingOverlay.style.display = 'flex';
        await checkOllamaAvailability();
    } else {
        onboardingOverlay.style.display = 'none';
    }
}

async function checkOllamaAvailability() {
    const statusEl = document.getElementById('onboarding-ollama-check');
    try {
        const models = await invoke('list_ollama_models');
        if (models.length > 0) {
            statusEl.innerHTML = `✅ Ollama gefunden mit ${models.length} Modell(en)`;
        } else {
            statusEl.innerHTML = `⚠️ Ollama läuft, aber kein Modell installiert. ` +
                `Führen Sie <code>ollama pull llama3.2</code> in einem Terminal aus.`;
        }
    } catch (e) {
        statusEl.innerHTML = `⚠️ Ollama wurde nicht gefunden. Bitte installieren Sie ` +
            `Ollama von <a href="https://ollama.com" target="_blank" style="color:#6ab0ff;">ollama.com</a> ` +
            `und starten Sie es, bevor Sie fortfahren.`;
    }
}

document.getElementById('onboarding-choose-folder').addEventListener('click', async () => {
    const selected = await open({ directory: true, multiple: false });
    if (!selected) return;

    try {
        await invoke('open_database', { path: selected });
        dbOpen = true;
        dbStatusEl.textContent = `✅ Verbunden: ${selected}`;
        dbStatusEl.classList.add('connected');

        document.getElementById('onboarding-step-1').style.display = 'none';
        document.getElementById('onboarding-step-2').style.display = 'block';
    } catch (e) {
        alert(`Fehler beim Öffnen der Datenbank: ${e}`);
    }
});

document.getElementById('onboarding-create-collection').addEventListener('click', async () => {
    const name = document.getElementById('onboarding-collection-name').value.trim();
    if (!name) {
        alert('Bitte geben Sie einen Namen ein.');
        return;
    }

    try {
        await invoke('create_collection', { name });
        activeCollection = name;
        document.getElementById('onboarding-collection-display').textContent = name;
        document.getElementById('onboarding-step-2').style.display = 'none';
        document.getElementById('onboarding-step-3').style.display = 'block';
        await refreshCollections();
        selectCollection(name);
    } catch (e) {
        alert(`Collection konnte nicht erstellt werden: ${e}`);
    }
});

document.getElementById('onboarding-import-now').addEventListener('click', async () => {
    onboardingOverlay.style.display = 'none';
    document.getElementById('import-folder-btn').click();  // bestehenden Import-Flow triggern
});

document.getElementById('onboarding-skip').addEventListener('click', () => {
    onboardingOverlay.style.display = 'none';
});

// Beim App-Start aufrufen:
checkOnboardingNeeded();
