import asyncio
import logging
from typing import Dict, Any, Optional
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)

@dataclass
class UIStream:
    """Holds the communication queue for a specific surface/session."""
    surface_id: str
    queue: asyncio.Queue = field(default_factory=asyncio.Queue)
    active_agent_id: Optional[str] = None

class A2UIStreamManager:
    """
    Manages active UI streams and provides a high-level API for agents 
    to push UI updates to the client.
    """
    _instance = None
    _streams: Dict[str, UIStream] = {}

    def __new__(cls):
        if cls._instance is None:
            cls._instance = super(A2UIStreamManager, cls).__new__(cls)
        return cls._instance

    def register_surface(self, surface_id: str) -> asyncio.Queue:
        """Called when a client connects. Returns the queue to read from."""
        if surface_id not in self._streams:
            logger.info(f"Registering new A2UI surface stream: {surface_id}")
            self._streams[surface_id] = UIStream(surface_id=surface_id)
        return self._streams[surface_id].queue

    def unregister_surface(self, surface_id: str):
        """Called when client disconnects."""
        if surface_id in self._streams:
            logger.info(f"Unregistering A2UI surface stream: {surface_id}")
            del self._streams[surface_id]

    async def emit_event(self, surface_id: str, event_type: str, payload: Dict[str, Any]):
        """Low-level emit. Puts a raw event dict into the queue."""
        if surface_id not in self._streams:
            logger.warning(f"Attempted to emit to unknown surface: {surface_id}")
            return
        
        # We wrap this in a structure the gRPC service understands.
        # Ideally, we should use a shared internal event object, but for now we use a dict
        # that the service will convert to a Protobuf message.
        await self._streams[surface_id].queue.put({
            "type": event_type,
            "payload": payload
        })

    # --- High Level Helpers for Agents ---

    async def emit_status(self, surface_id: str, message: str, state: str = "processing"):
        """Emits a status update (e.g. for a progress bar or toaster)."""
        await self.emit_event(surface_id, "status_update", {
            "message": message,
            "state": state
        })

    async def emit_render(self, surface_id: str, component_json: Dict[str, Any]):
        """Emits a full component render instruction."""
        await self.emit_event(surface_id, "render", {
            "component": component_json
        })

    async def emit_token(self, surface_id: str, text: str):
        """Emits a streaming token for text generation."""
        await self.emit_event(surface_id, "token", {
            "text": text
        })
