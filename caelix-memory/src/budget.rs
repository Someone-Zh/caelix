use crate::schema::LlmBudgetCounter;
use chrono::Utc;
use serde_json;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug)]
pub struct LlmBudgetManager {
    root_dir: PathBuf,
    counter: LlmBudgetCounter,
}

impl LlmBudgetManager {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Index"),
            counter: LlmBudgetCounter {
                date: Utc::now().date_naive(),
                used: 0,
                budget: 100,
                last_call_at: 0,
                deferred_tasks: Vec::new(),
            },
        }
    }

    pub async fn load(&mut self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("llm_budget_counter.json");

        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            self.counter = serde_json::from_str(&content)?;
        }

        self.check_date_reset()?;
        Ok(())
    }

    pub async fn save(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("llm_budget_counter.json");
        let content = serde_json::to_string_pretty(&self.counter)?;
        fs::write(&path, content).await?;
        Ok(())
    }

    fn check_date_reset(&mut self) -> anyhow::Result<()> {
        let today = Utc::now().date_naive();
        if self.counter.date != today {
            self.counter.date = today;
            self.counter.used = 0;
            self.counter.deferred_tasks.clear();
        }
        Ok(())
    }

    pub fn try_acquire(&mut self, task_id: &str) -> bool {
        let _ = self.check_date_reset();

        if self.counter.used >= self.counter.budget {
            if !self.counter.deferred_tasks.contains(&task_id.to_string()) {
                self.counter.deferred_tasks.push(task_id.to_string());
            }
            return false;
        }

        self.counter.used += 1;
        self.counter.last_call_at = Utc::now().timestamp();
        true
    }

    pub fn get_remaining(&self) -> u32 {
        self.counter.budget.saturating_sub(self.counter.used)
    }

    pub fn get_used(&self) -> u32 {
        self.counter.used
    }

    pub fn get_budget(&self) -> u32 {
        self.counter.budget
    }

    pub fn is_exhausted(&self) -> bool {
        self.counter.used >= self.counter.budget
    }

    pub fn set_budget(&mut self, budget: u32) {
        self.counter.budget = budget;
    }

    pub fn get_deferred_tasks(&self) -> &Vec<String> {
        &self.counter.deferred_tasks
    }

    pub fn clear_deferred_task(&mut self, task_id: &str) {
        self.counter.deferred_tasks.retain(|t| t != task_id);
    }
}
