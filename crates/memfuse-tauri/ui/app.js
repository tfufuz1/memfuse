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
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        dbStatusEl.textContent = `❌ Fehler: ${errMsg}`;
    }
});

// ── Collections laden und anzeigen ──────────────────────────────────────
async function refreshCollections() {
    if (!dbOpen) return;
    try {
        const collections = await invoke('list_collections');
        collectionsListEl.textContent = '';
        for (const col of collections) {
            const item = document.createElement('div');
            item.className = 'collection-item' + (col.name === activeCollection ? ' active' : '');

            const span = document.createElement('span');
            span.textContent = `${col.name} `;
            const small = document.createElement('small');
            small.style.opacity = '0.6';
            small.textContent = `(${col.document_count})`;
            span.appendChild(small);

            const btn = document.createElement('button');
            btn.className = 'delete-btn';
            btn.dataset.name = col.name;
            btn.textContent = '✕';

            item.appendChild(span);
            item.appendChild(btn);

            span.addEventListener('click', () => selectCollection(col.name));
            btn.addEventListener('click', async (ev) => {
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
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        alert(`Collection konnte nicht erstellt werden: ${errMsg}`);
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
    const payload = event.payload;
    if (!payload) return;
    const logEl = document.getElementById('import-log');
    if (!logEl) return;

    const line = document.createElement('div');

    if (payload.total_files_processed !== undefined) {
        // Gebatchter IngestProgressBatch
        const total = payload.total_files_processed;
        const chunks = payload.batch_chunks_created || 0;
        const errors = payload.batch_errors || [];
        const fileName = payload.last_file_path ? payload.last_file_path.split(/[/\\]/).pop() : 'Fortschritt';

        if (errors.length > 0) {
            line.textContent = `⚠️ Fortschritt (${total} Dateien): +${chunks} Abschnitte, ${errors.length} Fehler in diesem Batch`;
            line.title = errors.join('\n');
        } else {
            line.textContent = `✅ Fortschritt (${total} Dateien): ${fileName} (+${chunks} Abschnitte)`;
        }
    } else {
        // Einzelner IngestReport (Fallback)
        const errCount = (payload.errors || []).length;
        const fileName = payload.file_path ? payload.file_path.split(/[/\\]/).pop() : 'Datei';

        if (errCount > 0) {
            line.textContent = `⚠️ ${fileName}: ${payload.chunks_created || 0} Abschnitte, ${errCount} Fehler`;
            line.title = (payload.errors || []).join('\n');
        } else {
            line.textContent = `✅ ${fileName}: ${payload.chunks_created || 0} Abschnitte`;
        }
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
    statusEl.textContent = isFolder ? 'Ordner wird importiert... ' : 'Datei wird importiert... ';
    const spinner = document.createElement('span');
    spinner.className = 'spinner';
    statusEl.appendChild(spinner);
    logEl.textContent = '';

    try {
        const result = await importFn();
        const reports = Array.isArray(result) ? result : [result];

        let totalChunks = 0;
        let totalErrors = 0;

        // Falls es sich um eine Einzeldatei handelt oder die Progress Events nicht genutzt wurden
        if (!isFolder) {
            logEl.textContent = '';
            for (const report of reports) {
                totalChunks += report.chunks_created || 0;
                const errCount = (report.errors || []).length;
                totalErrors += errCount;

                const line = document.createElement('div');
                const fileName = report.file_path.split(/[/\\]/).pop();
                if (errCount > 0) {
                    line.textContent = `⚠️ ${fileName}: ${report.chunks_created} Abschnitte, ${errCount} Fehler`;
                    line.title = report.errors.join('\n');
                } else {
                    line.textContent = `✅ ${fileName}: ${report.chunks_created} Abschnitte`;
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
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        statusEl.textContent = `❌ Import fehlgeschlagen: ${errMsg}`;
    }
}

// ── Ollama Status Listener & Modell-Verwaltung ──────────────────────────
listen('ollama-status', (event) => {
    const statusEl = document.getElementById('ollama-status');
    if (!statusEl) return;
    const msg = event.payload;
    if (msg && msg.startsWith('Ollama ready')) {
        statusEl.textContent = `🟢 ${msg}`;
        statusEl.style.background = '#1f4e3d';
    } else {
        statusEl.textContent = `🔴 ${msg || 'Ollama nicht erreichbar'} — Bitte Ollama starten`;
        statusEl.style.background = '#5c1d1d';
    }
});

async function refreshModels() {
    const statusEl = document.getElementById('ollama-status');
    try {
        const models = await invoke('list_ollama_models');
        modelSelect.textContent = '';
        if (models.length === 0) {
            const opt = document.createElement('option');
            opt.textContent = 'Kein Ollama-Modell gefunden';
            modelSelect.appendChild(opt);
            if (statusEl) {
                statusEl.textContent = '⚠️ Ollama bereit: Keine Modelle installiert';
                statusEl.style.background = '#5c4a1d';
            }
            return;
        }
        for (const m of models) {
            const opt = document.createElement('option');
            opt.value = m;
            opt.textContent = m;
            modelSelect.appendChild(opt);
        }
        if (statusEl) {
            statusEl.textContent = `🟢 Ollama ready: ${models.length} Modelle`;
            statusEl.style.background = '#1f4e3d';
        }
    } catch (e) {
        modelSelect.textContent = '';
        const opt = document.createElement('option');
        opt.textContent = '⚠️ Ollama nicht erreichbar';
        modelSelect.appendChild(opt);
        if (statusEl) {
            statusEl.textContent = '🔴 Ollama nicht erreichbar — Bitte Ollama starten';
            statusEl.style.background = '#5c1d1d';
        }
    }
}

// ── Mode Handling & Chat/Search ──────────────────────────────────────────
let currentMode = 'chat';
document.querySelectorAll('input[name="mode"]').forEach(radio => {
    radio.addEventListener('change', (e) => {
        currentMode = e.target.value;
        const container = document.getElementById('multistep-container');
        if (container) {
            container.style.display = currentMode === 'search' ? 'inline-flex' : 'none';
        }
    });
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

    const userMsgEl = document.createElement('p');
    const userStrong = document.createElement('strong');
    userStrong.textContent = 'Sie: ';
    userMsgEl.appendChild(userStrong);
    userMsgEl.appendChild(document.createTextNode(message));
    chatLog.appendChild(userMsgEl);

    currentResponseEl = document.createElement('p');
    const assistStrong = document.createElement('strong');
    assistStrong.textContent = 'Assistent: ';
    currentResponseEl.appendChild(assistStrong);
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

        if (response.sources && response.sources.length > 0) {
            sourcesEl.textContent = '';
            const header = document.createElement('strong');
            header.textContent = '📎 Quellen:';
            sourcesEl.appendChild(header);
            sourcesEl.appendChild(document.createElement('br'));
            for (const s of response.sources) {
                const srcItem = document.createElement('div');
                srcItem.textContent = `• ${s.source} `;
                const relSpan = document.createElement('span');
                relSpan.style.opacity = '0.6';
                relSpan.textContent = `(Relevanz: ${(s.score * 100).toFixed(0)}%)`;
                srcItem.appendChild(relSpan);
                sourcesEl.appendChild(srcItem);
            }
        }
    } catch (e) {
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        currentResponseEl.textContent += `\n⚠️ Fehler: ${errMsg}`;
    }
}

async function runDirectSearch() {
    const input = document.getElementById('query-input');
    const query = input.value.trim();
    if (!query || !activeCollection) return;

    const isMultiStep = document.getElementById('multistep-checkbox')?.checked;

    const searchMsgEl = document.createElement('p');
    const searchStrong = document.createElement('strong');
    searchStrong.textContent = isMultiStep ? 'Suche (Multi-Step): ' : 'Suche: ';
    searchMsgEl.appendChild(searchStrong);
    searchMsgEl.appendChild(document.createTextNode(query));
    chatLog.appendChild(searchMsgEl);
    input.value = '';

    try {
        let results = [];
        let roundsExecuted = 1;
        let subQueries = [];

        if (isMultiStep) {
            const multiRes = await invoke('multi_step_search', {
                query,
                collectionName: activeCollection,
                k: 8,
                maxRounds: 3,
            });
            results = multiRes.results;
            roundsExecuted = multiRes.rounds_executed;
            subQueries = multiRes.sub_queries;
        } else {
            results = await invoke('hybrid_search', {
                query,
                collectionName: activeCollection,
                k: 8,
            });
        }

        const resultsEl = document.createElement('div');
        resultsEl.style.cssText = 'margin: 0.5rem 0 1rem 0;';

        if (isMultiStep) {
            const auditInfo = document.createElement('div');
            auditInfo.style.cssText = 'font-size: 0.8rem; opacity: 0.85; margin-bottom: 0.5rem; color: #4aa3df;';
            let infoText = `🔄 Multi-Step Suche: ${roundsExecuted} Runde(n) ausgeführt.`;
            if (subQueries && subQueries.length > 0) {
                infoText += ` Teil-Queries: "${subQueries.join('", "')}"`;
            }
            auditInfo.textContent = infoText;
            resultsEl.appendChild(auditInfo);
        }

        if (results.length === 0) {
            const noHitsEl = document.createElement('div');
            noHitsEl.textContent = 'Keine Treffer gefunden.';
            resultsEl.appendChild(noHitsEl);
        } else {
            for (const r of results) {
                const card = document.createElement('div');
                card.style.cssText = 'border: 1px solid #ddd; border-radius: 6px; padding: 0.6rem; margin-bottom: 0.4rem;';

                const meta = document.createElement('div');
                meta.style.cssText = 'font-size: 0.8rem; opacity: 0.7;';
                meta.textContent = `📄 ${r.source} — Relevanz: ${(r.score * 100).toFixed(0)}%`;

                const body = document.createElement('div');
                body.style.cssText = 'margin-top: 0.3rem;';
                body.textContent = `${r.text_preview}...`;

                card.appendChild(meta);
                card.appendChild(body);
                resultsEl.appendChild(card);
            }
        }
        chatLog.appendChild(resultsEl);
        chatLog.scrollTop = chatLog.scrollHeight;
    } catch (e) {
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        const errorMsgEl = document.createElement('p');
        errorMsgEl.textContent = `⚠️ Fehler: ${errMsg}`;
        chatLog.appendChild(errorMsgEl);
    }
}

function escapeHtml(str) {
    return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
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
            statusEl.textContent = `✅ Ollama gefunden mit ${models.length} Modell(en)`;
        } else {
            statusEl.textContent = `⚠️ Ollama läuft, aber kein Modell installiert. ` +
                `Führen Sie 'ollama pull llama3.2' in einem Terminal aus.`;
        }
    } catch (e) {
        statusEl.textContent = `⚠️ Ollama wurde nicht gefunden. Bitte installieren Sie ` +
            `Ollama von https://ollama.com und starten Sie es, bevor Sie fortfahren.`;
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
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        alert(`Fehler beim Öffnen der Datenbank: ${errMsg}`);
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
        const errMsg = typeof e === 'object' && e !== null && e.message ? `[${e.kind}] ${e.message}` : e;
        alert(`Collection konnte nicht erstellt werden: ${errMsg}`);
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
