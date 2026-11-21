// 文件: 标准库功能/文件系统函数.rs
// 职责: 文件 标准库 native 后端（与 标准库/文件.智 门面对应）。
//       由 分发器.rs::系统调用() 的 8..=21 分支委托到 处理文件()。
//
// 错误模型（规范 §5.2，2026-09-02 对齐）:
//   - 副作用操作（写入/追加/删除/复制/创建目录/删除目录/重命名/切换目录）与 读取：
//     失败一律抛「文件错误」可捕获异常（含路径与 OS 错误，供 异常错误().描述），返回空值；
//     不再静默返回 false / 空串（曾与规范"目标目录不存在时抛出异常"背道而驰，且静默失败=数据丢失隐患）。
//   - 纯查询（是否存在/是目录/是文件）返回逻辑，不存在=假是答案而非错误，保留。

use crate::标准库索引;
use crate::值类型::*;
use super::基础辅助::取文本;

/// 抛出可捕获的文件错误异常（错误模型：副作用/读取失败必须显式报错，绝不静默）。
fn 抛文件异常(操作: &str, 路径: &str, e: &std::io::Error) -> 运行时值 {
    let 描述 = if e.kind() == std::io::ErrorKind::InvalidData {
        format!("文件{}失败：{}（内容不是合法的 UTF-8 文本；文本接口不能用来读写二进制文件或其它编码的文本）", 操作, 路径)
    } else {
        format!("文件{}失败：{}（{}）", 操作, 路径, e)
    };
    crate::异常运行时::智语言_抛出异常(
        "文件错误".as_ptr(), "文件错误".len() as i64,
        描述.as_ptr(), 描述.len() as i64,
    );
    运行时值::空值()
}

pub(crate) fn 处理文件(索引: usize, 参数: &[运行时值]) -> 运行时值 {
    match 索引 {
        标准库索引::文件_读取 => {
            let 路径 = 取文本(参数, 0);
            match std::fs::read_to_string(&路径) {
                Ok(s) => 分配文本(s),
                Err(e) => 抛文件异常("读取", &路径, &e),
            }
        }
        标准库索引::文件_写入 => {
            let 路径 = 取文本(参数, 0);
            let 内容 = 取文本(参数, 1);
            match std::fs::write(&路径, &内容) {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("写入", &路径, &e),
            }
        }
        标准库索引::文件_追加 => {
            let 路径 = 取文本(参数, 0);
            let 内容 = 取文本(参数, 1);
            use std::io::Write;
            let 结果 = std::fs::OpenOptions::new()
                .append(true).create(true).open(&路径)
                .and_then(|mut f| f.write_all(内容.as_bytes()));
            match 结果 {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("追加", &路径, &e),
            }
        }
        标准库索引::文件_删除 => {
            let 路径 = 取文本(参数, 0);
            match std::fs::remove_file(&路径) {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("删除", &路径, &e),
            }
        }
        标准库索引::文件_获取大小 => {
            let 路径 = 取文本(参数, 0);
            let 大小 = std::fs::metadata(&路径).map(|m| m.len() as i64).unwrap_or(-1);
            运行时值::整数(大小)
        }
        标准库索引::文件_是否存在 => {
            let 路径 = 取文本(参数, 0);
            运行时值::逻辑(std::fs::metadata(&路径).is_ok())
        }
        标准库索引::文件_是目录 => {
            let 路径 = 取文本(参数, 0);
            运行时值::逻辑(std::fs::metadata(&路径).map(|m| m.is_dir()).unwrap_or(false))
        }
        标准库索引::文件_是文件 => {
            let 路径 = 取文本(参数, 0);
            运行时值::逻辑(std::fs::metadata(&路径).map(|m| m.is_file()).unwrap_or(false))
        }
        标准库索引::文件_创建目录 => {
            let 路径 = 取文本(参数, 0);
            match std::fs::create_dir_all(&路径) {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("创建目录", &路径, &e),
            }
        }
        标准库索引::文件_删除目录 => {
            let 路径 = 取文本(参数, 0);
            match std::fs::remove_dir_all(&路径) {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("删除目录", &路径, &e),
            }
        }
        标准库索引::文件_重命名 => {
            let 旧路径 = 取文本(参数, 0);
            let 新路径 = 取文本(参数, 1);
            match std::fs::rename(&旧路径, &新路径) {
                Ok(_) => 运行时值::空值(),
                Err(e) => {
                    let 描述 = format!("文件重命名失败：{} → {}（{}）", 旧路径, 新路径, e);
                    crate::异常运行时::智语言_抛出异常(
                        "文件错误".as_ptr(), "文件错误".len() as i64,
                        描述.as_ptr(), 描述.len() as i64,
                    );
                    运行时值::空值()
                }
            }
        }
        标准库索引::文件_遍历目录 => {
            let 路径 = 取文本(参数, 0);
            let mut 文件列表 = Vec::new();
            if let Ok(条目) = std::fs::read_dir(&路径) {
                for 条目 in 条目.flatten() {
                    let name = 条目.file_name().to_string_lossy().to_string();
                    文件列表.push(分配文本(name));
                }
            }
            分配数组(文件列表)
        }
        标准库索引::文件_获取当前目录 => {
            let s = std::env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            分配文本(s)
        }
        标准库索引::文件_切换目录 => {
            let 路径 = 取文本(参数, 0);
            match std::env::set_current_dir(&路径) {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("切换目录", &路径, &e),
            }
        }
        标准库索引::文件_复制 => {
            let 源路径 = 取文本(参数, 0);
            let 目标路径 = 取文本(参数, 1);
            match std::fs::copy(&源路径, &目标路径) {
                Ok(_) => 运行时值::空值(),
                Err(e) => 抛文件异常("复制", &源路径, &e),
            }
        }
        _ => 运行时值::空值(),
    }
}
