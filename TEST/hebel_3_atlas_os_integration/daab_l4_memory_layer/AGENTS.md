# Database-as-a-Brain (DaaB) - Context for AI Agents

> **Component**: DaaB (Database-as-a-Brain)  
> **Purpose**: Embedded persistence and cognitive memory  
> **Location**: `apps/kernel/src_agents/daab/`

## Overview

DaaB is Atlas's **Local-First, Embedded Stack** serving as active cognitive memory. It eliminates the need for external database servers while providing structured state management, semantic search, and long-term memory capabilities.

## Architecture

### Memory Tiers

```
┌──────────────────────────────────────┐
│         Hot Memory (In-Memory)       │
│  - Python dicts, asyncio.Lock        │
│  - Lowest latency, volatile          │
│  - Current conversation context      │
└──────────────┬───────────────────────┘
               │
┌──────────────▼───────────────────────┐
│       Warm Memory (Embedded DBs)     │
│  ┌────────────┐    ┌──────────────┐  │
│  │   SQLite   │    │   LanceDB    │  │
│  │ (Relational)│    │   (Vector)   │  │
│  └────────────┘    └──────────────┘  │
└──────────────┬───────────────────────┘
               │
┌──────────────▼───────────────────────┐
│      Cold Memory (Filesystem)        │
│  - Long-term artifact storage        │
│  - Raw files, documents, exports     │
└──────────────────────────────────────┘
```

### Components

| Component | Technology | Purpose | Location |
|-----------|-----------|---------|----------|
| **Hot Memory** | Python dict | Volatile context | In-memory |
| **SQLite** | aiosqlite 0.20 | State, logs, checkpoints | `~/.atlas/data/atlas.db` |
| **LanceDB** | lancedb 0.5 | Vector embeddings, semantic search | `~/.atlas/data/vectors` |
| **Filesystem** | Local FS | Artifacts, documents | `~/.atlas/data/artifacts` |

## SQLite Usage

### Purpose

- **LangGraph Checkpoints**: State snapshots for "time travel"
- **Audit Logs**: Immutable record of all operations
- **User Settings**: Preferences, configurations
- **Session History**: Conversation threads

### Schema Overview

```sql
-- LangGraph checkpoints
CREATE TABLE checkpoints (
    session_id TEXT NOT NULL,
    checkpoint_id TEXT NOT NULL,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    state_data TEXT NOT NULL,  -- JSON
    PRIMARY KEY (session_id, checkpoint_id)
);

-- Audit logs
CREATE TABLE audit_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    event_type TEXT NOT NULL,
    user_id TEXT,
    details TEXT,  -- JSON
    UNIQUE(id)
);

-- User settings
CREATE TABLE user_settings (
    user_id TEXT PRIMARY KEY,
    settings TEXT NOT NULL,  -- JSON
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Session history
CREATE TABLE sessions (
    session_id TEXT PRIMARY KEY,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT  -- JSON
);
```

### Using SQLite

```python
from src_agents.daab.memory_manager import MemoryManager

memory = MemoryManager()

# Save checkpoint
await memory.save_checkpoint(
    session_id="session-123",
    checkpoint_id="checkpoint-456",
    state_data={"user_input": "...", "results": [...]}
)

# Load checkpoint
checkpoint = await memory.load_checkpoint(
    session_id="session-123",
    checkpoint_id="checkpoint-456"
)

# Get checkpoint history
checkpoints = await memory.get_checkpoints(session_id="session-123")

# Audit logging
await memory.log_event(
    event_type="file_write",
    user_id="user-123",
    details={"path": "/path/to/file", "size": 1234}
)
```

## LanceDB Usage

### Purpose

- **Vector Embeddings**: Store document/code embeddings
- **Semantic Search**: Find relevant information by meaning
- **RAG (Retrieval Augmented Generation)**: Provide context to LLMs
- **Code Search**: Find similar code patterns

### Schema

```python
# LanceDB table schema
{
    "id": "doc-123",
    "text": "Original text content",
    "vector": [0.1, 0.2, ...],  # 1536-dim embedding
    "metadata": {
        "source": "file.py",
        "type": "code",
        "timestamp": "2026-01-09T19:00:00Z"
    }
}
```

### Using LanceDB

```python
from src_agents.daab.memory_manager import MemoryManager

memory = MemoryManager()

# Store embedding
await memory.store_embedding(
    text="Important information about authentication",
    metadata={
        "source": "auth.py",
        "type": "code",
        "function": "login"
    }
)

# Semantic search
results = await memory.search(
    query="how does authentication work?",
    top_k=5,
    filter={"type": "code"}
)

# Results format
[
    {
        "id": "doc-123",
        "text": "Authentication logic...",
        "score": 0.95,
        "metadata": {...}
    },
    # ...
]
```

## Memory Manager API

### Initialization

```python
from src_agents.daab.memory_manager import MemoryManager

# Initialize with default paths
memory = MemoryManager()

# Initialize with custom paths
memory = MemoryManager(
    sqlite_path="/custom/path/atlas.db",
    vector_path="/custom/path/vectors"
)
```

### Checkpoint Operations

```python
# Save checkpoint
await memory.save_checkpoint(
    session_id="session-123",
    checkpoint_id="checkpoint-456",
    state_data={"key": "value"}
)

# Load specific checkpoint
checkpoint = await memory.load_checkpoint(
    session_id="session-123",
    checkpoint_id="checkpoint-456"
)

# Load latest checkpoint
latest = await memory.load_latest_checkpoint(session_id="session-123")

# List all checkpoints for session
checkpoints = await memory.get_checkpoints(
    session_id="session-123",
    limit=10
)

# Delete old checkpoints
await memory.cleanup_checkpoints(
    session_id="session-123",
    keep_last=10
)
```

