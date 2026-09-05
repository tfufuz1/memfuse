# A2UI Quick Reference

## 🚀 Installation & Import

```python
# Installation
pip install atlas  # oder local: from src-python/

# Import
from a2ui import (
    # Pattern Builders
    SlotFillingFormBuilder,
    SmartSelectorBuilder,
    RangeNegotiatorBuilder,
    FileDropContextBuilder,
    # ... alle 15 Pattern ...
    
    # Integration
    AgentContext,
    AgentResponseBuilder,
    
    # Validation
    A2UIValidator,
    ValidationLevel,
)
```

## 📋 15 Patterns - Quick Reference

### A: Data Collection

#### 1. Slot-Filling Form
```python
form = SlotFillingFormBuilder()
form.add_text_field("name", "Name")
form.add_date_field("date", "Date")
form.add_number_field("count", "Count", min_val=1, max_val=10)
form.add_select_field("option", "Choose", ["A", "B", "C"])
payload = form.build()
```

#### 2. Smart Selector
```python
selector = SmartSelectorBuilder()
selector.add_option("opt1", "Option 1", "Description", "icon")
selector.add_option("opt2", "Option 2", "Description", "icon")
payload = selector.build()
```

#### 3. Range Negotiator
```python
slider = RangeNegotiatorBuilder(
    label="Quality",
    left_label="Fast",
    right_label="Accurate",
    current_value=0.5,
    step=0.1
)
payload = slider.build()
```

#### 4. File-Drop
```python
uploader = FileDropContextBuilder(
    label="Upload files"
)
uploader.add_mime_type("application/pdf")
uploader.add_mime_type("text/plain")
payload = uploader.build()
```

### B: Visualization

#### 5. Comparator
```python
comparator = SideBySideComparatorBuilder(
    left_title="Version 1",
    right_title="Version 2",
    left_content="old code",
    right_content="new code",
    content_type="code",
    language="python"
)
payload = comparator.build()
```

#### 6. Interactive Chart
```python
chart = InteractiveDataChartBuilder(
    title="Sales",
    chart_type="line"  # bar, pie, scatter
)
chart.add_data_point(month="Jan", sales=1000)
chart.add_data_point(month="Feb", sales=1200)
payload = chart.build()
```

#### 7. Code Playground
```python
playground = CodePlaygroundBuilder(
    language="python",
    code="print('Hello')",
    allow_edit=True,
    show_preview=True
)
payload = playground.build()
```

#### 8. Geo Map
```python
map_card = GeoSpatialMapCardBuilder()
map_card.add_marker(52.5, 13.4, "Berlin", "Start")
map_card.add_marker(48.8, 2.3, "Paris", "Stop")
map_card.set_center(50.5, 7.5, zoom=6)
payload = map_card.build()
```

### C: Workflow

#### 9. Human Approval
```python
approver = HumanInTheLoopApproverBuilder(
    title="Send email?",
    summary="To 100 users"
)
approver.add_detail("Subject", "Offer")
approver.add_detail("Recipients", "marketing@...")
payload = approver.build()
```

#### 10. Multi-Step Wizard
```python
wizard = MultiStepWizardBuilder()
wizard.add_step("Step 1", "Configure settings")
wizard.add_step("Step 2", "Review & confirm")
wizard.add_step("Step 3", "Complete")
payload = wizard.build()
```

#### 11. List Manager
```python
list_mgr = ListManagerBuilder()
list_mgr.add_item("1", "Item 1", is_checked=True)
list_mgr.add_item("2", "Item 2", is_checked=False)
payload = list_mgr.build()
```

#### 12. Calendar Scheduler
```python
scheduler = CalendarSchedulerBuilder(duration_minutes=60)
scheduler.add_slot("2024-05-15T10:00:00Z", is_available=True)
scheduler.add_slot("2024-05-15T14:00:00Z", is_available=False)
payload = scheduler.build()
```

### D: Context & Meta

#### 13. Reference Rail
```python
rail = ReferenceRailBuilder()
rail.add_source("doc1", "Research.pdf", "pdf", "https://...")
rail.add_source("doc2", "Article", "web", "https://...")
payload = rail.build()
```

#### 14. Thought Expander
```python
expander = ThoughtProcessExpanderBuilder(
    initial_summary="Analyzing 3 documents..."
)
expander.add_step("step1", "Parse docs", "Found 10 concepts")
expander.add_step("step2", "Analyze", "Key insights identified")
payload = expander.build()
```

#### 15. Memory Snapshot
```python
memory = MemorySnapshotBuilder(
    memory_key="skill_level",
    memory_value="Intermediate",
    confirmation_prompt="Adjust difficulty?"
)
payload = memory.build()
```

## 🔧 Integration Patterns

### Simple Response
```python
from a2ui import respond_with_form

response = respond_with_form(
    prompt="Enter your details",
    fields=[
        {"type": "text", "id": "name", "label": "Name"},
        {"type": "email", "id": "email", "label": "Email"},
    ]
)
```

