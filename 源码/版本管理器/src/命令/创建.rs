// 文件: 版本管理器/src/命令/创建.rs — 薄 CLI 壳：解析参数 → 调用 pm::创建项目 → 打印结果
// 命令: 智语言 创建 [--全部模版] <项目名> [路径]

use std::path::PathBuf;

pub fn 执行(参数: &[String]) {
    let 全部模版 = 参数.iter().any(|p| p == "--全部模版");
    let 位置: Vec<&String> = 参数.iter().filter(|a| a.as_str() != "--全部模版").collect();
    if 位置.is_empty() {
        eprintln!("[智语言] 错误：请指定项目名。用法: 智语言 创建 [--全部模版] <项目名> [/路径/]");
        return;
    }
    let 项目名 = 位置[0].as_str();
    let 目标 = if 位置.len() > 1 {
        // 路径参数视为「父目录」，始终与项目名拼接（无论是否带尾斜杠）
        PathBuf::from(位置[1].as_str()).join(项目名)
    } else { std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(项目名) };

    match pm::创建项目::生成项目(项目名, &目标, 全部模版) {
        Ok(()) => {
            let 绝对 = 目标.canonicalize().unwrap_or(目标.clone());
            eprintln!("[智语言] 项目已创建：{}/", 绝对.display());
            if 全部模版 {
                eprintln!("[智语言] 已生成完整目录模板：");
                eprintln!("[智语言]   配置/    - 项目配置文件");
                eprintln!("[智语言]   源码/    - 源代码文件");
                eprintln!("[智语言]   测试/    - 测试代码文件");
                eprintln!("[智语言]   库/      - 第三方依赖（安装后自动填充）");
                eprintln!("[智语言]   外部语言/ - 外部语言绑定（可选）");
                eprintln!("[智语言]   产出/    - 构建产物（自动生成）");
                eprintln!("[智语言]   文档/    - 项目说明文档");
                eprintln!("[智语言] 项目 项目.配置 中：");
                eprintln!("[智语言]   - 如果仅为本地应用，只需保留「名称」和「版本」");
                eprintln!("[智语言]   - 如果要发布为库，还需填写「描述」和「许可证」");
            }
            eprintln!("[智语言] 请运行 '智语言 运行' 测试项目是否正常启动");
        }
        Err(e) => eprintln!("[智语言] 错误：{}", e),
    }
}
