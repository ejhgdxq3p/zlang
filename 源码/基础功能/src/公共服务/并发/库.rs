// 文件: 并发运行时/src/库.rs
// 职责: 入口重导出 + 全局句柄表 + C ABI 出口。
//
// 设计要点：
//   - 解释模式：直接 use 本 crate 的 Rust 类型（通道/任务句柄/终止器/多路监听），
//     不经过 C ABI，零额外开销。
//   - 编译模式：Cranelift 生成的机器码通过 C ABI 入口调用本 crate。
//     C ABI 仅作为"门把手"，进入后仍是 100% Rust 并发能力（crossbeam）。
//   - 句柄（通道/任务/终止器/监听选项/值）以 u64 id 在 C ABI 边界传递，
//     实际对象存于进程级全局句柄表，由解释器/编译器侧持有 id 引用。
//   - C ABI 函数直接使用中文函数名 + `#[export_name = "智语言_xxx"]` 双重中文导出。

#![allow(dead_code)]
#![allow(non_snake_case)]

#[path = "值类型.rs"]
pub mod 值类型_;
#[path = "通道.rs"]
pub mod 通道_;
#[path = "任务.rs"]
pub mod 任务_;
#[path = "终止器.rs"]
pub mod 终止器_;
#[path = "多路监听.rs"]
pub mod 多路监听_;

pub use 值类型_::{键值, 拥有值};
pub use 通道_::通道;
pub use 任务_::任务句柄;
pub use 终止器_::终止器;
pub use 多路监听_::{监听选项, 多路监听};

// ============================================================
// 全局句柄表（C ABI 边界的对象生命周期管理）
// ============================================================

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use lazy_static::lazy_static;

lazy_static! {
    static ref 句柄表: Mutex<HashMap<u64, 句柄容器>> = Mutex::new(HashMap::new());
}

/// 句柄 id 计数器（原子自增，保证唯一）
static 句柄计数器: AtomicU64 = AtomicU64::new(1);

/// 句柄容器：C ABI 边界传递的对象类型
enum 句柄容器 {
    任务(任务句柄),
    通道(std::sync::Arc<通道>),
    终止器(终止器),
    监听选项(监听选项),
    拥有(拥有值),
}

/// 分配句柄 id（线程安全递增计数器）
fn 分配句柄(h: 句柄容器) -> u64 {
    let id = 句柄计数器.fetch_add(1, Ordering::SeqCst);
    let mut 表 = 句柄表.lock().unwrap();
    表.insert(id, h);
    id
}

pub fn 取任务(id: u64) -> Option<任务句柄> {
    let 表 = 句柄表.lock().unwrap();
    if let 句柄容器::任务(t) = 表.get(&id)? {
        Some(t.clone())
    } else {
        None
    }
}

/// 创建通道并直接返回句柄 id（Rust API，解释器直接调用）
pub fn 创建通道句柄(容量: usize) -> u64 {
    let 通道 = 通道::新建(容量);
    分配句柄(句柄容器::通道(通道))
}

/// 注册任务句柄并返回句柄 id（Rust API，解释器直接调用）
pub fn 创建任务句柄(句柄: 任务句柄) -> u64 {
    分配句柄(句柄容器::任务(句柄))
}

/// 从全局句柄表释放句柄（Rust API）
pub fn 释放句柄(句柄id: u64) {
    let mut 表 = 句柄表.lock().unwrap();
    表.remove(&句柄id);
}

pub fn 取通道(id: u64) -> Option<std::sync::Arc<通道>> {
    let 表 = 句柄表.lock().unwrap();
    if let 句柄容器::通道(c) = 表.get(&id)? {
        Some(c.clone())
    } else {
        None
    }
}

pub fn 取终止器(id: u64) -> Option<终止器> {
    let 表 = 句柄表.lock().unwrap();
    if let 句柄容器::终止器(t) = 表.get(&id)? {
        Some(t.clone())
    } else {
        None
    }
}

fn 取监听选项(id: u64) -> Option<监听选项> {
    let 表 = 句柄表.lock().unwrap();
    if let 句柄容器::监听选项(o) = 表.get(&id)? {
        Some(o.clone())
    } else {
        None
    }
}

