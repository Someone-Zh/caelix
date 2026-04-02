use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use crate::core::agent::executor::AgentExecutor;
use crate::core::AgentError;
use crate::runtime::message::{MessageBus, Message};
use super::{Task, TaskStatus, TaskType, AgentTaskContext};

/// 任务调度器
#[derive(Debug)]
pub struct TaskScheduler {
    /// 任务映射
    tasks: Arc<Mutex<HashMap<String, Task>>>,
    /// 待执行任务队列
    pending_tasks: Arc<Mutex<VecDeque<String>>>,
    /// 并发控制信号量
    semaphore: Arc<Semaphore>,
    /// Agent 执行器
    agent_executor: AgentExecutor,
    /// 消息总线
    message_bus: Option<Arc<MessageBus>>,
}

impl TaskScheduler {
    /// 创建新的任务调度器
    pub fn new(concurrency_limit: usize, agent_executor: AgentExecutor) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            pending_tasks: Arc::new(Mutex::new(VecDeque::new())),
            semaphore: Arc::new(Semaphore::new(concurrency_limit)),
            agent_executor,
            message_bus: None,
        }
    }
    
    /// 设置消息总线
    pub fn set_message_bus(&mut self, message_bus: Arc<MessageBus>) {
        self.message_bus = Some(message_bus);
    }

    /// 添加任务
    pub fn add_task(&mut self, task: Task) -> Result<(), AgentError> {
        // 检查依赖任务是否存在
        if let Some(parent_id) = &task.parent_id {
            let tasks = self.tasks.lock().unwrap();
            if !tasks.contains_key(parent_id) {
                return Err(AgentError::TaskError(format!("Parent task {} not found", parent_id)));
            }
        }

        let task_id = task.id.clone();
        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.insert(task_id.clone(), task);
        }

        // 将任务加入待执行队列
        let mut pending_tasks = self.pending_tasks.lock().unwrap();
        pending_tasks.push_back(task_id);

        Ok(())
    }

    /// 开始执行任务
    pub async fn start(&self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();
        let pending_tasks = self.pending_tasks.lock().unwrap().clone();

        for task_id in pending_tasks {
            let tasks = self.tasks.clone();
            let semaphore = self.semaphore.clone();
            let agent_executor = self.agent_executor.clone();
            let message_bus = self.message_bus.clone();

            let handle = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                
                // 更新任务状态为运行中
                {
                    let mut tasks = tasks.lock().unwrap();
                    if let Some(task) = tasks.get_mut(&task_id) {
                        task.status = TaskStatus::Running;
                    }
                }

                // 执行任务
                let result = Self::execute_task(&tasks, &agent_executor, &task_id).await;

                // 更新任务状态
                {
                    let mut tasks = tasks.lock().unwrap();
                    if let Some(task) = tasks.get_mut(&task_id) {
                        task.status = match result {
                            Ok(_) => TaskStatus::Completed,
                            Err(_) => TaskStatus::Failed,
                        };
                        
                        // 发布任务完成消息
                        if let Some(message_bus) = &message_bus {
                            let task_clone = task.clone();
                            let status_str = format!("{:?}", task.status);
                            let content = format!("Task {} completed with status: {}", task.id, status_str);
                            
                            let message = Message {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: crate::core::llm::MessageRole::System,
                                content,
                                tool_calls: vec![],
                                timestamp: chrono::Utc::now().timestamp(),
                                session_id: task.session_id.clone(),
                                belongs_to: Some(task.id.clone()),
                            };
                            
                            let bus_clone = message_bus.clone();
                            tokio::spawn(async move {
                                bus_clone.publish(&task_clone.session_id, message).await;
                            });
                        }
                    }
                }
            });

            handles.push(handle);
        }

        handles
    }

    /// 执行单个任务
    async fn execute_task(
        tasks: &Arc<Mutex<HashMap<String, Task>>>,
        agent_executor: &AgentExecutor,
        task_id: &str,
    ) -> Result<(), AgentError> {
        let task = {
            let tasks = tasks.lock().unwrap();
            tasks.get(task_id).cloned().ok_or(AgentError::TaskError(format!("Task {} not found", task_id)))?
        };

        match task.task_type {
            TaskType::Agent => {
                // 执行 Agent 任务
                if let Some(agent_context) = task.context.downcast_ref::<AgentTaskContext>() {
                    let _ = agent_executor.execute(&agent_context.agent_name, agent_context.messages.clone()).await?;
                } else {
                    return Err(AgentError::TaskError("Invalid agent task context".to_string()));
                }
            }
        }

        Ok(())
    }

    /// 获取任务状态
    pub fn get_task_status(&self, task_id: &str) -> Option<TaskStatus> {
        let tasks = self.tasks.lock().unwrap();
        tasks.get(task_id).map(|task| task.status.clone())
    }

    /// 获取所有任务
    pub fn get_all_tasks(&self) -> HashMap<String, Task> {
        self.tasks.lock().unwrap().clone()
    }
}