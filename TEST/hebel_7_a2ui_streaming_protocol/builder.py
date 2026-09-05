from typing import List, Optional, Dict, Any, Union, Literal
from pydantic import BaseModel, Field
import uuid

# --- Core Types ---

class A2UIComponent(BaseModel):
    type: str
    props: Dict[str, Any] = Field(default_factory=dict)
    children: Optional[List["A2UIComponent"]] = None
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))

    def to_json_dict(self) -> Dict[str, Any]:
        """Recursive serialization to clean JSON dictionary."""
        data = {
            "type": self.type,
            "props": self.props,
            "id": self.id
        }
        if self.children:
            data["children"] = [child.to_json_dict() for child in self.children]
        return data

# --- Atoms ---

class Text(A2UIComponent):
    def __init__(self, content: str, variant: Literal["h1", "h2", "body", "mono"] = "body", **kwargs):
        super().__init__(type="text", props={"content": content, "variant": variant, **kwargs})

class Button(A2UIComponent):
    def __init__(self, label: str, variant: Literal["primary", "secondary", "ghost", "danger", "success", "warning"] = "primary", icon: Optional[str] = None, action_id: Optional[str] = None, **kwargs):
        props = {"label": label, "variant": variant, **kwargs}
        if icon: props["icon"] = icon
        if action_id: props["action_id"] = action_id
        super().__init__(type="button", props=props)

class Input(A2UIComponent):
    def __init__(self, placeholder: str = "", value: str = "", action_id: Optional[str] = None, **kwargs):
        props = {"placeholder": placeholder, "value": value, **kwargs}
        if action_id: props["action_id"] = action_id
        super().__init__(type="input", props=props)

class Badge(A2UIComponent):
    def __init__(self, label: str, color: Literal["green", "red", "blue", "gray", "success", "warning", "danger", "info"] = "gray", **kwargs):
        # Map intents to colors if needed, but for now allow both
        super().__init__(type="badge", props={"label": label, "color": color, **kwargs})

# --- Molecules ---

class MetricCard(A2UIComponent):
    def __init__(self, label: str, value: str, trend: Optional[str] = None, icon: Optional[str] = None, **kwargs):
        props = {"label": label, "value": value, **kwargs}
        if trend: props["trend"] = trend
        if icon: props["icon"] = icon
        super().__init__(type="metric_card", props=props)

class AlertBanner(A2UIComponent):
    def __init__(self, title: str, message: str, severity: Literal["info", "warning", "error", "success"] = "info", **kwargs):
        super().__init__(type="alert_banner", props={"title": title, "message": message, "severity": severity, **kwargs})

# --- Organisms ---

class DataTable(A2UIComponent):
    def __init__(self, columns: List[str], data: List[List[Any]], **kwargs):
        super().__init__(type="data_table", props={"columns": columns, "data": data, **kwargs})

class StatGrid(A2UIComponent):
    def __init__(self, stats: List[Dict[str, str]], **kwargs):
        # stats expects list of dicts like {"label": "CPU", "value": "90%"}
        super().__init__(type="stat_grid", props={"stats": stats, **kwargs})

class ChatStream(A2UIComponent):

    def __init__(self, messages: List[Dict[str, str]], **kwargs):

        super().__init__(type="chat_stream", props={"messages": messages, **kwargs})



class SkillTree(A2UIComponent):

    def __init__(self, nodes: List[Dict[str, Any]], connections: List[Dict[str, Any]], **kwargs):

        super().__init__(type="skill_tree", props={"nodes": nodes, "connections": connections, **kwargs})



class SpatialCanvas(A2UIComponent):

    def __init__(self, items: List[Dict[str, Any]], width: str = "100%", height: str = "500px", **kwargs):

        super().__init__(type="spatial_canvas", props={"items": items, "width": width, "height": height, **kwargs})



class CraftingBench(A2UIComponent):



    def __init__(self, slots: List[Dict[str, Any]], output: Optional[Dict[str, Any]] = None, is_craftable: bool = False, **kwargs):



        super().__init__(type="crafting_bench", props={"slots": slots, "output": output, "isCraftable": is_craftable, **kwargs})







class LootBox(A2UIComponent):



    def __init__(self, items: List[Dict[str, Any]], is_open: bool = False, rarity: str = "common", **kwargs):



        super().__init__(type="loot_box", props={"items": items, "isOpen": is_open, "rarity": rarity, **kwargs})







# --- Templates & Layouts ---







class LayoutDashboard(A2UIComponent):

    def __init__(self, children: List[A2UIComponent], **kwargs):

        super().__init__(type="layout_dashboard", children=children, props=kwargs)



class Container(A2UIComponent):

    def __init__(self, children: List[A2UIComponent], layout: Literal["col", "row"] = "col", **kwargs):

        super().__init__(type="container", children=children, props={"layout": layout, **kwargs})



# --- Builder Utility ---



class A2UI:

    """Static Builder Factory for cleaner syntax in Agents."""

    

    @staticmethod

    def text(content: str, variant: str = "body") -> Text:

        return Text(content, variant)

        

    @staticmethod

    def button(label: str, action_id: str, variant: str = "primary") -> Button:

        return Button(label, variant=variant, action_id=action_id)

        

    @staticmethod

    def row(*children: A2UIComponent) -> Container:

        return Container(children=list(children), layout="row")

        

    @staticmethod

    def col(*children: A2UIComponent) -> Container:

        return Container(children=list(children), layout="col")

        

    @staticmethod

    def dashboard(*children: A2UIComponent) -> LayoutDashboard:

        return LayoutDashboard(children=list(children))



    @staticmethod

    def skill_tree(nodes: List[Dict[str, Any]], connections: List[Dict[str, Any]]) -> SkillTree:

        return SkillTree(nodes, connections)



    @staticmethod

    def spatial_canvas(items: List[Dict[str, Any]]) -> SpatialCanvas:

        return SpatialCanvas(items)



        @staticmethod



        def crafting_bench(slots: List[Dict[str, Any]], output: Optional[Dict[str, Any]] = None, is_craftable: bool = False) -> CraftingBench:



            return CraftingBench(slots, output, is_craftable)



    



        @staticmethod



        def loot_box(items: List[Dict[str, Any]], is_open: bool = False, rarity: str = "common") -> LootBox:



            return LootBox(items, is_open, rarity)



    



        @staticmethod



        def metric(label: str, value: str, trend: Optional[str] = None) -> MetricCard:



    



            return MetricCard(label, value, trend=trend)



    



        @staticmethod



        def table(columns: List[str], data: List[List[Any]]) -> DataTable:



            return DataTable(columns, data)



    



        @staticmethod



        def grid(*children: A2UIComponent) -> StatGrid:



            # Convert children to the expected format for StatGrid if they are MetricCards



            stats = []



            for child in children:



                if child.type == "metric_card":



                    stats.append(child.props)



            return StatGrid(stats)



    
UIBuilder = A2UI
