//! 统一日志系统模块
//!
//! 提供 `LogConfig` 配置结构与 `init_logging` 初始化函数，支持：
//! - 控制台 / 文件双输出
//! - 每行 JSON 结构化日志
//! - 按 target（模块）配置不同的日志级别
//! - 按文件大小滚动，保留最近 N 个日志文件
//! - 幂等初始化（重复调用安全忽略）

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Arc;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

static LOGGING_INIT: OnceLock<()> = OnceLock::new();

/// 日志配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    /// 全局级别：trace / debug / info / warn / error
    #[serde(default = "default_level")]
    pub level: String,

    /// 每包（target）覆盖的级别："caelix_agent=debug,caelix_llm=info"
    #[serde(default)]
    pub directives: Vec<String>,

    /// 日志文件目录
    #[serde(default = "default_dir")]
    pub dir: PathBuf,

    /// 单文件大小上限（MB），超出切分
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,

    /// 保留多少个历史日志文件
    #[serde(default = "default_max_files")]
    pub max_files: usize,

    /// 是否输出到 stdout
    #[serde(default = "default_stdout")]
    pub stdout: bool,

    /// 是否每行 JSON 格式
    #[serde(default = "default_json")]
    pub json: bool,
}

fn default_level() -> String {
    "info".to_string()
}

fn default_dir() -> PathBuf {
    PathBuf::from(".caelix").join("logs")
}

fn default_max_file_size_mb() -> u64 {
    50
}

fn default_max_files() -> usize {
    5
}

fn default_stdout() -> bool {
    true
}

fn default_json() -> bool {
    true
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            directives: Vec::new(),
            dir: default_dir(),
            max_file_size_mb: default_max_file_size_mb(),
            max_files: default_max_files(),
            stdout: default_stdout(),
            json: default_json(),
        }
    }
}

impl LogConfig {
    /// 使用 caelix_home 作为日志目录基准：`caelix_home/logs`
    pub fn with_caelix_home(mut self, caelix_home: &Path) -> Self {
        self.dir = caelix_home.join("logs");
        self
    }
}

/// 构造 `EnvFilter`：全局 level + directives 拼接
fn build_filter(config: &LogConfig) -> EnvFilter {
    let mut base = EnvFilter::new(&config.level);
    for directive in &config.directives {
        base = base.add_directive(directive.parse().unwrap_or_else(|_| {
            tracing::metadata::LevelFilter::INFO.into()
        }));
    }
    base
}

// -------- RollingFileInner --------

/// 文件 writer：按大小滚动 + 保留 N 个历史文件
///
/// 使用 `Arc<Mutex<Option<File>>>` 间接持有 File，以便在 rollover 时
/// 安全地关闭（释放锁后关闭 File）。
struct RollingFileInner {
    dir: PathBuf,
    current_path: PathBuf,
    file: Arc<Mutex<Option<File>>>,
    max_bytes: u64,
    max_files: usize,
    current_bytes: Arc<Mutex<u64>>,
}

