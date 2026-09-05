-- Enable Extensions
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS age;
LOAD 'age';
SET search_path = ag_catalog, "$user", public;

-- Initialize Graph
SELECT create_graph('atlas_graph');

-- 1. Tool Executions (Execution Provenance)
CREATE TABLE IF NOT EXISTS tool_executions (
    _key UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT,
    tool_name TEXT,
    input_args JSONB,
    output_summary TEXT,
    status TEXT CHECK (status IN ('success', 'error', 'timeout')),
    latency_ms INTEGER,
    error_message TEXT,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);

-- Edges for causal relationships (simulated in relational for now, can be graph edges too)
CREATE TABLE IF NOT EXISTS tool_execution_relationships (
    from_execution_id UUID REFERENCES tool_executions(_key),
    to_execution_id UUID REFERENCES tool_executions(_key),
    relationship TEXT,
    PRIMARY KEY (from_execution_id, to_execution_id)
);

-- 2. Managed Tools Resource
CREATE TABLE IF NOT EXISTS tools (
    tool_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    version TEXT NOT NULL,
    schema_definition JSONB NOT NULL,
    execution_logic JSONB,
    created_by_agent_id UUID,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    performance_stats JSONB DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS agent_tool_permissions (
    agent_id UUID NOT NULL,
    tool_id UUID NOT NULL REFERENCES tools(tool_id),
    allowed_operations TEXT[] DEFAULT ARRAY['execute'],
    policy_rules JSONB,
    PRIMARY KEY (agent_id, tool_id)
);

-- 3. Learnings (Knowledge Base)
CREATE TABLE IF NOT EXISTS learnings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content TEXT NOT NULL,
    embedding vector(1536), -- Assuming OpenAI ada-002 or similar
    tags TEXT[],
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB
);

-- Index for vector search
CREATE INDEX ON learnings USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);

-- 4. Roles & Security
DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'ai_research') THEN
    CREATE ROLE ai_research WITH LOGIN;
  END IF;
  IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = 'ai_admin') THEN
    CREATE ROLE ai_admin WITH LOGIN;
  END IF;
END
$$;

GRANT SELECT, INSERT ON tool_executions TO ai_research;
GRANT SELECT ON tools TO ai_research;
GRANT ALL ON learnings TO ai_research;

GRANT ALL ON ALL TABLES IN SCHEMA public TO ai_admin;

-- 5. LangGraph Persistence (Checkpoints)
CREATE TABLE IF NOT EXISTS checkpoints (
    thread_id TEXT NOT NULL,
    checkpoint_ns TEXT NOT NULL DEFAULT '',
    checkpoint_id TEXT NOT NULL,
    parent_checkpoint_id TEXT,
    type TEXT,
    checkpoint JSONB NOT NULL,
    metadata JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id)
);

CREATE TABLE IF NOT EXISTS checkpoint_writes (
    thread_id TEXT NOT NULL,
    checkpoint_ns TEXT NOT NULL DEFAULT '',
    checkpoint_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    channel TEXT NOT NULL,
    type TEXT,
    value JSONB NOT NULL, -- Blob/JSON
    PRIMARY KEY (thread_id, checkpoint_ns, checkpoint_id, task_id, idx)
);

GRANT ALL ON checkpoints TO ai_research;
GRANT ALL ON checkpoint_writes TO ai_research;
GRANT ALL ON checkpoints TO ai_admin;
GRANT ALL ON checkpoint_writes TO ai_admin;

-- 6. App Store & Quota Management

CREATE TABLE IF NOT EXISTS providers (
    id TEXT PRIMARY KEY, -- 'openai', 'anthropic', 'local'
    name TEXT NOT NULL,
    base_url TEXT,
    is_local BOOLEAN DEFAULT FALSE,
    config JSONB -- Auth methods, etc.
);

CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY, -- 'gpt-4o', 'gemini-1.5-pro'
    provider_id TEXT REFERENCES providers(id),
    name TEXT NOT NULL,
    capabilities TEXT[], -- ['text', 'image', 'audio']
    input_cost_per_1k NUMERIC(10, 6),
    output_cost_per_1k NUMERIC(10, 6),
    image_cost_per_unit NUMERIC(10, 6),
    is_active BOOLEAN DEFAULT TRUE
);

-- Apps table extension (if using purely relational, otherwise relying on JSON in 'apps' table from SQLAlchemy)
-- Since 'apps' might be created by SQLAlchemy, we should ensure this schema is compatible.
-- We will rely on the SQLAlchemy 'apps' definition but add the usage tracking here.

CREATE TABLE IF NOT EXISTS app_usage (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    app_id TEXT, -- Corresponds to App.id (String/UUID)
    model_id TEXT REFERENCES models(id),
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    prompt_tokens INTEGER DEFAULT 0,
    completion_tokens INTEGER DEFAULT 0,
    total_tokens INTEGER DEFAULT 0,
    cost_usd NUMERIC(10, 6) DEFAULT 0,
    request_type TEXT -- 'chat', 'embedding', 'image'
);

CREATE INDEX ON app_usage (app_id);
CREATE INDEX ON app_usage (timestamp);

-- Seed Default Providers
INSERT INTO providers (id, name, is_local) VALUES
('openai', 'OpenAI', FALSE),
('anthropic', 'Anthropic', FALSE),
('gemini', 'Google Gemini', FALSE),
('local', 'Local LLM', TRUE)
ON CONFLICT (id) DO NOTHING;

-- Seed Default Models (Example)
INSERT INTO models (id, provider_id, name, capabilities, input_cost_per_1k, output_cost_per_1k) VALUES
('gpt-4o', 'openai', 'GPT-4o', ARRAY['text', 'image'], 0.005, 0.015),
('gpt-4o-mini', 'openai', 'GPT-4o Mini', ARRAY['text'], 0.00015, 0.0006),
('claude-3-5-sonnet-20240620', 'anthropic', 'Claude 3.5 Sonnet', ARRAY['text'], 0.003, 0.015),
('gemini-1.5-pro', 'gemini', 'Gemini 1.5 Pro', ARRAY['text', 'image', 'audio'], 0.0035, 0.0105)
ON CONFLICT (id) DO NOTHING;

