// 文件: 项目管理/src/清理.rs
// 职责: 清理业务逻辑 — 要清理的路径列表、统计目录、格式化大小
//       不含任何 I/O 删除操作（fs::remove_dir_all）和用户交互输出（eprintln!）
//       CLI 层调用本模块获取路径列表和统计信息后自行执行删除和打印

use std::fs;
use std::path::{Path, PathBuf};

/// 返回应该清理的路径列表（不含 I/O 删除）
/// 产出根: 项目产出目录 或 全局缓存产出目录
/// 目标子目录: 空 = 清理全部，否则只清理指定子目录
pub fn 要清理的路径(产出根: &Path, 目标子目录: &[&str]) -> Vec<PathBuf> {
    if 目标子目录.is_empty() {
        vec![产出根.to_path_buf()]
    } else {
        目标子目录
            .iter()
            .map(|子目录| 产出根.join(子目录))
            .collect()
    }
}

/// 统计目录下的文件数和总大小
pub fn 统计目录(目录: &Path) -> (usize, u64) {
    let mut 文件数 = 0;
    let mut 总大小 = 0u64;
    递归统计(目录, &mut 文件数, &mut 总大小);
    (文件数, 总大小)
}

fn 递归统计(目录: &Path, 文件数: &mut usize, 总大小: &mut u64) {
    if let Ok(条目) = fs::read_dir(目录) {
        for 条目 in 条目.flatten() {
            let 路径 = 条目.path();
            if 路径.is_dir() {
                递归统计(&路径, 文件数, 总大小);
            } else if let Ok(元数据) = 路径.metadata() {
                *文件数 += 1;
                *总大小 += 元数据.len();
            }
        }
    }
}

/// 格式化字节数为可读字符串
pub fn 格式化大小(字节: u64) -> String {
    if 字节 < 1024 {
        format!("{} B", 字节)
    } else if 字节 < 1024 * 1024 {
        format!("{:.1} KB", 字节 as f64 / 1024.0)
    } else if 字节 < 1024 * 1024 * 1024 {
        format!("{:.1} MB", 字节 as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", 字节 as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
