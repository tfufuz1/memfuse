# src-python/daab/models.py

from pydantic import BaseModel, Field
from typing import Dict, Any, List
from enum import Enum
import uuid

class RelationshipType(str, Enum):
    """
    Defines the types of relationships (edges) between nodes in the knowledge graph.
    Aligned with database/init/03_graph_schema.sql.
    """
    EXECUTED = "EXECUTED"
    CREATED = "CREATED"
    MODIFIED = "MODIFIED"
    DEPENDS_ON = "DEPENDS_ON"
    BELONGS_TO = "BELONGS_TO"
    RELATED_TO = "RELATED_TO"
    # Keeping some for potential internal use or future expansion
    ANALYZES = "ANALYZES"
    CONTAINS = "CONTAINS"
    REFERENCES = "REFERENCES"
    GENERATES = "GENERATES"


class GraphNode(BaseModel):
    """
    Base model for a node in the knowledge graph.
    Provides common attributes like a unique ID and a label.
    """
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    label: str
    properties: Dict[str, Any] = Field(default_factory=dict)

    def to_cypher_properties(self) -> str:
        """
        Converts the properties dictionary to a Cypher properties string.
        Example: {name: 'Agent1', type: 'Supervisor'}
        """
        if not self.properties:
            return ""
        
        items = []
        for key, value in self.properties.items():
            if isinstance(value, str):
                # Escape single quotes in string values
                escaped_value = value.replace("'", "'\'")
                items.append(f"{key}: '{escaped_value}'")
            elif isinstance(value, (int, float, bool)):
                items.append(f"{key}: {str(value).lower()}")
            else:
                # For simplicity, other types are converted to string.
                # A more robust implementation might handle lists, dicts, etc.
                escaped_value = str(value).replace("'", "'\'")
                items.append(f"{key}: '{escaped_value}'")
        return "{" + ", ".join(items) + "}"

# Specific Node types aligned with 03_graph_schema.sql
class AgentNode(GraphNode):
    """Represents an Agent in the graph."""
    label: str = "Agent"

class ConceptNode(GraphNode):
    """Represents a Concept in the graph."""
    label: str = "Concept"

class TaskNode(GraphNode):
    """Represents a Task in the graph."""
    label: str = "Task"

class FileNode(GraphNode):
    """Represents a File in the graph."""
    label: str = "File"

class UserNode(GraphNode):
    """Represents a User in the graph."""
    label: str = "User"

class AppNode(GraphNode):
    """Represents an App in the graph."""
    label: str = "App"

class WorkspaceNode(GraphNode):
    """Represents a Workspace in the graph."""
    label: str = "Workspace"

class GraphRelationship(BaseModel):
    """
    Base model for a relationship (edge) between two nodes.
    """
    source_id: str
    target_id: str
    type: RelationshipType
    properties: Dict[str, Any] = Field(default_factory=dict)

# Example Usage (for understanding)
if __name__ == "__main__":
    # Create an agent node
    agent = AgentNode(
        properties={"name": "SupervisorAgent", "role": "Supervisor", "version": "1.0"}
    )
    print("Agent Node:", agent)
    print("Cypher Properties:", agent.to_cypher_properties())

    # Create a task node
    task = TaskNode(
        properties={
            "description": "Analyze project structure",
            "status": "in_progress",
            "priority": "high"
        }
    )
    print("\nTask Node:", task)
    print("Cypher Properties:", task.to_cypher_properties())

    # Create a file node
    file_node = FileNode(properties={"path": "/src/main.py", "type": "python"})
    print("\nFile Node:", file_node)

    # Create a relationship
    relationship = GraphRelationship(
        source_id=agent.id,
        target_id=task.id,
        type=RelationshipType.EXECUTED
    )
    print("\nRelationship (Agent EXECUTED Task):", relationship)

    relationship2 = GraphRelationship(
        source_id=task.id,
        target_id=file_node.id,
        type=RelationshipType.MODIFIED
    )
    print("\nRelationship (Task MODIFIED File):", relationship2)