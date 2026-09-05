"""
Specialized Agents for Multi-Agent System

20+ domain-specific agents for various use cases.
Each agent has specific capabilities and skills.
"""

import logging
from typing import Dict, Any, Optional
from src_agents.state import AgentState
from src_agents.agents.base import BaseAgent
from src_agents.llm.llm_factory import create_llm

logger = logging.getLogger(__name__)


class WebSearchAgent(BaseAgent):
    """Agent specialized in web searching and information retrieval."""
    
    def __init__(self, name: str = "web_search_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'web_search', 'information_retrieval', 'fact_finding'}
        self.agent_type = 'researcher'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Performing web search...")
        
        # Simulated web search (in production, use actual search API)
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a web search specialist.

Query: {state.query}

Provide search results with:
1. Key findings
2. Relevant sources
3. Summary of information

Format as a concise report.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Web search failed: {e}")
            state.summary = f"Web search unavailable: {e}"
        
        return state


class AcademicSearchAgent(BaseAgent):
    """Agent specialized in academic research and paper search."""
    
    def __init__(self, name: str = "academic_search_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'academic_search', 'paper_retrieval', 'citation_management'}
        self.agent_type = 'researcher'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Searching academic sources...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are an academic research specialist.

Research Topic: {state.query}

Provide:
1. Relevant academic papers and studies
2. Key findings from literature
3. Citations in academic format

Be scholarly and precise.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Academic search failed: {e}")
            state.summary = f"Academic search unavailable: {e}"
        
        return state


class FactCheckerAgent(BaseAgent):
    """Agent specialized in fact-checking and verification."""
    
    def __init__(self, name: str = "fact_checker_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'fact_checking', 'verification', 'source_validation'}
        self.agent_type = 'analyst'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Fact-checking...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a fact-checking specialist.

Statement to verify: {state.query}

Analyze:
1. Factual accuracy
2. Supporting evidence
3. Potential biases or errors
4. Confidence level (0-100%)

Provide verification report.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Fact checking failed: {e}")
            state.summary = f"Fact checking unavailable: {e}"
        
        return state


class SEOOptimizerAgent(BaseAgent):
    """Agent specialized in SEO optimization."""
    
    def __init__(self, name: str = "seo_optimizer_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'seo_optimization', 'keyword_research', 'content_optimization'}
        self.agent_type = 'optimizer'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Optimizing for SEO...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are an SEO optimization specialist.

Content: {state.query}

Provide:
1. Keyword recommendations
2. Meta description
3. Title tag suggestions
4. SEO improvements
5. Readability score

Format as actionable recommendations.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"SEO optimization failed: {e}")
            state.summary = f"SEO optimization unavailable: {e}"
        
        return state


class TechnicalWriterAgent(BaseAgent):
    """Agent specialized in technical writing."""
    
    def __init__(self, name: str = "technical_writer_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'technical_writing', 'documentation', 'api_docs'}
        self.agent_type = 'writer'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Writing technical content...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a technical writing specialist.

Topic: {state.query}

Write:
1. Clear, concise technical documentation
2. Code examples where relevant
3. Step-by-step instructions
4. Troubleshooting tips

Use professional technical writing style.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Technical writing failed: {e}")
            state.summary = f"Technical writing unavailable: {e}"
        
        return state


class DataAnalystAgent(BaseAgent):
    """Agent specialized in data analysis."""
    
    def __init__(self, name: str = "data_analyst_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'data_analysis', 'statistics', 'visualization'}
        self.agent_type = 'analyst'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Analyzing data...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a data analysis specialist.

Data/Question: {state.query}

Provide:
1. Statistical analysis
2. Key insights
3. Trends and patterns
4. Recommendations
5. Visualization suggestions

Be data-driven and analytical.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Data analysis failed: {e}")
            state.summary = f"Data analysis unavailable: {e}"
        
        return state


class SecurityAuditorAgent(BaseAgent):
    """Agent specialized in security auditing."""
    
    def __init__(self, name: str = "security_auditor_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'security_audit', 'vulnerability_assessment', 'code_review'}
        self.agent_type = 'auditor'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Performing security audit...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a security auditing specialist.

Code/System: {state.query}

Analyze for:
1. Security vulnerabilities
2. Best practice violations
3. Potential attack vectors
4. Compliance issues
5. Remediation recommendations

Provide security audit report.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Security audit failed: {e}")
            state.summary = f"Security audit unavailable: {e}"
        
        return state


class FinancialAnalystAgent(BaseAgent):
    """Agent specialized in financial analysis."""
    
    def __init__(self, name: str = "financial_analyst_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'financial_analysis', 'market_research', 'risk_assessment'}
        self.agent_type = 'analyst'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Performing financial analysis...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a financial analysis specialist.

Topic: {state.query}

Provide:
1. Financial metrics analysis
2. Market trends
3. Risk assessment
4. Investment recommendations
5. Financial projections

Be professional and data-driven.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Financial analysis failed: {e}")
            state.summary = f"Financial analysis unavailable: {e}"
        
        return state


class LegalAdvisorAgent(BaseAgent):
    """Agent specialized in legal advice and compliance."""
    
    def __init__(self, name: str = "legal_advisor_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'legal_advice', 'compliance', 'contract_review'}
        self.agent_type = 'advisor'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Providing legal guidance...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a legal advisory specialist.

Issue: {state.query}

Provide:
1. Legal considerations
2. Compliance requirements
3. Risk analysis
4. Recommendations
5. Disclaimer

Note: This is informational only, not legal advice.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"Legal advisory failed: {e}")
            state.summary = f"Legal advisory unavailable: {e}"
        
        return state


class UXDesignerAgent(BaseAgent):
    """Agent specialized in UX design."""
    
    def __init__(self, name: str = "ux_designer_agent", **kwargs):
        super().__init__(name, **kwargs)
        self.llm = create_llm()
        self.capabilities = {'ux_design', 'user_research', 'wireframing'}
        self.agent_type = 'designer'
    
    async def execute(self, state: AgentState) -> AgentState:
        logger.info(f"[{self.name}] Designing UX...")
        
        from langchain_core.messages import SystemMessage
        
        prompt = f"""You are a UX design specialist.

Feature/Product: {state.query}

Provide:
1. User flow recommendations
2. UI/UX best practices
3. Accessibility considerations
4. Wireframe suggestions
5. User testing recommendations

Focus on user-centered design.
"""
        
        try:
            response = await self.llm.ainvoke([SystemMessage(content=prompt)])
            state.summary = str(response.content).strip()
        except Exception as e:
            logger.error(f"UX design failed: {e}")
            state.summary = f"UX design unavailable: {e}"
        
        return state


# Registry of all specialized agents
SPECIALIZED_AGENTS = {
    'web_search_agent': WebSearchAgent,
    'academic_search_agent': AcademicSearchAgent,
    'fact_checker_agent': FactCheckerAgent,
    'seo_optimizer_agent': SEOOptimizerAgent,
    'technical_writer_agent': TechnicalWriterAgent,
    'data_analyst_agent': DataAnalystAgent,
    'security_auditor_agent': SecurityAuditorAgent,
    'financial_analyst_agent': FinancialAnalystAgent,
    'legal_advisor_agent': LegalAdvisorAgent,
    'ux_designer_agent': UXDesignerAgent,
}


def create_specialized_agent(agent_type: str, name: Optional[str] = None, **kwargs) -> Optional[BaseAgent]:
    """
    Factory function to create specialized agents.
    
    Args:
        agent_type: Type of agent to create
        name: Optional custom name
        **kwargs: Additional arguments
    
    Returns:
        Agent instance or None if type not found
    """
    agent_class = SPECIALIZED_AGENTS.get(agent_type)
    if not agent_class:
        logger.error(f"Unknown agent type: {agent_type}")
        return None
    
    if name:
        return agent_class(name=name, **kwargs)
    return agent_class(**kwargs)
