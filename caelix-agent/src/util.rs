use caelix_api::{ChatMessage, ToolCall};

/// 检查消息列表的最后一条消息是否包含未执行的 tool calls
pub fn has_pending_tool_calls(messages: &[ChatMessage]) -> bool {
    messages.last().is_some_and(|msg| {
        msg.tool_calls.is_some() && !msg.tool_calls.as_ref().unwrap().is_empty()
    })
}

/// 从最后一条消息中提取待执行的 tool calls
pub fn extract_pending_tool_calls(messages: &[ChatMessage]) -> Option<Vec<ToolCall>> {
    messages
        .last()
        .and_then(|msg| msg.tool_calls.clone().filter(|calls| !calls.is_empty()))
}
