import logging
import functools
from contextlib import asynccontextmanager
from typing import Optional, Callable
from .db_manager import DatabaseManager

logger = logging.getLogger(__name__)

class SchemaContext:
    """
    Context manager to safely switch database schemas for a block of code.
    Ensures the schema is reset after execution.
    
    Usage:
        async with SchemaContext("app_travel_planner"):
            await DatabaseManager.fetch("SELECT * FROM trips")
    """
    def __init__(self, schema_name: str):
        self.schema_name = schema_name
        self.token = None

    async def __aenter__(self):
        # We read the token so we can potentially use it strictly if we moved to contextvars directly (which we did inside DBManager)
        # But DBManager.set_schema manages the contextvar. 
        # Wait, contextvars.ContextVar.set returns a Token to reset.
        # DBManager.set_schema currently just sets it. 
        # Ideally DBManager.set_schema should return the token or we handle it here if we had access to the var directly.
        # But since DBManager wraps it, we rely on set_schema logic.
        # Actually, looking at DBManager implementation:
        # It calls cls._current_schema.set(schema). This returns a token.
        # But the method returns None.
        
        # To be purely safe with ContextVars in nested calls, we should capture the PREVIOUS value.
        self.previous_schema = DatabaseManager.get_schema()
        DatabaseManager.set_schema(self.schema_name)
        logger.debug(f"Entered SchemaContext: {self.schema_name}")
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        DatabaseManager.set_schema(self.previous_schema)
        logger.debug(f"Exited SchemaContext: {self.schema_name} -> restored {self.previous_schema}")

def with_schema(schema_name: str):
    """
    Decorator to run a function within a specific schema context.
    
    Usage:
        @with_schema("app_travel_planner")
        async def get_trips():
            ...
    """
    def decorator(func: Callable):
        @functools.wraps(func)
        async def wrapper(*args, **kwargs):
            async with SchemaContext(schema_name):
                return await func(*args, **kwargs)
        return wrapper
    return decorator
