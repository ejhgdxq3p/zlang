// 文件: 并发运行时/src/终止器.rs
// 职责: 基于 Arc<AtomicBool> + 信号通道实现文档 5.7.5 的终止器。
//   - 终止器(): 手动触发的一次性取消信号。
//   - 终止器(毫秒): 指定毫秒后自动触发的超时终止器（独立计时线程）。
//   - 触发终止(): 幂等（多次调用无副作用）。
//   - 是否终止(): 同步查询（O(1) 原子读）。
//   - 信号通道(): 返回信号接收端，可接入多路监听；终止时该通道断开。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossbeam::channel::{self, Receiver, Sender};

/// 终止器内部状态（被 Arc 包裹，可 Clone）
#[derive(Debug)]
pub struct 终止器内部 {
    已触发: AtomicBool,
    // 信号发送端用 Option 包裹：触发时 take() 并 drop，使接收端断开
    信号发送端: Mutex<Option<Sender<()>>>,
    信号接收端: Receiver<()>,
}

/// 终止器（Clone 仅克隆 Arc，底层共享同一终止信号）
#[derive(Debug, Clone)]
pub struct 终止器(pub Arc<终止器内部>);

impl 终止器 {
    /// 创建手动终止器
    pub fn 新建() -> 终止器 {
        let (信号发送端, 信号接收端) = channel::bounded(0);
        终止器(Arc::new(终止器内部 {
            已触发: AtomicBool::new(false),
            信号发送端: Mutex::new(Some(信号发送端)),
            信号接收端,
        }))
    }

    /// 创建超时终止器：指定毫秒后自动触发（计时从创建时刻开始）
    pub fn 新建_超时(毫秒: u64) -> 终止器 {
        let t = 终止器::新建();
        let t_clone = t.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(毫秒));
            t_clone.触发终止();
        });
        t
    }

    /// 触发终止：幂等
    pub fn 触发终止(&self) {
        if !self.0.已触发.swap(true, Ordering::SeqCst) {
            // take() 取出并 drop 信号发送端 -> 所有监听此终止器的 select 立即触发
            let _ = self.0.信号发送端.lock().unwrap().take();
        }
    }

    /// 是否终止（不阻塞）
    pub fn 是否终止(&self) -> bool {
        self.0.已触发.load(Ordering::SeqCst)
    }

    /// 信号通道接收端（供多路监听注册"终止事件"）
    pub fn 信号接收端(&self) -> &Receiver<()> {
        &self.0.信号接收端
    }
}
