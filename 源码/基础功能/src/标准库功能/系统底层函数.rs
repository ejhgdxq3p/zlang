// 文件: 基础功能/src/标准库功能/系统底层函数.rs
// 职责: 系统 标准库 native 后端（与 标准库/系统.智 门面对应）。
//       由 分发器.rs::系统调用() 的 29..=34 分支委托到 处理系统()。

use crate::标准库索引;
use crate::值类型::*;
use super::基础辅助::{取文本, 取整数};

pub(crate) fn 处理系统(索引: usize, 参数: &[运行时值]) -> 运行时值 {
    match 索引 {
        标准库索引::系统_执行 => {
            let 命令 = 取文本(参数, 0);
            let (程序, 标志) = crate::平台::外壳();
            let s = std::process::Command::new(程序).arg(标志).arg(&命令)
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            分配文本(s)
        }
        标准库索引::系统_获取环境变量 => {
            let 名称 = 取文本(参数, 0);
            let s = std::env::var(&名称).unwrap_or_default();
            分配文本(s)
        }
        标准库索引::系统_设置环境变量 => {
            let 键 = 取文本(参数, 0);
            let 值 = 取文本(参数, 1);
            std::env::set_var(&键, &值);
            运行时值::空值()
        }
        标准库索引::系统_获取进程编号 => { 运行时值::整数(std::process::id() as i64) }
        标准库索引::系统_获取平台 => {
            let 平台 = crate::平台::名称();
            分配文本(平台.to_string())
        }
        标准库索引::系统_退出 => {
            let code = 取整数(参数, 0);
            std::process::exit(code as i32);
        }
        _ => 运行时值::空值(),
    }
}
