from typing import List, Literal, Optional, Any, Dict, Union
from pydantic import BaseModel, Field, ConfigDict

# --- Props Definitions ---

class TextProps(BaseModel):
    content: str
    variant: Literal["h1", "h2", "h3", "body", "caption", "mono"] = "body"
    color: Optional[str] = None

class ButtonProps(BaseModel):
    label: str
    variant: Literal["primary", "ghost"] = "primary"
    icon: Optional[str] = None
    actionId: Optional[str] = None

class MetricCardProps(BaseModel):
    label: str
    value: str
    trend: Optional[str] = None
    icon: Optional[str] = None
    intent: Literal["primary", "secondary", "success", "danger", "warning", "info"] = "info"

class DataTableProps(BaseModel):
    columns: List[str]
    data: List[List[Any]]

class HeaderProps(BaseModel):
    title: str
    subtitle: Optional[str] = None
    icon: Optional[str] = None

class DateTimeInputProps(BaseModel):
    label: str
    value: Optional[str] = None
    enable_date: bool = Field(default=True, alias="enableDate")
    enable_time: bool = Field(default=True, alias="enableTime")
    model_config = ConfigDict(populate_by_name=True)

class SliderProps(BaseModel):
    label: str
    value: int = 0
    min_value: int = Field(default=0, alias="minValue")
    max_value: int = Field(default=100, alias="maxValue")
    model_config = ConfigDict(populate_by_name=True)

class MultipleChoiceProps(BaseModel):
    description: str
    options: List[str]
    selections: List[str] = []
    max_allowed_selections: int = Field(default=1, alias="maxAllowedSelections")
    model_config = ConfigDict(populate_by_name=True)

# --- Advanced Props ---

class SkillTreeNode(BaseModel):
    id: str
    label: str
    x: Optional[float] = 0
    y: Optional[float] = 0
    status: Literal["locked", "available", "unlocked"] = "locked"
    icon: Optional[str] = None
    description: Optional[str] = None

class SkillTreeConnection(BaseModel):
    from_node: str = Field(alias="from")
    to_node: str = Field(alias="to")
    model_config = ConfigDict(populate_by_name=True)

class SkillTreeProps(BaseModel):
    nodes: List[SkillTreeNode]
    connections: List[SkillTreeConnection]

class SpatialCanvasItem(BaseModel):
    id: str
    x: float
    y: float
    component: Any # Will be A2UIComponent but avoid circular dependency during definition

class SpatialCanvasProps(BaseModel):
    items: List[SpatialCanvasItem]
    width: Optional[str] = "100%"
    height: Optional[str] = "500px"

class CraftingBenchProps(BaseModel):
    slots: List[Dict[str, Any]]
    output: Optional[Dict[str, Any]] = None
    is_craftable: bool = Field(default=False, alias="isCraftable")
    model_config = ConfigDict(populate_by_name=True)

class LootBoxProps(BaseModel):
    is_open: bool = Field(default=False, alias="isOpen")
    items: List[Dict[str, Any]]
    rarity: Literal["common", "rare", "epic", "legendary"] = "common"
    model_config = ConfigDict(populate_by_name=True)

# --- Component Definitions ---

class BaseComponent(BaseModel):
    id: str
    type: str
    props: Dict[str, Any]
    children: Optional[List['A2UIComponent']] = None

class TextComponent(BaseModel):
    id: str
    type: Literal["text"]
    props: TextProps

class ButtonComponent(BaseModel):
    id: str
    type: Literal["button"]
    props: ButtonProps

class MetricCardComponent(BaseModel):
    id: str
    type: Literal["metric_card"]
    props: MetricCardProps

class DataTableComponent(BaseModel):
    id: str
    type: Literal["data_table"]
    props: DataTableProps

class HeaderComponent(BaseModel):
    id: str
    type: Literal["header"]
    props: HeaderProps

class DateTimeInputComponent(BaseModel):
    id: str
    type: Literal["datetime_input"]
    props: DateTimeInputProps

class SliderComponent(BaseModel):
    id: str
    type: Literal["slider"]
    props: SliderProps

class MultipleChoiceComponent(BaseModel):
    id: str
    type: Literal["multiple_choice"]
    props: MultipleChoiceProps

class SkillTreeComponent(BaseModel):
    id: str
    type: Literal["skill_tree"]
    props: SkillTreeProps

class SpatialCanvasComponent(BaseModel):
    id: str
    type: Literal["spatial_canvas"]
    props: SpatialCanvasProps

class CraftingBenchComponent(BaseModel):
    id: str
    type: Literal["crafting_bench"]
    props: CraftingBenchProps

class LootBoxComponent(BaseModel):
    id: str
    type: Literal["loot_box"]
    props: LootBoxProps

class ContainerComponent(BaseModel):
    id: str
    type: Literal["container"]
    children: List['A2UIComponent'] = []
    props: Dict[str, Any] = {}

class StatGridComponent(BaseModel):
    id: str
    type: Literal["stat_grid"]
    props: Dict[str, Any] # Usually contains stats: List[MetricProps]

# Union type for all components
A2UIComponent = Union[
    TextComponent, 
    ButtonComponent, 
    MetricCardComponent, 
    DataTableComponent, 
    HeaderComponent, 
    DateTimeInputComponent,
    SliderComponent,
    MultipleChoiceComponent,
    SkillTreeComponent,
    SpatialCanvasComponent,
    CraftingBenchComponent,
    LootBoxComponent,
    ContainerComponent,
    StatGridComponent
]

# Recursive self-reference update
ContainerComponent.model_rebuild()
SpatialCanvasItem.model_rebuild()

class SurfaceUpdate(BaseModel):
    surfaceId: str
    components: List[A2UIComponent]

# --- Transport / Interaction Models ---

class ActionResult(BaseModel):
    success: bool
    error_message: Optional[str] = Field(default=None, alias="errorMessage")

class ClientEvent(BaseModel):
    # Placeholder for client events (interactions)
    type: str
    payload: Dict[str, Any] = {}

class ServerMessage(BaseModel):
    model_config = ConfigDict(populate_by_name=True)
    surface_update: Optional[Dict[str, Any]] = Field(default=None, alias="surfaceUpdate")
    ack: Optional[ActionResult] = None
