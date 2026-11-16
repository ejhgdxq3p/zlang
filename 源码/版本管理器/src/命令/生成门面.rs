// 文件: 版本管理器/src/命令/生成门面.rs
// 职责: 「生成门面」命令 —— 从 标准库元数据.rs 单一真相机械重生成 标准库/*.智 门面，并立即跑真源自洽校验。
//   彻底单源：门面不再手改，由本命令从元数据投影；--校验 仅运行校验（不写盘）。

use frontend::{标准库门面生成, 标准库真源校验};

pub fn 执行(参数: &[String]) {
    let 仅校验 = 参数.iter().any(|a| a == "--校验");
    let 输出目录 = 参数.iter().find(|a| !a.starts_with("--")).cloned();

    if !仅校验 {
        let 目录 = 输出目录
            .clone()
            .unwrap_or_else(标准库门面生成::默认输出目录);
        match 标准库门面生成::生成并写入(&目录) {
            Ok(()) => eprintln!("[智语言] 门面已生成至 {}", 目录),
            Err(e) => {
                eprintln!("[智语言] 门面生成失败:\n{}", e);
                std::process::exit(1);
            }
        }
    }

    match 标准库真源校验::校验(输出目录.as_deref()) {
        Ok(()) => {
            eprintln!("[智语言] 标准库真源自洽校验通过：元数据单一真相 ↔ 门面 一致，无漂移");
        }
        Err(e) => {
            eprintln!("[智语言] 标准库真源自洽校验失败:\n{}", e);
            std::process::exit(1);
        }
    }
}
