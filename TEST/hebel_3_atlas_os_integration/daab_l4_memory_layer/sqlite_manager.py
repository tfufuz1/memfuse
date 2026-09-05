import os
import aiosqlite
import logging
from pathlib import Path

logger = logging.getLogger(__name__)

class SQLiteManager:
    """
    Manages SQLite database connection and schema initialization.
    Designed for "Zero-Config" local usage without external database servers.
    """
    
    def __init__(self, db_path: str = None):
        if db_path is None:
            # Allow override via environment variable
            db_path_env = os.getenv("ATLAS_DB_PATH")
            if db_path_env:
                self.db_path = db_path_env
            else:
                # Default to ~/.atlas/data/atlas.db
                home = Path.home()
                data_dir = home / ".atlas" / "data"
                data_dir.mkdir(parents=True, exist_ok=True)
                self.db_path = str(data_dir / "atlas.db")
        else:
            self.db_path = db_path
            
        logger.info(f"SQLiteManager initialized with path: {self.db_path}")

    async def initialize(self):
        """Creates necessary tables if they don't exist."""
        logger.info("Initializing SQLite database...")
        async with aiosqlite.connect(self.db_path) as db:
            # 1. Checkpointers (for LangGraph state)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS checkpointers (
                    thread_id TEXT NOT NULL,
                    thread_ts TEXT NOT NULL,
                    parent_ts TEXT,
                    checkpoint BLOB,
                    metadata BLOB,
                    PRIMARY KEY (thread_id, thread_ts)
                )
            """)
            
            # 2. Conversations (Chat History)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS conversations (
                    id TEXT PRIMARY KEY,
                    title TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    metadata TEXT
                )
            """)
            
            # 3. Messages (Chat Messages)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS messages (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    metadata TEXT,
                    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
                )
            """)
            
            # 4. User Settings (Key-Value Store)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS user_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)
            
            # 5. Edges (Graph Relationships)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS edges (
                    source_id TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    relation_type TEXT NOT NULL,
                    properties TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    PRIMARY KEY (source_id, target_id, relation_type)
                )
            """)

            # 5b. Nodes (Graph Entities)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS nodes (
                    id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    properties TEXT, -- JSON dict
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)
            
            # 6. MCP Servers (Registry)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS mcp_servers (
                    server_id TEXT PRIMARY KEY,
                    name TEXT UNIQUE NOT NULL,
                    command TEXT NOT NULL,
                    args TEXT, -- JSON list
                    env TEXT, -- JSON dict
                    auto_restart BOOLEAN DEFAULT 1,
                    max_retries INTEGER DEFAULT 3,
                    timeout_seconds INTEGER DEFAULT 30,
                    permissions TEXT, -- JSON dict
                    metadata TEXT, -- JSON dict
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)

            # 7. MCP Server Stats
            await db.execute("""
                CREATE TABLE IF NOT EXISTS mcp_server_stats (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    server_id TEXT NOT NULL,
                    total_calls INTEGER DEFAULT 0,
                    successful_calls INTEGER DEFAULT 0,
                    failed_calls INTEGER DEFAULT 0,
                    total_uptime_seconds INTEGER DEFAULT 0,
                    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY(server_id) REFERENCES mcp_servers(server_id) ON DELETE CASCADE
                )
            """)

            # 8. Tool Executions
            await db.execute("""
                CREATE TABLE IF NOT EXISTS tool_executions (
                    execution_id TEXT PRIMARY KEY,
                    thread_id TEXT NOT NULL,
                    agent_name TEXT,
                    tool_name TEXT NOT NULL,
                    input_args TEXT,
                    output_result TEXT,
                    success INTEGER,
                    error_message TEXT,
                    execution_time_ms INTEGER,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)

            # 9. Workspaces
            await db.execute("""
                CREATE TABLE IF NOT EXISTS workspaces (
                    workspace_id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    root_path TEXT NOT NULL,
                    settings TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)

            # 10. Business Logic Components
            await db.execute("""
                CREATE TABLE IF NOT EXISTS business_logic_components (
                    component_id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    component_type TEXT NOT NULL,
                    description TEXT,
                    input_schema TEXT,
                    output_schema TEXT,
                    implementation TEXT,
                    category TEXT,
                    version TEXT,
                    tags TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)
            
            # 11. Apps (App Factory)
            await db.execute("""
                CREATE TABLE IF NOT EXISTS apps (
                    app_id TEXT PRIMARY KEY,
                    workspace_id TEXT,
                    app_name TEXT,
                    app_description TEXT,
                    app_type TEXT,
                    a2ui_schema TEXT,
                    langgraph_definition TEXT,
                    required_tools TEXT,
                    created_by TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)

            # 12. Knowledge FTS (Full-Text Search)
            # This is a virtual table for keyword search, synchronized with LanceDB content.
            await db.execute("""
                CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
                    id UNINDEXED,
                    content,
                    metadata UNINDEXED,
                    tokenize='porter unicode61'
                )
            """)
            
            await db.commit()
        logger.info("SQLite database initialized successfully.")

    def get_db_path(self):
        return self.db_path

    async def get_connection(self):
        """Returns an aiosqlite connection. Caller must close it."""
        return await aiosqlite.connect(self.db_path)

    async def health_check(self) -> bool:
        """
        Verifies database connectivity.
        Returns True if connection is successful, False otherwise.
        """
        try:
            async with aiosqlite.connect(self.db_path) as db:
                await db.execute("SELECT 1")
            return True
        except Exception as e:
            logger.error(f"Database health check failed: {e}")
            return False

    async def execute(self, query: str, *args):
        """Helper to execute a single query (auto-commits)."""
        async with aiosqlite.connect(self.db_path) as db:
            await db.execute(query, args)
            await db.commit()

    async def fetchall(self, query: str, *args):
        """Helper to fetch all results from a query."""
        async with aiosqlite.connect(self.db_path) as db:
            db.row_factory = aiosqlite.Row
            async with db.execute(query, args) as cursor:
                return await cursor.fetchall()
