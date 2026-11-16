// 文件: 前端/src/诊断全局.rs
// 职责: 提供全局诊断工具，方便其他模块记录错误和警告

use crate::公共数据结构::*;
use crate::诊断收集器::诊断收集器;
use std::sync::Mutex;

static 全局诊断器: Mutex<Option<诊断收集器>> = Mutex::new(None);

pub fn 初始化诊断() {
    let mut 诊断器 = 全局诊断器.lock().unwrap();
    *诊断器 = Some(诊断收集器::新建());
}

pub fn 记录诊断(诊断: 诊断信息) {
    if let Ok(mut 诊断器) = 全局诊断器.lock() {
        if let Some(ref mut 收集器) = *诊断器 {
            收集器.报告(诊断);
        }
    }
}

pub fn 有错误() -> bool {
    if let Ok(诊断器) = 全局诊断器.lock() {
        if let Some(ref 收集器) = *诊断器 {
            return 收集器.有错误();
        }
    }
    false
}

/// 从进程环境命令行参数中读取 `--简洁` 和 `--颜色` 标志，
/// 并设置到诊断收集器。适用于各命令函数在创建收集器后调用。
pub fn 设置诊断模式_自CLI(诊断: &mut 诊断收集器) {
    let 参数: Vec<String> = std::env::args().collect();
    for 参数项 in &参数 {
        if 参数项 == "--简洁" {
            诊断.设置简洁模式(true);
        }
        if 参数项 == "--颜色" {
            诊断.设置颜色模式(true);
        }
        if 参数项 == "--结构化" {
            诊断.设置结构化模式(true);
        }
    }
}

pub fn 获取诊断输出() -> Vec<String> {
    if let Ok(诊断器) = 全局诊断器.lock() {
        if let Some(ref 收集器) = *诊断器 {
            return 收集器.输出所有诊断();
        }
    }
    Vec::new()
}