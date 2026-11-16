// 文件: 基础功能/src/运行时/回调管理器.rs
// 职责: C 回调 trampoline 系统 — 支持 C 库通过函数指针回调智语言函数
//
// 设计: 预分配签名分桶 + 槽位 trampoline：
//   - 每个 C 回调签名形状（如 void(int) 或 int(int, void*)）对应一个「签名分桶」
//   - 每个分桶有固定数量（256）的 trampoline 槽位
//   - 每个槽位存储一个「智语言函数索引 + 回调时的参数编组方式」
//   - 当 C 代码调用 trampoline 时，通过槽位索引找到智语言函数并调用
//
// 统一架构: 解释模式直接调 Rust API，编译模式通过 C ABI + 注册回调 调用

use std::collections::HashMap;
use std::sync::Mutex;

/// 回调签名形状标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum 回调签名 {
    /// void(int)
    V空值I64,
    /// int(void)
    I64V空值,
    /// int(int)
    I64I64,
    /// int(int, 指针)
    I64I64指针,
    /// void(指针)
    V空值指针,
    /// 动态形状（槽位索引 + 自定义编组）
    动态(u64),
}

/// 回调槽位
#[derive(Debug, Clone)]
struct 回调槽位 {
    函数索引: usize,
    已占用: bool,
}

/// 外部 trampoline 分配函数：返回 (槽位索引, trampoline 地址)
/// 由 shim cdylib 加载后注册，NULL=无 trampoline 支持
pub type 外部注册函数 = extern "C" fn(u64, u64) -> u64;

/// 回调管理器
pub struct 回调管理器 {
    槽位表: HashMap<回调签名, Vec<回调槽位>>,
    函数到形状: HashMap<usize, 回调签名>,
    外部注册器: Option<外部注册函数>,
}

impl 回调管理器 {
    pub fn 新建() -> Self {
        let mut 槽位表 = HashMap::new();
        let 预设形状 = vec![
            回调签名::V空值I64,
            回调签名::I64V空值,
            回调签名::I64I64,
            回调签名::I64I64指针,
            回调签名::V空值指针,
        ];
        for 形状 in 预设形状 {
            槽位表.insert(形状, (0..256).map(|_| 回调槽位 {
                函数索引: 0,
                已占用: false,
            }).collect());
        }
        回调管理器 {
            槽位表,
            函数到形状: HashMap::new(),
            外部注册器: None,
        }
    }

    /// 设置外部 trampoline 注册器（由 shim cdylib 提供）
    pub fn 设置外部注册器(&mut self, 注册器: 外部注册函数) {
        self.外部注册器 = Some(注册器);
    }

    /// 注册一个智语言函数为回调目标
    /// 返回: (槽位索引, trampoline 地址)
    /// - 优先走外部 shim trampoline（返回真实 C ABI 函数地址）
    /// - 回退自管槽位表（trampoline 地址为 0，仅解释模式有效）
    pub fn 注册回调(&mut self, 函数索引: usize, 签名: 回调签名) -> Option<(usize, u64)> {
        let 形状编号 = Self::签名到编号(&签名);
        // 优先走外部 shim trampoline
        if let Some(注册器) = self.外部注册器 {
            let 地址 = 注册器(形状编号 as u64, 函数索引 as u64);
            if 地址 != 0 {
                // trampoline 已由 shim 分配并返回地址
                // 槽位编号由 shim 内部决定，我们无法预知，暂不维护双份槽位表
                // 返回 (0, 地址) 表示外部托管的 trampoline
                return Some((0, 地址));
            }
        }
        // 回退自管槽位表
        let 槽位 = self.槽位表.get_mut(&签名)?;
        for (i, 槽) in 槽位.iter_mut().enumerate() {
            if !槽.已占用 {
                槽.已占用 = true;
                槽.函数索引 = 函数索引;
                self.函数到形状.insert(函数索引, 签名);
                // 无外部 trampoline 时地址为 0
                return Some((i, 0));
            }
        }
        None
    }

    pub fn 释放回调(&mut self, 函数索引: usize) {
        if let Some(形状) = self.函数到形状.remove(&函数索引) {
            if let Some(槽位) = self.槽位表.get_mut(&形状) {
                for 槽 in 槽位.iter_mut() {
                    if 槽.函数索引 == 函数索引 {
                        槽.已占用 = false;
                        槽.函数索引 = 0;
                        break;
                    }
                }
            }
        }
    }

    pub fn 查找函数索引(&self, 签名: &回调签名, 槽位索引: usize) -> Option<usize> {
        let 槽位 = self.槽位表.get(签名)?;
        槽位.get(槽位索引).filter(|s| s.已占用).map(|s| s.函数索引)
    }

    fn 签名到编号(签名: &回调签名) -> usize {
        match 签名 {
            回调签名::V空值I64 => 0,
            回调签名::I64V空值 => 1,
            回调签名::I64I64 => 2,
            回调签名::I64I64指针 => 3,
            回调签名::V空值指针 => 4,
            回调签名::动态(n) => *n as usize,
        }
    }
}

use once_cell::sync::Lazy;
static 全局回调管理器: Lazy<Mutex<回调管理器>> = Lazy::new(|| Mutex::new(回调管理器::新建()));

/// 设置外部 trampoline 注册器（由 shim 加载后调用）
pub fn 设置外部注册器(注册器: 外部注册函数) {
    全局回调管理器.lock().unwrap().设置外部注册器(注册器);
}

pub fn 注册回调(函数索引: usize, 签名: 回调签名) -> Option<(usize, u64)> {
    全局回调管理器.lock().unwrap().注册回调(函数索引, 签名)
}

pub fn 释放回调(函数索引: usize) {
    全局回调管理器.lock().unwrap().释放回调(函数索引)
}

#[export_name = "智语言_注册回调"]
pub extern "C" fn 智语言_注册回调_导出(函数索引: u64, 签名形状: u64) -> u64 {
    let 签名 = match 签名形状 {
        0 => 回调签名::V空值I64,
        1 => 回调签名::I64V空值,
        2 => 回调签名::I64I64,
        3 => 回调签名::I64I64指针,
        4 => 回调签名::V空值指针,
        _ => 回调签名::动态(签名形状),
    };
    match 注册回调(函数索引 as usize, 签名) {
        Some((_, 地址)) => 地址,
        None => u64::MAX,
    }
}

#[export_name = "智语言_释放回调"]
pub extern "C" fn 智语言_释放回调_导出(函数索引: u64) {
    释放回调(函数索引 as usize);
}

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 注册和查找() {
        注册回调(42, 回调签名::I64I64).unwrap();
        let 索引 = 全局回调管理器.lock().unwrap().查找函数索引(&回调签名::I64I64, 0);
        assert_eq!(索引, Some(42));
    }

    #[test]
    fn 释放和重复() {
        注册回调(99, 回调签名::V空值I64).unwrap();
        释放回调(99);
        let 索引 = 全局回调管理器.lock().unwrap().查找函数索引(&回调签名::V空值I64, 0);
        assert_eq!(索引, None);
        // 释放后槽位可复用
        注册回调(100, 回调签名::V空值I64).unwrap();
        let 索引 = 全局回调管理器.lock().unwrap().查找函数索引(&回调签名::V空值I64, 0);
        assert_eq!(索引, Some(100));
    }
}
