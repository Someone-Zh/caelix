use super::spec::AgentSpec;
use super::spec::AgentMetadata;
use crate::core::tool::file_tools::FileReadTool;
use crate::core::tool::file_tools::FileWriteTool;
use crate::core::tool::file_tools::FileModifyTool;

/// 创建规划专家智能体
pub fn create_planner_agent() -> AgentSpec {
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

    let tools = vec![
        Box::new(FileReadTool::new()),
        Box::new(FileWriteTool::new()),
        Box::new(FileModifyTool::new()),
    ];

    let metadata = AgentMetadata {
        description: "规划专家，负责将任务拆分为多个原子的子任务，并根据任务复杂度分配给不同的专家处理".to_string(),
        version: "1.0.0".to_string(),
        author: Some("Caelix Team".to_string()),
        tags: vec!["planner", "task分解", "任务规划"],
    };

    AgentSpec::new(
        "planner_agent".to_string(),
        metadata,
        system_prompt.to_string(),
        tools,
    )
}

/// 创建收集专家智能体
pub fn create_collector_agent() -> AgentSpec {
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

    let tools = vec![
        Box::new(FileReadTool::new()),
    ];

    let metadata = AgentMetadata {
        description: "收集专家，负责根据任务信息收集相关的代码、文档等外部信息".to_string(),
        version: "1.0.0".to_string(),
        author: Some("Caelix Team".to_string()),
        tags: vec!["collector", "信息收集", "数据获取"],
    };

    AgentSpec::new(
        "collector_agent".to_string(),
        metadata,
        system_prompt.to_string(),
        tools,
    )
}

/// 创建架构专家智能体
pub fn create_architecture_agent() -> AgentSpec {
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

    let tools = vec![
        Box::new(FileWriteTool::new()),
    ];

    let metadata = AgentMetadata {
        description: "架构专家，负责为大型任务设计架构图和具体的子任务分解".to_string(),
        version: "1.0.0".to_string(),
        author: Some("Caelix Team".to_string()),
        tags: vec!["architecture", "架构设计", "任务分解"],
    };

    AgentSpec::new(
        "architecture_agent".to_string(),
        metadata,
        system_prompt.to_string(),
        tools,
    )
}

/// 创建代码操作执行者智能体
pub fn create_code_executor_agent() -> AgentSpec {
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
"#;

    let tools = vec![
        Box::new(FileReadTool::new()),
        Box::new(FileWriteTool::new()),
        Box::new(FileModifyTool::new()),
    ];

    let metadata = AgentMetadata {
        description: "代码操作执行者，负责执行与代码相关的任务，拥有文件系统权限和工具".to_string(),
        version: "1.0.0".to_string(),
        author: Some("Caelix Team".to_string()),
        tags: vec!["executor", "code", "文件操作"],
    };

    AgentSpec::new(
        "code_executor_agent".to_string(),
        metadata,
        system_prompt.to_string(),
        tools,
    )
}

/// 创建浏览器操作执行者智能体
pub fn create_browser_executor_agent() -> AgentSpec {
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

    let tools = vec![];

    let metadata = AgentMetadata {
        description: "浏览器操作执行者，负责执行与浏览器相关的任务".to_string(),
        version: "1.0.0".to_string(),
        author: Some("Caelix Team".to_string()),
        tags: vec!["executor", "browser", "网页操作"],
    };

    AgentSpec::new(
        "browser_executor_agent".to_string(),
        metadata,
        system_prompt.to_string(),
        tools,
    )
}

/// 创建UI操作执行者智能体
pub fn create_ui_executor_agent() -> AgentSpec {
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

    let tools = vec![];

    let metadata = AgentMetadata {
        description: "UI操作执行者，负责执行与UI相关的任务".to_string(),
        version: "1.0.0".to_string(),
        author: Some("Caelix Team".to_string()),
        tags: vec!["executor", "ui", "界面操作"],
    };

    AgentSpec::new(
        "ui_executor_agent".to_string(),
        metadata,
        system_prompt.to_string(),
        tools,
    )
}

/// 注册所有角色智能体到注册中心
pub async fn register_all_agents(registry: &crate::core::agent::AgentRegistry) -> Result<(), crate::core::agent::registry::AgentRegistryError> {
    registry.register(create_planner_agent()).await?;
    registry.register(create_collector_agent()).await?;
    registry.register(create_architecture_agent()).await?;
    registry.register(create_code_executor_agent()).await?;
    registry.register(create_browser_executor_agent()).await?;
    registry.register(create_ui_executor_agent()).await?;
    Ok(())
}