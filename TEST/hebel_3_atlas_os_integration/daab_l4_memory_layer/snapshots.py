from pydantic import BaseModel
from typing import Dict, Any, Optional
import datetime
import uuid
from .db_manager import DatabaseManager

class ContextSnapshot(BaseModel):
    """
    Represents a frozen state of an application/agent at a specific point in time.
    Used for 'Time Travel' or switching contexts.
    """
    id: str = str(uuid.uuid4())
    timestamp: datetime.datetime = datetime.datetime.now()
    app_id: str
    schema_name: str
    
    # The LangGraph Agent State 
    agent_state: Dict[str, Any]
    
    # The Front-End UI State (JSON representation of the current View)
    ui_state: Dict[str, Any]
    
    # Optional metadata (e.g. "Draft specific", "User checkpoint")
    metadata: Optional[Dict[str, Any]] = None

class SnapshotManager:
    @staticmethod
    async def save_snapshot(snapshot: ContextSnapshot):
        """Persists a snapshot to the database."""
        query = """
            INSERT INTO snapshots (id, app_id, schema_name, data, created_at)
            VALUES ($1, $2, $3, $4, $5)
        """
        # We serialize the entire model to JSONB
        import json
        data_json = snapshot.model_dump_json()
        
        # We need to ensure the 'snapshots' table exists in the global schema probably?
        # Or in the app schema? 
        # Strategy: Snapshots allow restoring an app, so they might live in the Global Brain 
        # OR the App Brain. Let's assume App Brain for now, or Local System Brain.
        # If we are inside an app context, we write to that schema's table.
        
        await DatabaseManager.execute(
            query, 
            snapshot.id, 
            snapshot.app_id, 
            snapshot.schema_name, 
            data_json, 
            snapshot.timestamp
        )
        return snapshot.id

    @staticmethod
    async def load_snapshot(snapshot_id: str) -> Optional[ContextSnapshot]:
        """Loads a snapshot by ID."""
        query = "SELECT data FROM snapshots WHERE id = $1"
        record = await DatabaseManager.fetchrow(query, snapshot_id)
        if record:
            import json
            data = json.loads(record['data'])
            return ContextSnapshot(**data)
        return None