### Vector Operations

```python
# Store single embedding
await memory.store_embedding(
    text="Content to embed",
    metadata={"source": "file.py"}
)

# Batch store embeddings
await memory.store_embeddings_batch([
    {"text": "Content 1", "metadata": {"source": "file1.py"}},
    {"text": "Content 2", "metadata": {"source": "file2.py"}},
])

# Semantic search
results = await memory.search(
    query="authentication logic",
    top_k=5,
    filter={"type": "code"}
)

# Hybrid search (semantic + keyword)
results = await memory.hybrid_search(
    query="login function",
    keywords=["auth", "password"],
    top_k=5
)
```

### Session Management

```python
# Create session
await memory.create_session(
    session_id="session-123",
    metadata={"user_id": "user-456"}
)

# Update session
await memory.update_session(
    session_id="session-123",
    metadata={"last_activity": "2026-01-09T19:00:00Z"}
)

# Get session
session = await memory.get_session(session_id="session-123")

# List sessions
sessions = await memory.list_sessions(user_id="user-456")
```

### Audit Logging

```python
# Log event
await memory.log_event(
    event_type="file_write",
    user_id="user-123",
    details={
        "path": "/path/to/file",
        "size": 1234,
        "action": "create"
    }
)

# Query audit logs
logs = await memory.query_audit_logs(
    event_type="file_write",
    user_id="user-123",
    start_date="2026-01-01",
    end_date="2026-01-09"
)
```

## Common Patterns

### 1. RAG (Retrieval Augmented Generation)

```python
from src_agents.daab.memory_manager import MemoryManager
from src_agents.llm.client import LLMClient

memory = MemoryManager()
llm = LLMClient()

# User query
query = "How does authentication work?"

# Retrieve relevant context
context_docs = await memory.search(query, top_k=3)
context = "\n\n".join([doc["text"] for doc in context_docs])

# Generate response with context
response = await llm.generate(
    prompt=f"Context:\n{context}\n\nQuestion: {query}\n\nAnswer:",
    capability="text"
)
```

### 2. Conversation History

```python
# Store conversation turn
await memory.save_checkpoint(
    session_id="session-123",
    checkpoint_id=f"turn-{turn_number}",
    state_data={
        "user_input": "What is LangGraph?",
        "agent_response": "LangGraph is...",
        "timestamp": datetime.now().isoformat()
    }
)

# Retrieve conversation history
history = await memory.get_checkpoints(
    session_id="session-123",
    limit=10
)
```

### 3. Code Indexing

```python
import os
from pathlib import Path

# Index codebase
async def index_codebase(directory: str):
    memory = MemoryManager()
    
    for file_path in Path(directory).rglob("*.py"):
        with open(file_path) as f:
            content = f.read()
        
        await memory.store_embedding(
            text=content,
            metadata={
                "source": str(file_path),
                "type": "code",
                "language": "python"
            }
        )

# Search codebase
results = await memory.search(
    query="authentication implementation",
    filter={"type": "code", "language": "python"},
    top_k=5
)
```

## Performance Optimization

### Connection Pooling

```python
# SQLite connection pool
from aiosqlite import connect

class MemoryManager:
    def __init__(self):
        self._connection_pool = []
    
    async def get_connection(self):
        if not self._connection_pool:
            return await connect(self.sqlite_path)
        return self._connection_pool.pop()
    
    async def release_connection(self, conn):
        self._connection_pool.append(conn)
```

### Batch Operations

```python
# Batch insert embeddings
await memory.store_embeddings_batch([
    {"text": f"Document {i}", "metadata": {"id": i}}
    for i in range(1000)
])

# Batch checkpoint cleanup
await memory.cleanup_checkpoints_batch([
    "session-1", "session-2", "session-3"
])
```

### Caching

```python
from functools import lru_cache

class MemoryManager:
    @lru_cache(maxsize=100)
    async def get_session_cached(self, session_id: str):
        return await self.get_session(session_id)
```

## Testing

### Unit Tests

```python
import pytest
from src_agents.daab.memory_manager import MemoryManager

@pytest.mark.asyncio
async def test_checkpoint_save_load():
    memory = MemoryManager()
    
    # Save checkpoint
    await memory.save_checkpoint(
        session_id="test-session",
        checkpoint_id="test-checkpoint",
        state_data={"key": "value"}
    )
    
    # Load checkpoint
    checkpoint = await memory.load_checkpoint(
        session_id="test-session",
        checkpoint_id="test-checkpoint"
    )
    
    assert checkpoint["state_data"]["key"] == "value"

@pytest.mark.asyncio
async def test_semantic_search():
    memory = MemoryManager()
    
    # Store embeddings
    await memory.store_embedding(
        text="Python is a programming language",
        metadata={"topic": "programming"}
    )
    
    # Search
    results = await memory.search("coding language", top_k=1)
    
    assert len(results) > 0
```

## Best Practices

### DO
- Use transactions for related SQLite operations
- Batch vector operations when possible
- Clean up old checkpoints periodically
- Use appropriate indexes on SQLite tables
- Validate data before storing

### DON'T
- Store large binary data in SQLite (use filesystem)
- Perform synchronous I/O operations
- Skip error handling on database operations
- Store sensitive data unencrypted
- Create too many small transactions

## Related Documentation

- **Python Kernel**: [../GEMINI.md](apps/kernel/GEMINI.md)
- **Workflows**: [../workflows/GEMINI.md](apps/kernel/src_agents/workflows/GEMINI.md)
- **Master Architecture**: [docs/architecture/01_System_Architecture.md](docs/architecture/01_System_Architecture.md)
- **Core Concepts**: [docs/architecture/02_Cognitive_Architecture.md](docs/architecture/02_Cognitive_Architecture.md)
