
use crate::manager::AgentRegistryError;
use crate::base::agent::AgentSpec;
use crate::config::CaelixContext;
use crate::manager::ToolManager;

/// 创建规划专家智能体
pub async  fn create_planner_agent(tool_manager: &ToolManager) -> AgentSpec {
    let system_prompt = r#"
你是一名专业的规划专家，擅长将复杂任务拆解为可执行的子任务。

你的职责：
1. 分析用户提出的任务，理解其目标和要求
2. 将任务拆分为多个原子的子任务
3. 将需要收集信息的子任务分发给收集专家
4. 收到收集专家的信息后，根据信息进行详细的任务规划
5. 对于大型任务，将其交给架构专家生成架构图和具体子任务
6. 对于小型任务，直接交给执行者进行处理
7. 确保子任务之间的依赖关系清晰，原子任务支持并发执行，非原子任务基于依赖顺序串行执行

你需要：
- 确保任务拆分合理，覆盖所有必要的步骤
- 评估任务的复杂度，决定是否需要架构专家的参与
- 为不同类型的子任务选择合适的执行者
- 提供清晰的任务描述和执行顺序
"#;
    let diff_edit_tool = tool_manager.get("diff_edit").await.unwrap();
    let global_file_search_tool = tool_manager.get("global_file_search").await.unwrap();
    let directory_tree_tool = tool_manager.get("directory_tree").await.unwrap();
    let tools = vec![
        diff_edit_tool,
        global_file_search_tool,
        directory_tree_tool,
    ];

    AgentSpec::new(
        "planner_agent".to_string(),
        system_prompt.to_string(),
        tools,
    )
}

/// 创建收集专家智能体
pub async fn create_collector_agent(tool_manager: &ToolManager) -> AgentSpec {
    let system_prompt = r#"
你是一名专业的收集专家，擅长根据任务信息收集相关的外部信息。

你的职责：
1. 接收规划专家分配的信息收集任务
2. 根据任务要求，收集相关的代码、文档、数据等外部信息
3. 确保收集到的信息全面、准确、相关
4. 将收集到的信息整理后返回给规划专家

你需要：
- 明确信息收集的范围和目标
- 利用各种工具和资源获取所需信息
- 对收集到的信息进行筛选和整理
- 确保信息的时效性和可靠性
"#;

    let diff_edit_tool = tool_manager.get("diff_edit").await.unwrap();
    let global_file_search_tool = tool_manager.get("global_file_search").await.unwrap();
    let directory_tree_tool = tool_manager.get("directory_tree").await.unwrap();
    let tools = vec![
        diff_edit_tool,
        global_file_search_tool,
        directory_tree_tool,
    ];

    AgentSpec::new(
        "collector_agent".to_string(),
        system_prompt.to_string(),
        tools,
    )
}

/// 创建架构专家智能体
pub async fn create_architecture_agent(tool_manager: &ToolManager) -> AgentSpec {
    let system_prompt = r#"
你是一名专业的架构专家，擅长为大型任务设计架构图和具体的子任务。

你的职责：
1. 接收规划专家分配的大型任务
2. 分析任务的需求和约束
3. 设计合理的架构方案，包括组件、接口、数据流等
4. 生成详细的架构图
5. 将大型任务分解为具体的子任务
6. 确定子任务之间的依赖关系
7. 确保原子任务支持并发执行，非原子任务基于依赖顺序串行执行

你需要：
- 确保架构设计符合最佳实践
- 考虑系统的可扩展性、可维护性和性能
- 提供清晰的子任务分解和执行顺序
- 确保子任务的粒度适当，便于执行和管理
"#;

    let diff_edit_tool = tool_manager.get("diff_edit").await.unwrap();
    let global_file_search_tool = tool_manager.get("global_file_search").await.unwrap();
    let directory_tree_tool = tool_manager.get("directory_tree").await.unwrap();
    let tools = vec![
        diff_edit_tool,
        global_file_search_tool,
        directory_tree_tool,
    ];

    AgentSpec::new(
        "architecture_agent".to_string(),
        system_prompt.to_string(),
        tools,
    )
}

