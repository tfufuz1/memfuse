"""
MemFuse A2UI Result Card Generator.

Transforms MemFuse 4-Signal Hybrid Retrieval results into structured
A2UI components (Card, Badges, Provenance Breakdown) for live streaming in Atlas / Tauri.
"""

from typing import Any, Dict, List
import json


def render_memfuse_hit_as_a2ui(hit: Dict[str, Any]) -> Dict[str, Any]:
    """
    Renders a single MemFuse hybrid retrieval hit as an interactive A2UI Card.
    """
    score = hit.get("score", 0.0)
    text = hit.get("content", "")
    metadata = hit.get("metadata", {})
    source_app = metadata.get("source_app", "System")

    # Score color coding
    badge_color = "green" if score > 0.7 else ("amber" if score > 0.4 else "gray")

    card_component = {
        "type": "card",
        "title": f"Memory Match (Score: {score:.3f})",
        "header_actions": [
            {
                "type": "badge",
                "label": f"{score * 100:.1f}% Match",
                "color": badge_color,
            },
            {
                "type": "badge",
                "label": source_app,
                "color": "blue",
            }
        ],
        "children": [
            {
                "type": "text",
                "content": text[:300] + ("..." if len(text) > 300 else ""),
                "variant": "body",
            },
            {
                "type": "row",
                "children": [
                    {"type": "text", "content": f"Tags: {metadata.get('tags', [])}", "variant": "caption"},
                    {"type": "text", "content": f"Created: {metadata.get('created_at', '')}", "variant": "caption"},
                ]
            }
        ]
    }

    return card_component


if __name__ == "__main__":
    sample_hit = {
        "score": 0.892,
        "content": "SIMD AVX-512 distance computation reduces vector search latency by up to 8x.",
        "metadata": {
            "source_app": "VS Code",
            "tags": ["simd", "performance", "avx512"],
            "created_at": "2026-09-05T12:00:00Z",
        }
    }
    rendered = render_memfuse_hit_as_a2ui(sample_hit)
    print(json.dumps(rendered, indent=2))
