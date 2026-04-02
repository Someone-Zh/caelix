// --- 钩子上下文 ---
// 用于在钩子之间传递状态
pub struct HookContext {
    pub session_id: SessionId,
    pub current_messages: Vec<Message>,
    pub metadata: HashMap<String, String>,
}

// --- 钩子接口 ---
// 对应架构：第二层 - 增强层
#[async_trait]
pub trait Hook: Send + Sync {
    // 在 LLM 推理前执行（如：Prompt 注入、敏感词过滤）
    async fn pre_llm(&self, ctx: &mut HookContext) -> Result<(), AgentError> {
        Ok(())
    }

    // 在工具执行前执行（如：权限校验）
    async fn pre_tool_call(&self, tool_name: &str, args: &serde_json::Value) -> Result<(), AgentError> {
        Ok(())
    }

    // 在 LLM 推理后执行（如：结果格式化）
    async fn post_llm(&self, ctx: &mut HookContext, response: &str) -> Result<(), AgentError> {
        Ok(())
    }
}

// --- 技能定义 ---
// 技能本质上是“预定义的 Prompt 模板” + “一组允许使用的工具”
#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub system_prompt_template: String,
    pub allowed_tool_ids: Vec<ToolId>,
}