/// 创建代码操作执行者智能体
pub async fn create_code_executor_agent(tool_manager: &ToolManager) -> AgentSpec {
    let system_prompt = r#"
你是一名专业的代码操作执行者，擅长执行与代码相关的任务。

你的职责：
1. 接收规划专家或架构专家分配的代码操作任务
2. 利用文件系统工具执行具体的代码操作
3. 确保代码操作的准确性和完整性
4. 及时反馈执行结果

你需要：
- 熟练使用文件系统工具进行文件读写和修改
- 确保代码操作符合要求
- 处理执行过程中遇到的问题
- 提供清晰的执行结果反馈
- 当工具执行失败时，可以尝试别的方法但是最多3次都失败则结束
"#;

    let diff_edit_tool = tool_manager.get("diff_edit").await.unwrap();
    let global_file_search_tool = tool_manager.get("global_file_search").await.unwrap();
    let directory_tree_tool = tool_manager.get("directory_tree").await.unwrap();
    let tools = vec![
        diff_edit_tool,
        global_file_search_tool,
        directory_tree_tool,
    ];
    AgentSpec::new(
        "code_executor_agent".to_string(),
        system_prompt.to_string(),
        tools,
    )
}

/// 创建浏览器操作执行者智能体
pub async  fn create_browser_executor_agent(tool_manager: &ToolManager) -> AgentSpec {
    let system_prompt = r#"
你是一名专业的浏览器操作执行者，擅长执行与浏览器相关的任务。

你的职责：
1. 接收规划专家分配的浏览器操作任务
2. 执行浏览器相关的操作，如网页访问、信息获取等
3. 确保浏览器操作的准确性和完整性
4. 及时反馈执行结果

你需要：
- 熟练使用浏览器工具进行网页操作
- 确保操作符合要求
- 处理执行过程中遇到的问题
- 提供清晰的执行结果反馈
"#;

    let diff_edit_tool = tool_manager.get("diff_edit").await.unwrap();
    let global_file_search_tool = tool_manager.get("global_file_search").await.unwrap();
    let directory_tree_tool = tool_manager.get("directory_tree").await.unwrap();
    let tools = vec![
        diff_edit_tool,
        global_file_search_tool,
        directory_tree_tool,
    ];

    AgentSpec::new(
        "browser_executor_agent".to_string(),
        system_prompt.to_string(),
        tools,
    )
}

/// 创建UI操作执行者智能体
pub async fn create_ui_executor_agent(tool_manager: &ToolManager) -> AgentSpec {
    let system_prompt = r#"
你是一名专业的UI操作执行者，擅长执行与UI相关的任务。

你的职责：
1. 接收规划专家分配的UI操作任务
2. 执行UI相关的操作，如界面交互、元素操作等
3. 确保UI操作的准确性和完整性
4. 及时反馈执行结果

你需要：
- 熟练使用UI工具进行界面操作
- 确保操作符合要求
- 处理执行过程中遇到的问题
- 提供清晰的执行结果反馈
"#;
    let diff_edit_tool = tool_manager.get("diff_edit").await.unwrap();
    let global_file_search_tool = tool_manager.get("global_file_search").await.unwrap();
    let directory_tree_tool = tool_manager.get("directory_tree").await.unwrap();
    let tools = vec![
        diff_edit_tool,
        global_file_search_tool,
        directory_tree_tool,
    ];
    AgentSpec::new(
        "ui_executor_agent".to_string(),
        system_prompt.to_string(),
        tools,
    )
}

/// 注册所有角色智能体到注册中心
pub async fn register_all_agents(context: &CaelixContext) -> Result<(), AgentRegistryError> {
    let agent_manager = context.agent_manager.clone();
    let tool_manager = context.tool_manager.clone();
    agent_manager.register(create_planner_agent(&tool_manager).await).await?;
    agent_manager.register(create_collector_agent(&tool_manager).await).await?;
    agent_manager.register(create_architecture_agent(&tool_manager).await).await?;
    agent_manager.register(create_code_executor_agent(&tool_manager).await).await?;
    agent_manager.register(create_browser_executor_agent(&tool_manager).await).await?;
    agent_manager.register(create_ui_executor_agent(&tool_manager).await).await?;
    Ok(())
}