//! JSON 日志系统模块
//! 
//! 提供低侵入性的 JSON 格式日志功能，支持三级追踪（session_id/request_id/span_id）
//! 通过 feature flag `logging` 控制编译，生产环境可完全移除

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use chrono::Utc;

/// 全局日志器实例（通过 CaelixContext 设置）
static GLOBAL_LOGGER: Mutex<Option<Arc<JsonLogger>>> = Mutex::new(None);

/// 设置全局日志器
pub fn set_global_logger(logger: Arc<JsonLogger>) {
    let mut global = GLOBAL_LOGGER.lock().unwrap();
    *global = Some(logger);
}

/// 获取全局日志器
pub fn get_global_logger() -> Option<Arc<JsonLogger>> {
    let global = GLOBAL_LOGGER.lock().unwrap();
    global.clone()
}

/// 日志条目结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// 会话 ID（会话级追踪）
    pub session_id: String,
    /// 请求 ID（请求级追踪）
    pub request_id: String,
    /// Span ID（操作级追踪）
    pub span_id: String,
    /// 日志级别（DEBUG/INFO/WARN/ERROR）
    pub level: String,
    /// 代码位置（file:line）
    pub location: String,
    /// ISO 8601 时间戳
    pub timestamp: String,
    /// 日志消息内容（JSON 对象）
    pub message: serde_json::Value,
}

/// 日志器配置
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    /// 是否启用 debug 日志
    pub debug_enabled: bool,
    /// 日志目录路径
    pub log_dir: PathBuf,
    /// 单个日志文件最大大小（字节），默认 10MB
    pub max_file_size: u64,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            debug_enabled: false,
            log_dir: PathBuf::from("./logs"),
            max_file_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// JSON 日志器（线程安全单例）
pub struct JsonLogger {
    config: LoggerConfig,
    current_file: Arc<Mutex<Option<File>>>,
    current_file_size: Arc<Mutex<u64>>,
    current_file_path: Arc<Mutex<PathBuf>>,
}

impl JsonLogger {
    /// 创建新的日志器实例
    pub fn new(config: LoggerConfig) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        // 创建日志目录
        std::fs::create_dir_all(&config.log_dir)?;
        
        let logger = Arc::new(Self {
            config,
            current_file: Arc::new(Mutex::new(None)),
            current_file_size: Arc::new(Mutex::new(0)),
            current_file_path: Arc::new(Mutex::new(PathBuf::new())),
        });
        
        // 初始化第一个日志文件
        logger.rotate_file()?;
        
        Ok(logger)
    }
    
    /// 记录日志
    pub fn log(
        &self,
        level: &str,
        session_id: &str,
        request_id: &str,
        span_id: &str,
        location: &str,
        message: serde_json::Value,
    ) {
        // 只有 DEBUG 级别需要检查开关
        if level == "DEBUG" && !self.config.debug_enabled {
            return;
        }
        
        let entry = LogEntry {
            session_id: session_id.to_string(),
            request_id: request_id.to_string(),
            span_id: span_id.to_string(),
            level: level.to_string(),
            location: location.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            message,
        };
        
        // 序列化为 JSON 字符串
        let json_line = match serde_json::to_string(&entry) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Failed to serialize log entry: {}", e);
                return;
            }
        };
        
        // 异步写入文件（使用 spawn_blocking 避免阻塞 async 运行时）
        let file_mutex = self.current_file.clone();
        let size_mutex = self.current_file_size.clone();
        let path_mutex = self.current_file_path.clone();
        let config = self.config.clone();
        let line_with_newline = format!("{}\n", json_line);
        let line_size = line_with_newline.len() as u64;
        
        std::thread::spawn(move || {
            if let Err(e) = Self::write_to_file(
                &file_mutex,
                &size_mutex,
                &path_mutex,
                &config,
                line_with_newline,
                line_size,
            ) {
                eprintln!("Failed to write log: {}", e);
            }
        });
    }
    
    /// 写入文件并处理轮转
    fn write_to_file(
        file_mutex: &Arc<Mutex<Option<File>>>,
        size_mutex: &Arc<Mutex<u64>>,
        path_mutex: &Arc<Mutex<PathBuf>>,
        config: &LoggerConfig,
        content: String,
        content_size: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 检查是否需要轮转
        let need_rotate = {
            let size_guard = size_mutex.lock().unwrap();
            *size_guard + content_size > config.max_file_size
        };
        
        if need_rotate {
            // 重新获取锁进行轮转
            let mut file_guard = file_mutex.lock().unwrap();
            let mut size_guard = size_mutex.lock().unwrap();
            let mut path_guard = path_mutex.lock().unwrap();
            
            // 关闭当前文件
            *file_guard = None;
            *size_guard = 0;
            
            // 创建新文件
            Self::create_new_file(&mut file_guard, &mut path_guard, &config.log_dir)?;
        }
        
        // 写入日志
        let mut file_guard = file_mutex.lock().unwrap();
        let mut size_guard = size_mutex.lock().unwrap();
        
        if let Some(ref mut file) = *file_guard {
            file.write_all(content.as_bytes())?;
            file.flush()?;
            *size_guard += content_size;
        }
        
        Ok(())
    }
    
    /// 创建新的日志文件
    fn create_new_file(
        file: &mut Option<File>,
        path: &mut PathBuf,
        log_dir: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("caelix_{}.log", timestamp);
        *path = log_dir.join(&filename);
        
        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        
        *file = Some(new_file);
        Ok(())
    }
    
    /// 轮转日志文件
    fn rotate_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut file_guard = self.current_file.lock().unwrap();
        let mut size_guard = self.current_file_size.lock().unwrap();
        let mut path_guard = self.current_file_path.lock().unwrap();
        
        *file_guard = None;
        *size_guard = 0;
        
        Self::create_new_file(&mut file_guard, &mut path_guard, &self.config.log_dir)?;
        
        Ok(())
    }
    
    /// 获取是否启用 debug 日志
    #[allow(dead_code)] // 为将来使用预留
    pub fn is_debug_enabled(&self) -> bool {
        self.config.debug_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_log_entry_serialization() {
        let entry = LogEntry {
            session_id: "sess_123".to_string(),
            request_id: "req_456".to_string(),
            span_id: "span_789".to_string(),
            level: "DEBUG".to_string(),
            location: "test.rs:42".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            message: json!({"event": "test", "value": 123}),
        };
        
        let json_str = serde_json::to_string(&entry).unwrap();
        assert!(json_str.contains("sess_123"));
        assert!(json_str.contains("req_456"));
        assert!(json_str.contains("span_789"));
    }
}
