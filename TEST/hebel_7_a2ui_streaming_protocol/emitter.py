import uuid
from typing import Dict, List, Optional, Any, Union
from pydantic import BaseModel, Field

# --- Data Models (Internal use for Emitter) ---

class AtomicComponent(BaseModel):
    id: str
    type: str
    props: Dict[str, Any] = Field(default_factory=dict)
    children: Optional[List[Union[Dict[str, Any], 'AtomicComponent']]] = None
    action: Optional[Dict[str, Any]] = None
    transition: Optional[Dict[str, Any]] = None

    class Config:
        arbitrary_types_allowed = True

class SurfaceUpdate(BaseModel):
    type: str  # FULL_RENDER, PARTIAL_UPDATE, etc.
    payload: Union[AtomicComponent, List[Dict[str, Any]], Dict[str, Any]]
    surfaceId: Optional[str] = None

# --- Emitter Class ---

class A2UIEmitter:
    """
    A2UI Atomic+ Emitter.
    Generates specification-compliant JSON payloads for the frontend.
    """

    def __init__(self):
        pass

    def _generate_id(self, prefix: str = "comp") -> str:
        """Generates a unique ID with a prefix."""
        return f"{prefix}-{str(uuid.uuid4())[:8]}"

    def full_render(self, component: Dict[str, Any], surface_id: Optional[str] = None) -> Dict[str, Any]:
        """Creates a FULL_RENDER event."""
        return SurfaceUpdate(
            type="FULL_RENDER",
            payload=component,
            surfaceId=surface_id
        ).model_dump(exclude_none=True)

    def partial_update(self, updates: List[Dict[str, Any]], surface_id: Optional[str] = None) -> Dict[str, Any]:
        """Creates a PARTIAL_UPDATE event."""
        # Updates should be list of {id: "...", props: {...}}
        return SurfaceUpdate(
            type="PARTIAL_UPDATE",
            payload=updates,
            surfaceId=surface_id
        ).model_dump(exclude_none=True)

    def append_child(self, parent_id: str, components: List[Dict[str, Any]], surface_id: Optional[str] = None) -> Dict[str, Any]:
        """Creates an APPEND_CHILD event."""
        return SurfaceUpdate(
            type="APPEND_CHILD",
            payload={"parentId": parent_id, "components": components},
            surfaceId=surface_id
        ).model_dump(exclude_none=True)
    
    def remove_component(self, component_ids: List[str], surface_id: Optional[str] = None) -> Dict[str, Any]:
        """Creates a REMOVE_COMPONENT event."""
        return SurfaceUpdate(
            type="REMOVE_COMPONENT",
            payload=component_ids,
            surfaceId=surface_id
        ).model_dump(exclude_none=True)

    # --- Atoms ---

    def text(self, content: str, variant: str = "body", color: Optional[str] = None, id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a Text atom."""
        props = {"content": content, "variant": variant}
        if color:
            props["color"] = color
        
        return AtomicComponent(
            id=id or self._generate_id("text"),
            type="text",
            props=props
        ).model_dump(exclude_none=True)

    def button(self, label: str, action_id: str, variant: str = "primary", icon: Optional[str] = None, payload: Optional[Dict] = None, id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a Button atom."""
        props = {"label": label, "variant": variant}
        if icon:
            props["icon"] = icon

        return AtomicComponent(
            id=id or self._generate_id("btn"),
            type="button",
            props=props,
            action={
                "actionId": action_id,
                "payload": payload or {}
            }
        ).model_dump(exclude_none=True)
    
    def badge(self, label: str, color: str = "blue", id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a Badge atom."""
        return AtomicComponent(
            id=id or self._generate_id("badge"),
            type="badge",
            props={"label": label, "color": color}
        ).model_dump(exclude_none=True)

    def input_field(self, placeholder: str = "", value: str = "", input_type: str = "text", action_id: str = "update_input", id: Optional[str] = None) -> Dict[str, Any]:
        """Generates an Input atom."""
        return AtomicComponent(
            id=id or self._generate_id("input"),
            type="input",
            props={
                "value": value,
                "placeholder": placeholder,
                "type": input_type
            },
            action={
                "actionId": action_id,
                "payload": {} # Payload usually auto-filled by frontend with value
            }
        ).model_dump(exclude_none=True)

    # --- Molecules ---

    def header(self, title: str, subtitle: Optional[str] = None, icon: Optional[str] = None, breadcrumb: Optional[str] = None, id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a Header molecule."""
        props = {"title": title}
        if subtitle:
            props["subtitle"] = subtitle
        if icon:
            props["icon"] = icon
        if breadcrumb:
            props["breadcrumb"] = breadcrumb
            
        return AtomicComponent(
            id=id or self._generate_id("header"),
            type="header",
            props=props
        ).model_dump(exclude_none=True)

    def metric_card(self, label: str, value: str, trend: Optional[str] = None, trend_dir: Optional[str] = None, icon: Optional[str] = None, id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a MetricCard molecule."""
        props = {"label": label, "value": value}
        if trend:
            props["trend"] = trend
        if trend_dir:
            props["trendDir"] = trend_dir
        if icon:
            props["icon"] = icon

        return AtomicComponent(
            id=id or self._generate_id("metric"),
            type="metric_card",
            props=props
        ).model_dump(exclude_none=True)

    def alert_banner(self, title: str, message: str, severity: str = "info", id: Optional[str] = None) -> Dict[str, Any]:
        """Generates an AlertBanner molecule."""
        return AtomicComponent(
            id=id or self._generate_id("alert"),
            type="alert_banner",
            props={
                "title": title,
                "message": message,
                "severity": severity
            }
        ).model_dump(exclude_none=True)
    
    def loading_spinner(self, size: str = "md", id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a LoadingSpinner molecule/atom."""
        return AtomicComponent(
            id=id or self._generate_id("spinner"),
            type="spinner",
            props={"size": size}
        ).model_dump(exclude_none=True)

    # --- Organisms ---

    def stat_grid(self, stats: List[Dict[str, Any]], id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a StatGrid organism."""
        return AtomicComponent(
            id=id or self._generate_id("stat-grid"),
            type="stat_grid",
            props={"stats": stats}
        ).model_dump(exclude_none=True)
    
    def data_table(self, columns: List[str], data: List[List[Any]], id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a DataTable organism."""
        return AtomicComponent(
            id=id or self._generate_id("table"),
            type="data_table",
            props={"columns": columns, "data": data}
        ).model_dump(exclude_none=True)

    def chat_stream(self, messages: List[Dict], component_id: Optional[str] = None) -> Dict:
        # messages: List of {role: str, content: str}
        return self._base_component("chat_stream", {"messages": messages}, component_id=component_id)

    def _base_component(self, type: str, props: Dict[str, Any], component_id: Optional[str] = None) -> Dict[str, Any]:
        """Base helper for creating component dicts."""
        return AtomicComponent(
            id=component_id or self._generate_id(type),
            type=type,
            props=props
        ).model_dump(exclude_none=True)

    def confirmation_dialog(self, title: str, message: str, action_id: str, component_id: Optional[str] = None) -> Dict:
        """
        Creates a modal dialog for requesting user confirmation.
        """
        actions = [
            self.button(label="Cancel", action_id=f"{action_id}_cancel", variant="secondary"),
            self.button(label="Approve", action_id=f"{action_id}_approve", variant="primary")
        ]
        return self._base_component("dialog", {
            "title": title,
            "message": message,
            "actions": actions
        }, component_id=component_id)

    # --- Templates ---

    def container(self, children: List[Dict[str, Any]], padding: Optional[str] = None, gap: Optional[str] = None, id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a Container template."""
        props = {}
        if padding:
            props["padding"] = padding
        if gap:
            props["gap"] = gap

        return AtomicComponent(
            id=id or self._generate_id("container"),
            type="container",
            props=props,
            children=children
        ).model_dump(exclude_none=True)

    def dashboard_layout(self, children: List[Dict[str, Any]], id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a DashboardLayout template."""
        return AtomicComponent(
            id=id or self._generate_id("dashboard"),
            type="layout_dashboard",
            props={},
            children=children
        ).model_dump(exclude_none=True)
    
    # Alias for tests
    def layout_dashboard(self, *args, **kwargs):
        return self.dashboard_layout(*args, **kwargs)
    
    def split_layout(self, sidebar: Dict[str, Any], main: Dict[str, Any], id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a SplitLayout template."""
        return AtomicComponent(
            id=id or self._generate_id("split"),
            type="layout_split",
            props={},
            children=[sidebar, main]
        ).model_dump(exclude_none=True)
    
    def feed_layout(self, children: List[Dict[str, Any]], id: Optional[str] = None) -> Dict[str, Any]:
        """Generates a FeedLayout template."""
        return AtomicComponent(
            id=id or self._generate_id("feed"),
            type="layout_feed",
            props={},
            children=children
        ).model_dump(exclude_none=True)
