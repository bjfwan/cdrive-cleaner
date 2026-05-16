//! 轻量统计工具：给"重复跑 N 次"的基准测试用。
//!
//! 不引入 criterion / cargo bench，避免加 nightly 依赖。
//! 使用方式见 `src-tauri/tests/bench_runner.rs`。

use serde::Serialize;
use std::time::{Duration, Instant};

/// 一次实验的原始样本（毫秒为单位）。
#[derive(Debug, Clone, Serialize)]
pub struct Sample {
    pub run_index: usize,
    pub duration_ms: f64,
    /// 实验自定义的载荷指标（如扫描到的文件数、迁移的字节数等）。
    pub payload: serde_json::Value,
}

/// 一组实验的统计摘要。
#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub n: usize,
    pub mean_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub stddev_ms: f64,
    /// 变异系数 stddev / mean，越小越稳定（< 0.1 算很稳）。
    pub cv: f64,
}

impl Stats {
    pub fn from_samples(samples: &[Sample]) -> Self {
        let mut durs: Vec<f64> = samples.iter().map(|s| s.duration_ms).collect();
        let n = durs.len();
        assert!(n > 0, "Stats::from_samples 需要至少一个样本");

        durs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = durs.iter().sum::<f64>() / n as f64;
        let median = if n % 2 == 0 {
            (durs[n / 2 - 1] + durs[n / 2]) / 2.0
        } else {
            durs[n / 2]
        };
        // p95 用最近邻插值，样本少时(< 20)就是最大值或次大值
        let p95_idx = ((n as f64 - 1.0) * 0.95).round() as usize;
        let p95 = durs[p95_idx];
        let min = durs[0];
        let max = durs[n - 1];
        let var = durs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        let stddev = var.sqrt();
        let cv = if mean > 0.0 { stddev / mean } else { 0.0 };

        Self {
            n,
            mean_ms: mean,
            median_ms: median,
            p95_ms: p95,
            min_ms: min,
            max_ms: max,
            stddev_ms: stddev,
            cv,
        }
    }
}

/// 一组重复实验的完整结果。
#[derive(Debug, Clone, Serialize)]
pub struct BenchSeries {
    /// 实验名，如 "scan_known_tree_1k"。
    pub name: String,
    /// 实验描述：跑了什么场景。
    pub description: String,
    pub stats: Stats,
    pub samples: Vec<Sample>,
}

impl BenchSeries {
    pub fn new(name: impl Into<String>, description: impl Into<String>, samples: Vec<Sample>) -> Self {
        let stats = Stats::from_samples(&samples);
        Self {
            name: name.into(),
            description: description.into(),
            stats,
            samples,
        }
    }
}

/// 重复跑闭包 N 次，每次记录耗时和载荷。
///
/// `f` 接收 run_index（0-based），返回实验产生的载荷（任意可序列化对象）。
pub fn repeat<F, T>(n: usize, mut f: F) -> Vec<Sample>
where
    F: FnMut(usize) -> T,
    T: Serialize,
{
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let start = Instant::now();
        let payload = f(i);
        let elapsed = start.elapsed();
        samples.push(Sample {
            run_index: i,
            duration_ms: duration_to_ms(elapsed),
            payload: serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
        });
    }
    samples
}

/// async 版本。
pub async fn repeat_async<F, Fut, T>(n: usize, mut f: F) -> Vec<Sample>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = T>,
    T: Serialize,
{
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let start = Instant::now();
        let payload = f(i).await;
        let elapsed = start.elapsed();
        samples.push(Sample {
            run_index: i,
            duration_ms: duration_to_ms(elapsed),
            payload: serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null),
        });
    }
    samples
}

/// 把多个 series 渲染成 Markdown 报告。
pub fn render_markdown(title: &str, series: &[BenchSeries]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", title));
    out.push_str(&format!(
        "生成时间：{}\n\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    out.push_str("## 摘要\n\n");
    out.push_str("| 实验 | n | mean (ms) | median | p95 | min | max | stddev | CV |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for s in series {
        out.push_str(&format!(
            "| {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.3} |\n",
            s.name,
            s.stats.n,
            s.stats.mean_ms,
            s.stats.median_ms,
            s.stats.p95_ms,
            s.stats.min_ms,
            s.stats.max_ms,
            s.stats.stddev_ms,
            s.stats.cv,
        ));
    }
    out.push_str("\n> CV = stddev / mean，越小越稳定。CV < 0.1 通常视为可重复。\n\n");

    for s in series {
        out.push_str(&format!("## {}\n\n", s.name));
        out.push_str(&format!("{}\n\n", s.description));
        out.push_str("| run | ms | payload |\n|---|---|---|\n");
        for sm in &s.samples {
            let p = serde_json::to_string(&sm.payload).unwrap_or_default();
            // 简短显示，超长 payload 截断
            let p_display = if p.len() > 80 { format!("{}…", &p[..80]) } else { p };
            out.push_str(&format!(
                "| {} | {:.2} | `{}` |\n",
                sm.run_index, sm.duration_ms, p_display
            ));
        }
        out.push('\n');
    }

    out
}

fn duration_to_ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
