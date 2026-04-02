# caelix
天庭
├── Cargo.toml                # 项目根配置 (依赖管理)
├── README.md                 # 项目文档
├── .env                      # 环境变量 (API Keys, DB URLs)
├── docker-compose.yml        # 本地开发依赖 (Redis, Postgres, VectorDB)
│
├── src/
│   ├── main.rs               # 程序入口 (初始化依赖注入，启动服务)
│   ├── lib.rs                # 库根 (导出公共接口，方便测试)
│   │
│   ├── config/               # [配置层] 全局配置加载
│   │   ├── mod.rs
│   │   └── settings.rs       # 解析 .env 和 config.yaml
│   │
│   ├── core/                 # [第一层：核心层] 原子能力
│   │   ├── mod.rs
│   │   ├── llm/              # LLM 提供者实现
│   │   │   ├── mod.rs
│   │   │   ├── trait.rs      # 定义 LlmProvider Trait
│   │   │   ├── openai.rs     # OpenAI 实现
│   │   │   └── ollama.rs     # Ollama 实现
│   │   ├── tools/            # 工具定义与执行
│   │   │   ├── mod.rs
│   │   │   ├── trait.rs      # 定义 Tool Trait
│   │   │   ├── registry.rs   # 工具注册中心
│   │   │   └── builtins/     # 内置工具 (Search, Calculator, etc.)
│   │   └── models/           # 基础数据结构 (Message, Role, ToolCall)
│   │       └── mod.rs
│   │
│   ├── enhancement/          # [第二层：增强层] 技能与钩子
│   │   ├── mod.rs
│   │   ├── hooks/            # 钩子实现
│   │   │   ├── mod.rs
│   │   │   ├── trait.rs      # 定义 Hook Trait
│   │   │   ├── auth.rs       # 鉴权钩子
│   │   │   ├── logging.rs    # 日志钩子
│   │   │   └── safety.rs     # 敏感词过滤钩子
│   │   └── skills/           # 技能编排
│   │       ├── mod.rs
│   │       └── manager.rs    # 技能加载与管理
│   │
│   ├── runtime/              # [第三层：运行时管理层] 状态与编排
│   │   ├── mod.rs
│   │   ├── memory/           # 记忆系统
│   │   │   ├── mod.rs
│   │   │   ├── trait.rs      # MemoryManager Trait
│   │   │   ├── short_term.rs # 短期上下文 (In-Memory/Redis)
│   │   │   └── long_term.rs  # 长期记忆 (VectorDB/Pgvector)
│   │   ├── session/          # 会话管理
│   │   │   └── mod.rs
│   │   ├── tasks/            # 子任务调度
│   │   │   ├── mod.rs
│   │   │   ├── scheduler.rs  # 任务队列与状态机
│   │   │   └── executor.rs   # 任务执行器
│   │   ├── bus/              # 消息总线
│   │   │   └── mod.rs        # 基于 tokio::broadcast 或 Redis PubSub
│   │   └── agent.rs          # 【核心】Agent 运行主逻辑 (组装各层)
│   │
│   ├── api/                  # [第四层：服务接入层] 对外接口
│   │   ├── mod.rs
│   │   ├── handlers/         # 请求处理函数
│   │   │   ├── chat.rs       # 聊天接口
│   │   │   └── health.rs     # 健康检查
│   │   ├── routes.rs         # 路由定义
│   │   ├── ws.rs             # WebSocket 处理器 (流式输出)
│   │   └── dto.rs            # 数据传输对象 (Request/Response 结构)
│   │
│   ├── utils/                # 通用工具函数
│   │   ├── mod.rs
│   │   ├── error.rs          # 统一错误定义 (AgentError)
│   │   └── tracing.rs        # 链路追踪初始化
│   │
│   └── bin/                  # 额外的可执行文件 (可选)
│       └── cli.rs            # 命令行调试工具
│
├── tests/                    # 集成测试
│   ├── common/               # 测试辅助代码
│   ├── e2e_chat_test.rs      # 端到端聊天测试
│   └── tool_execution_test.rs# 工具执行测试
│
├── migrations/               # 数据库迁移脚本 (如果使用 SQLx/SeaORM)
│   └── 20231027_create_sessions.sql
│
└── scripts/                  # 运维脚本
    └── deploy.sh