fn 取拥有值(id: u64) -> Option<拥有值> {
    let 表 = 句柄表.lock().unwrap();
    if let 句柄容器::拥有(v) = 表.get(&id)? {
        Some(v.clone())
    } else {
        None
    }
}

// ============================================================
// C ABI 辅助：值的指针读写
// ============================================================

/// 从裸指针读取 `拥有值`（调用方拥有该值，本函数只读）
unsafe fn 读值<'a>(p: *const 拥有值) -> &'a 拥有值 {
    &*p
}

/// 向裸指针写入 `拥有值`（调用方已分配未初始化槽位）
unsafe fn 写值(p: *mut 拥有值, v: 拥有值) {
    std::ptr::write(p, v);
}

/// 从裸指针读取 UTF-8 文本
unsafe fn 读文本(p: *const u8, 长度: i64) -> String {
    if p.is_null() || 长度 <= 0 {
        return String::new();
    }
    let 切片 = std::slice::from_raw_parts(p, 长度 as usize);
    String::from_utf8_lossy(切片).into_owned()
}

// ============================================================
// ⚠️⚡双模式⚡: C ABI 出口 — 以下函数供编译模式调用（解释模式走 Rust 直接调用）
// 📋 解释对应: 虚拟机.rs CALL系统函数/opcode dispatch
// 📋 新增 C ABI: 同步代码生成.rs → 操作码::调用 分支
// 📋 规范: .codebuddy/rules/双模式开发规范.md（含完整缺口表）
// ============================================================
// C ABI 出口（ASCII 函数名 + 中文 export_name）
// ============================================================

/// 启动任务（编译模式）：闭包函数指针 + 捕获数据指针(u64) -> 返回句柄 id
#[export_name = "智语言_启动任务"]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn 智语言启动任务(
    闭包函数: extern "C" fn(*const 拥有值) -> 拥有值,
    捕获数据: u64,
    _捕获大小: u64,
) -> u64 {
    let 句柄 = 任务句柄::启动(move || {
        let 指针 = 捕获数据 as *const 拥有值;
        闭包函数(指针)
    });
    分配句柄(句柄容器::任务(句柄))
}

/// 创建通道 -> 返回句柄 id
#[export_name = "智语言_创建通道"]
pub extern "C" fn 智语言创建通道(容量: u64) -> u64 {
    let 通道 = 通道::新建(容量 as usize);
    分配句柄(句柄容器::通道(通道))
}

