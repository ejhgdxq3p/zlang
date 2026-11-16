// 文件: 并发运行时/src/任务.rs
// 职责: 基于 std::thread + crossbeam 结果通道实现文档 5.7.2 的任务调度。
//   - 启动任务: 在独立 OS 线程执行闭包，结果通过 bounded(1) 通道回传。
//   - 等待结果: 阻塞接收结果，并缓存以支持多次调用。
//   - 批量等待: 按传入顺序返回各任务结果。
//   - 批量等待(超时): 用 recv_deadline，超时填空值；未完成任务线程继续运行，
//     稍后调用 等待结果() 仍可获取最终结果（符合文档语义）。
//   - 任务让出: 提示 OS 调度器让出当前线程时间片。
//   - 异常隔离: 用 catch_unwind 捕获任务内 panic，转为 拥有值::异常 回传。

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crossbeam::channel::Receiver;
use std::sync::Arc;

use super::值类型_::拥有值;

/// 任务句柄内部状态（被 Arc 包裹，可 Clone）
#[derive(Debug)]
pub struct 任务句柄内部 {
    // 结果通道：任务完成后发送结果
    结果接收端: Receiver<拥有值>,
    // 结果缓存：支持多次 等待结果()（首次 recv 后写入）
    结果缓存: Mutex<Option<拥有值>>,
    // 线程结束标志：任务体（含 catch_unwind 包装）真正执行完毕后置真。
    // 与 结果缓存 解耦——即使调用方从未 等待结果()，也能正确反映“任务是否跑完”。
    已结束: Arc<AtomicBool>,
}

/// 任务句柄（Clone 仅克隆 Arc，底层共享同一任务）
#[derive(Debug, Clone)]
pub struct 任务句柄(pub Arc<任务句柄内部>);

impl 任务句柄 {
    /// 启动任务：在独立 OS 线程执行闭包，结果通过通道回传。
    /// 闭包返回自包含的 `拥有值`，内部 catch_unwind 捕获异常。
    /// 注：使用 std::thread::spawn（而非 rayon 线程池），避免任务内再等待子任务时
    ///     阻塞 rayon 工作线程导致线程池饥饿死锁。任务生命周期由结果通道管理。
    pub fn 启动(闭包: impl FnOnce() -> 拥有值 + Send + 'static) -> 任务句柄 {
        let 已结束 = Arc::new(AtomicBool::new(false));
        let 已结束克隆 = 已结束.clone();
        let (tx, rx) = crossbeam::channel::bounded(1);
        std::thread::spawn(move || {
            let 结果 = catch_unwind(AssertUnwindSafe(闭包));
            let 最终值 = match 结果 {
                Ok(v) => v,
                Err(_) => 拥有值::异常(
                    "任务异常".to_string(),
                    "任务执行过程中发生未捕获异常".to_string(),
                ),
            };
            // 任务体已真正执行完毕（无论正常返回还是被 catch_unwind 捕获），
            // 在发送结果前置位“已结束”，使 是否完成() 不依赖是否曾 等待结果()。
            已结束克隆.store(true, Ordering::SeqCst);
            let _ = tx.send(最终值); // 发送结果（tx drop 后通道关闭）
        });
        任务句柄(Arc::new(任务句柄内部 {
            结果接收端: rx,
            结果缓存: Mutex::new(None),
            已结束,
        }))
    }

    /// 等待结果：阻塞直到任务完成，返回结果（支持多次调用，结果被缓存）。
    pub fn 等待结果(&self) -> 拥有值 {
        // 先检查缓存（已等待过则直接返回）
        {
            let 缓存 = self.0.结果缓存.lock().unwrap();
            if let Some(v) = 缓存.as_ref() {
                return v.clone();
            }
        }
        // 阻塞等待结果
        let 结果 = self.0.结果接收端.recv().unwrap_or(拥有值::空值);
        // 写入缓存（供后续调用）
        *self.0.结果缓存.lock().unwrap() = Some(结果.clone());
        结果
    }

    /// 批量等待：按传入顺序返回所有结果
    pub fn 批量等待(句柄列表: &[任务句柄]) -> Vec<拥有值> {
        句柄列表.iter().map(|h| h.等待结果()).collect()
    }

    /// 批量等待（带超时）：超时返回空值，未完成任务继续运行。
    pub fn 批量等待超时(句柄列表: &[任务句柄], 超时毫秒: u64) -> Vec<拥有值> {
        let 截止时间 = Instant::now() + Duration::from_millis(超时毫秒);
        let mut 结果 = Vec::with_capacity(句柄列表.len());
        for h in 句柄列表 {
            match h.0.结果接收端.recv_deadline(截止时间) {
                Ok(值) => {
                    // 写入缓存（稍后 等待结果() 可直接获取）
                    *h.0.结果缓存.lock().unwrap() = Some(值.clone());
                    结果.push(值);
                }
                Err(_) => {
                    // 超时：填入空值；任务线程未被 join，继续运行
                    结果.push(拥有值::空值);
                }
            }
        }
        结果
    }

    /// 非阻塞查询任务结果是否已就绪（已被接收并写入缓存）。
    /// 批量等待超时后，已完成的任务该值为 true，超时未完成的为 false，
    /// 调用方借此区分“完成”与“超时”，避免误释放仍在运行的任务句柄。
    pub fn 结果是否就绪(&self) -> bool {
        self.0.结果缓存.lock().unwrap().is_some()
    }

    /// 文档 5.7 语义：任务.是否完成() 非阻塞查询。
    /// 查“任务线程是否已真正结束”，与结果是否曾被取走无关——
    /// 即使调用方从未 等待结果()，任务跑完后仍返回真。
    pub fn 是否完成(&self) -> bool {
        self.0.已结束.load(Ordering::SeqCst)
    }
}

/// 任务让出：提示 OS 调度器让出当前线程时间片。
/// 用于长计算任务防止饿死其他任务。
pub fn 任务让出() {
    std::thread::yield_now();
}
