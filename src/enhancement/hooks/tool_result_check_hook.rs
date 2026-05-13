use crate::enhancement::hooks::{AgentHook, HookCapability, PostToolExecContext};
use async_trait::async_trait;

const MAX_RESULT_SIZE: usize = 1024; // 1KB

/// ToolResultSizeCheckHook - 检查工具结果大小，超过1KB则截断
pub struct ToolResultSizeCheckHook;

impl ToolResultSizeCheckHook {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AgentHook for ToolResultSizeCheckHook {
    fn name(&self) -> &str {
        "tool_result_size_check_hook"
    }
    
    fn capabilities(&self) -> HookCapability {
        HookCapability::POST_TOOL_EXEC
    }
    
    async fn on_post_tool_exec(&self, ctx: &mut PostToolExecContext) -> Result<(), anyhow::Error> {
        // 只处理成功的结果
        if ctx.tool_result.error.is_some() {
            return Ok(());
        }
        
        let output_len = ctx.tool_result.output.len();
        
        if output_len > MAX_RESULT_SIZE {
            // 截断到1KB，确保不切断UTF-8字符
            let mut truncate_at = MAX_RESULT_SIZE;
            
            // 取出output以避免借用问题
            let output_clone = std::mem::take(&mut ctx.tool_result.output);
            
            // 向前查找，确保在字符边界处截断
            while truncate_at > 0 && !output_clone.is_char_boundary(truncate_at) {
                truncate_at -= 1;
            }
            
            let truncated = &output_clone[..truncate_at];
            ctx.tool_result.output = format!("{}\n\n[内容过多无法全部显示]", truncated);
            
            #[cfg(feature = "logging")]
            {
                crate::debug_log!(
                    "WARN",
                    &ctx.base.session_id,
                    &ctx.base.request_id,
                    &ctx.base.span_id,
                    &format!("tool_result_check_hook.rs:{}", line!()),
                    serde_json::json!({
                        "event": "tool_result_truncated",
                        "tool_name": ctx.tool_name,
                        "original_size": output_len,
                        "truncated_size": truncate_at
                    })
                );
            }
        }
        
        Ok(())
    }
}