/// 通道发送：返回 0=成功，1=失败（已关闭）
#[export_name = "智语言_通道发送"]
pub extern "C" fn 智语言通道发送(通道指针: u64, 值指针: *const 拥有值) -> u64 {
    let 通道 = match 取通道(通道指针) {
        Some(c) => c,
        None => return 1,
    };
    let 数据 = unsafe { 读值(值指针).clone() };
    match 通道.发送(数据) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// 通道接收：返回 1=有数据（写入输出），0=空值/已关闭
#[export_name = "智语言_通道接收"]
pub extern "C" fn 智语言通道接收(通道指针: u64, 输出: *mut 拥有值) -> u64 {
    let 通道 = match 取通道(通道指针) {
        Some(c) => c,
        None => return 0,
    };
    match 通道.接收() {
        Some(值) => {
            unsafe { 写值(输出, 值) };
            1
        }
        None => 0,
    }
}

/// 关闭通道
#[export_name = "智语言_通道关闭"]
pub extern "C" fn 智语言通道关闭(通道指针: u64) {
    if let Some(c) = 取通道(通道指针) {
        c.关闭();
    }
}

/// 通道是否关闭 -> 0=否，1=是
#[export_name = "智语言_通道是否关闭"]
pub extern "C" fn 智语言通道是否关闭(通道指针: u64) -> u64 {
    match 取通道(通道指针) {
        Some(c) => c.是否关闭() as u64,
        None => 1,
    }
}

/// 通道尝试发送：返回 0=成功，1=失败（满或已关闭）
#[export_name = "智语言_通道尝试发送"]
pub extern "C" fn 智语言通道尝试发送(通道指针: u64, 值指针: *const 拥有值) -> u64 {
    let 通道 = match 取通道(通道指针) {
        Some(c) => c,
        None => return 1,
    };
    let 数据 = unsafe { 读值(值指针).clone() };
    match 通道.尝试发送(数据) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// 通道尝试接收：返回 1=有数据（写入输出），0=无数据
#[export_name = "智语言_通道尝试接收"]
pub extern "C" fn 智语言通道尝试接收(通道指针: u64, 输出: *mut 拥有值) -> u64 {
    let 通道 = match 取通道(通道指针) {
        Some(c) => c,
        None => return 0,
    };
    match 通道.尝试接收() {
        Some(值) => {
            unsafe { 写值(输出, 值) };
            1
        }
        None => 0,
    }
}

/// 任务是否完成 -> 0=否，1=是
#[export_name = "智语言_任务是否完成"]
pub extern "C" fn 智语言任务是否完成(任务指针: u64) -> u64 {
    match 取任务(任务指针) {
        Some(h) => h.是否完成() as u64,
        None => 1,
    }
}

/// 等待任务结果，写入输出
#[export_name = "智语言_等待结果"]
pub extern "C" fn 智语言任务等待(任务指针: u64, 输出: *mut 拥有值) {
    if let Some(h) = 取任务(任务指针) {
        let 结果 = h.等待结果();
        unsafe { 写值(输出, 结果) };
        // 🆕 2026-09-22 内存（D1）：与解释模式 `执行任务等待寄存器`（VM 侧 `释放句柄`）对齐 ——
        //   取值后释放任务句柄，避免全局句柄表 + 任务结果缓存（`拥有值`）长期驻留 ⇒ 逐轮泄漏。
        //   ⚠ 两引擎红线（方案 §5.1）：编译模式必须与解释模式同判定，否则失去双模硬判据资格。
        // ⚠ 2026-09-22 回退：不释放句柄（规范 §5.7.2：等待后句柄仍有效 ⇒ 必需驻留）。
    } else {
        unsafe { 写值(输出, 拥有值::空值) };
    }
}

/// 批量等待：按序写入结果数组
#[export_name = "智语言_批量等待"]
pub extern "C" fn 智语言任务批量等待(任务数组: *const u64, 数量: u64, 输出: *mut 拥有值) {
    let 列表 = unsafe { std::slice::from_raw_parts(任务数组, 数量 as usize) };
    let 任务集合: Vec<任务句柄> = 列表.iter().filter_map(|id| 取任务(*id)).collect();
    let 结果 = 任务句柄::批量等待(&任务集合);
    for (i, v) in 结果.iter().enumerate() {
        unsafe { std::ptr::write(输出.add(i), v.clone()) };
    }
    // 🆕 2026-09-22 内存（D1）：与解释模式 `执行批量等待寄存器` 对齐 —— 等待完成后释放任务句柄。
    //   结果已逐一 clone 写入输出缓冲区 ⇒ 移除句柄表条目不伤本次取值（`释放句柄` 对不存在 id 幂等）。
    // ⚠ 2026-09-22 回退：不释放句柄（同上，规范要求句柄等待后仍有效）。
}

/// 批量等待（带超时）
#[export_name = "智语言_批量等待超时"]
pub extern "C" fn 智语言任务批量等待超时(
    任务数组: *const u64,
    数量: u64,
    超时毫秒: u64,
    输出: *mut 拥有值,
) {
    let 列表 = unsafe { std::slice::from_raw_parts(任务数组, 数量 as usize) };
    let 任务集合: Vec<任务句柄> = 列表.iter().filter_map(|id| 取任务(*id)).collect();
    let 结果 = 任务句柄::批量等待超时(&任务集合, 超时毫秒);
    for (i, v) in 结果.iter().enumerate() {
        unsafe { std::ptr::write(输出.add(i), v.clone()) };
    }
    // 🆕 2026-09-22 内存（D1）：与解释模式 `执行批量等待超时寄存器` 对齐 ⇒ 等待完成后释放任务句柄。
    //   句柄在本次调用内已完成取值（结果写入输出缓冲）；再次等待应使用语言层自有句柄资源。
    // ⚠ 2026-09-22 回退：不释放句柄（同上，规范要求句柄等待后仍有效）。
}

/// 任务让出
#[export_name = "智语言_任务让出"]
pub extern "C" fn 智语言任务让出() {
    任务_::任务让出();
}

/// 创建终止器：毫秒=0 手动，>0 超时自动触发 -> 返回句柄 id
#[export_name = "智语言_创建终止器"]
pub extern "C" fn 智语言终止器创建(毫秒: u64) -> u64 {
    let t = if 毫秒 == 0 {
        终止器::新建()
    } else {
        终止器::新建_超时(毫秒)
    };
    分配句柄(句柄容器::终止器(t))
}

/// 触发终止
#[export_name = "智语言_终止器触发"]
pub extern "C" fn 智语言终止器触发(终止器指针: u64) {
    if let Some(t) = 取终止器(终止器指针) {
        t.触发终止();
    }
}

/// 是否终止 -> 0=否，1=是
#[export_name = "智语言_终止器查询"]
pub extern "C" fn 智语言终止器查询(终止器指针: u64) -> u64 {
    match 取终止器(终止器指针) {
        Some(t) => t.是否终止() as u64,
        None => 1,
    }
}

/// 注册通道监听 -> 返回监听选项 id
#[export_name = "智语言_监听通道"]
pub extern "C" fn 智语言监听通道(名称: *const u8, 名称长度: i64, 通道指针: u64) -> u64 {
    let 名称文本 = unsafe { 读文本(名称, 名称长度) };
    let 通道 = match 取通道(通道指针) {
        Some(c) => c,
        None => return 0,
    };
    let 选项 = 监听选项::通道可读(名称文本, 通道.数据接收端().clone(), 通道.关闭信号接收端().clone());
    分配句柄(句柄容器::监听选项(选项))
}

/// 注册超时监听 -> 返回监听选项 id
#[export_name = "智语言_监听超时"]
pub extern "C" fn 智语言监听超时(名称: *const u8, 名称长度: i64, 毫秒: u64) -> u64 {
    let 名称文本 = unsafe { 读文本(名称, 名称长度) };
    let 选项 = 监听选项::超时(名称文本, 毫秒);
    分配句柄(句柄容器::监听选项(选项))
}

/// 注册终止监听 -> 返回监听选项 id
#[export_name = "智语言_监听终止"]
pub extern "C" fn 智语言监听终止(名称: *const u8, 名称长度: i64, 终止器指针: u64) -> u64 {
    let 名称文本 = unsafe { 读文本(名称, 名称长度) };
    let 终止器对象 = match 取终止器(终止器指针) {
        Some(t) => t,
        None => return 0,
    };
    let 选项 = 监听选项::终止(名称文本, 终止器对象.信号接收端().clone());
    分配句柄(句柄容器::监听选项(选项))
}

/// 多路监听：返回结果映射的句柄 id（映射仅含 "来源" 键）
#[export_name = "智语言_多路监听"]
pub extern "C" fn 智语言多路监听(选项数组: *const u64, 数量: u64) -> u64 {
    let 列表 = unsafe { std::slice::from_raw_parts(选项数组, 数量 as usize) };
    let 选项: Vec<监听选项> = 列表.iter().filter_map(|id| 取监听选项(*id)).collect();
    let 结果 = 多路监听(&选项);
    分配句柄(句柄容器::拥有(结果))
}

/// 读取已存值（例如多路监听返回的映射）到输出
#[export_name = "智语言_读取值"]
pub extern "C" fn 智语言读取拥有值(值指针: u64, 输出: *mut 拥有值) {
    if let Some(v) = 取拥有值(值指针) {
        unsafe { 写值(输出, v) };
    }
}

/// 释放句柄（从全局表移除，对象随后被回收）
#[export_name = "智语言_释放句柄"]
pub extern "C" fn 智语言释放句柄(句柄指针: u64) {
    let mut 表 = 句柄表.lock().unwrap();
    表.remove(&句柄指针);
}