### Advanced Response
```python
from a2ui import AgentContext, AgentResponseBuilder

context = AgentContext(
    user_id="user_123",
    session_id="sess_456",
    task_description="Data analysis"
)

builder = AgentResponseBuilder(context)
builder.text("Choose your analysis type:")
builder.ui_component(SmartSelectorBuilder().build())
response = builder.build()
```

### Progressive Streaming
```python
builder = AgentResponseBuilder()
builder.text("Loading your data...")

builder.queue_component(quick_chart.build())
builder.queue_component(detailed_analysis.build())

payloads = builder.build_streaming()
# Send each payload individually
for payload in payloads:
    send_to_client(payload)
```

## ✔️ Validation

### Quick Validate
```python
from a2ui import validate, ValidationLevel

is_valid, errors = validate(
    component,
    level=ValidationLevel.STRICT
)

if not is_valid:
    print(f"Errors: {errors}")
```

### Full Validator
```python
from a2ui import A2UIPayloadValidator, ValidationLevel

validator = A2UIPayloadValidator(ValidationLevel.MODERATE)
is_valid, errors, warnings = validator.validate_component(component)
```

## 🎯 Common Use Cases

### Use Case 1: Form Input
```python
def handle_booking():
    form = SlotFillingFormBuilder()
    form.add_date_field("date", "Date")
    form.add_number_field("people", "Party size")
    
    response = AgentResponseBuilder()
    response.text("Book your table:")
    response.ui_component(form.build())
    return response.build()
```

### Use Case 2: Decision Making
```python
def handle_choice():
    selector = SmartSelectorBuilder()
    selector.add_option("gpt4", "GPT-4", "Most powerful", "⚡")
    selector.add_option("claude", "Claude", "Balanced", "🧠")
    
    response = AgentResponseBuilder()
    response.text("Which model?")
    response.ui_component(selector.build())
    return response.build()
```

### Use Case 3: Data Display
```python
def handle_analysis():
    chart = InteractiveDataChartBuilder(
        title="Revenue Trend",
        chart_type="line"
    )
    for month, revenue in monthly_data.items():
        chart.add_data_point(month=month, revenue=revenue)
    
    response = AgentResponseBuilder()
    response.text("Here's your analysis:")
    response.ui_component(chart.build())
    return response.build()
```

### Use Case 4: Approval Gate
```python
def handle_critical_action():
    approver = HumanInTheLoopApproverBuilder(
        title="Delete 100 records?",
        summary="This cannot be undone"
    )
    
    response = AgentResponseBuilder()
    response.ui_component(approver.build())
    return response.build()
```

## 🔐 Security Checklist

- ✅ Always validate before sending
```python
is_valid, errors = validate(component, ValidationLevel.STRICT)
```

- ✅ Use builders (type-safe)
```python
form = SlotFillingFormBuilder()  # Safe API
```

- ✅ Never embed user code
```python
# ❌ DON'T
code = user_provided_string
playground = CodePlaygroundBuilder(code=code)

# ✅ DO
code = safe_code_snippet
playground = CodePlaygroundBuilder(code=code)
```

- ✅ Sanitize user text
```python
from html import escape
safe_text = escape(user_text)
```

## 📊 Payload Structure

### Minimal Response
```json
{
  "message_id": "msg_123",
  "text_content": "Hello user",
  "ui_component": null
}
```

### With Component
```json
{
  "message_id": "msg_123",
  "text_content": "Choose option:",
  "ui_component": {
    "type": "choice_chips",
    "options": [
      {"id": "opt1", "label": "Option 1"}
    ],
    "on_select": {"action_id": "option_selected"}
  },
  "metadata": {}
}
```

### Streaming Payload
```json
{
  "message_id": "msg_123",
  "chunk_index": 0,
  "type": "initial",
  "text_content": "Loading...",
  "ui_component": {}
}

// Later:
{
  "message_id": "msg_123",
  "chunk_index": 1,
  "type": "component_stream",
  "component": {}
}
```

## 🚀 Performance Tips

1. **Use Progressive Rendering**
   ```python
   # Fast component first
   builder.queue_component(quick_component)
   # Slow component later
   builder.queue_component(heavy_component)
   ```

2. **Cache Common Builders**
   ```python
   from functools import lru_cache
   
   @lru_cache(maxsize=100)
   def get_model_selector():
       return SmartSelectorBuilder().build()
   ```

3. **Limit Data**
   ```python
   # ✅ Good
   items = data[:100]  # Paginate
   
   # ❌ Bad
   items = all_data  # 1M items = slow
   ```

## 🐛 Debugging

### Print Component
```python
import json
print(json.dumps(component.build(), indent=2))
```

### Check Validation
```python
validator = A2UIPayloadValidator(ValidationLevel.STRICT)
is_valid, errors, warnings = validator.validate_component(component)
print(f"Valid: {is_valid}")
print(f"Errors: {errors}")
print(f"Warnings: {warnings}")
```

### Log Response
```python
import logging
logger = logging.getLogger("a2ui")
logger.info(f"Response: {response.to_json()}")
```

---

**Quick Links:**
- Full Docs: `/docs/A2UI_INTEGRATION.md`
- Examples: `src-python/a2ui/examples.py`
- Architecture: `/docs/A2UI_ARCHITECTURE.md`