impl RollingFileInner {
    fn new(dir: &Path, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;

        let current_path = dir.join("caelix.current.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current_path)?;
        let current_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            dir: dir.to_path_buf(),
            current_path,
            file: Arc::new(Mutex::new(Some(file))),
            max_bytes,
            max_files,
            current_bytes: Arc::new(Mutex::new(current_bytes)),
        })
    }

    fn rollover(&self) -> io::Result<()> {
        // 关闭当前文件：从 Option 中 take，drop 释放句柄
        {
            let mut guard = self.file.lock().unwrap();
            let _ = guard.take(); // drop 旧 File
        }

        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S%.3f");
        let rotated = self.dir.join(format!("caelix.{}.log", ts));
        if self.current_path.exists()
            && let Err(e) = fs::rename(&self.current_path, &rotated) {
                tracing::warn!(
                    from = %self.current_path.display(),
                    to = %rotated.display(),
                    error = %e,
                    "log file rotate failed"
                );
            }

        // 打开新文件
        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.current_path)?;
        {
            let mut guard = self.file.lock().unwrap();
            *guard = Some(new_file);
        }
        {
            let mut guard = self.current_bytes.lock().unwrap();
            *guard = 0;
        }

        self.purge_old()?;
        Ok(())
    }

    fn purge_old(&self) -> io::Result<()> {
        let mut entries: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
                s.starts_with("caelix.") && s.ends_with(".log") && s != "caelix.current.log"
            }) {
                entries.push(path);
            }
        }
        entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        for old in entries.into_iter().skip(self.max_files) {
            if let Err(e) = fs::remove_file(&old) {
                tracing::warn!(file = %old.display(), error = %e, "remove old log failed");
            }
        }
        Ok(())
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        // 先判断是否需要滚动
        {
            let mut bytes_guard = self.current_bytes.lock().unwrap();
            if *bytes_guard + buf.len() as u64 > self.max_bytes {
                if let Err(e) = self.rollover() {
                    tracing::warn!(error = %e, "log rotate failed");
                }
                *bytes_guard = 0;
            }
        }
        let mut guard = self.file.lock().unwrap();
        let file_opt = &mut *guard;
        if let Some(file) = file_opt.as_mut() {
            let n = file.write(buf)?;
            let mut bytes_guard = self.current_bytes.lock().unwrap();
            *bytes_guard += n as u64;
            Ok(n)
        } else {
            Err(io::Error::other("log file closed"))
        }
    }

    fn flush(&self) -> io::Result<()> {
        let mut guard = self.file.lock().unwrap();
        if let Some(file) = guard.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

struct SharedRollingWriter {
    inner: Arc<RollingFileInner>,
}

impl Write for SharedRollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

// -------- Tee writer（stdout + file 同时输出） --------

struct TeeWriter {
    file: Option<Arc<RollingFileInner>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut stdout = io::stdout();
        let _ = stdout.write_all(buf);
        if let Some(f) = &self.file {
            let _ = f.write(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stdout().flush();
        if let Some(f) = &self.file {
            let _ = f.flush();
        }
        Ok(())
    }
}

// -------- MakeWriter 实现 --------

enum LogSinkKind {
    Stdout,
    File(Arc<RollingFileInner>),
    Tee(Arc<RollingFileInner>),
}

struct LogMakeWriter {
    kind: LogSinkKind,
}

impl<'a> MakeWriter<'a> for LogMakeWriter {
    type Writer = Box<dyn Write + Send + 'a>;

    fn make_writer(&'a self) -> Self::Writer {
        match &self.kind {
            LogSinkKind::Stdout => Box::new(io::stdout()),
            LogSinkKind::File(inner) => Box::new(SharedRollingWriter { inner: inner.clone() }),
            LogSinkKind::Tee(inner) => Box::new(TeeWriter { file: Some(inner.clone()) }),
        }
    }
}

/// 初始化全局 tracing subscriber（幂等；重复调用直接返回 Ok）
pub fn init_logging(config: &LogConfig) -> Result<(), String> {
    if LOGGING_INIT.get().is_some() {
        return Ok(());
    }

    let filter = build_filter(config);
    let max_bytes = config.max_file_size_mb.saturating_mul(1024 * 1024);

    let has_file = max_bytes > 0;
    let has_stdout = config.stdout;

    let kind = match (has_file, has_stdout) {
        (true, true) => match RollingFileInner::new(&config.dir, max_bytes, config.max_files) {
            Ok(inner) => LogSinkKind::Tee(Arc::new(inner)),
            Err(e) => {
                eprintln!("[logging] cannot open log dir {:?}: {}", config.dir, e);
                LogSinkKind::Stdout
            }
        },
        (true, false) => match RollingFileInner::new(&config.dir, max_bytes, config.max_files) {
            Ok(inner) => LogSinkKind::File(Arc::new(inner)),
            Err(e) => {
                eprintln!("[logging] cannot open log dir {:?}: {}", config.dir, e);
                return Err(format!("open log dir failed: {}", e));
            }
        },
        (false, true) => LogSinkKind::Stdout,
        (false, false) => {
            tracing_subscriber::registry().with(filter).try_init().ok();
            LOGGING_INIT.set(()).ok();
            return Ok(());
        }
    };

    let make_writer = LogMakeWriter { kind };
    let registry = tracing_subscriber::registry().with(filter);

    let result = if config.json {
        registry
            .with(
                fmt::layer()
                    .json()
                    .with_timer(fmt::time::ChronoLocal::new(
                        "%Y-%m-%d %H:%M:%S%.3f".to_string(),
                    ))
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_writer(make_writer),
            )
            .try_init()
    } else {
        registry
            .with(
                fmt::layer()
                    .with_timer(fmt::time::ChronoLocal::new(
                        "%Y-%m-%d %H:%M:%S%.3f".to_string(),
                    ))
                    .with_writer(make_writer),
            )
            .try_init()
    };

    if let Err(e) = result {
        eprintln!("[logging] init subscriber warning: {}", e);
    }

    LOGGING_INIT.set(()).ok();
    Ok(())
}
