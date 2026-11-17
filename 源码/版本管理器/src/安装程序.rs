// 智语言安装程序 — 自解压安装（规范 6.1.5 首次安装路径选择 / 6.1.6 安装程序）
//
// 交付形态：单个 exe，内部 = 安装程序本体 + 追加的「便携版 ZIP 载荷」+ 24 字节尾记录。
//   尾记录（位于文件最末 24 字节，小端）: [载荷偏移 u64][载荷长度 u64][魔数 "ZHILANGI"]
//   追加在 PE 映像末尾之外的字节不会被 Windows 加载器读取，故自解压是安全的常规做法。
//
// 行为（双击即安装）：
//   1) 从自身 exe 读出内嵌 ZIP 载荷（与 便携版/*.zip 逐字节相同）
//   2) 交互选择安装目录（规范 6.1.5 三选项：推荐 / 系统 / 自定义）
//   3) 解压到临时目录 → 剥掉包内顶层 `智语言/`（launcher::包内顶层目录）→ 校验结构 → 合并到安装根
//      （安装根扮演的就是那个 `智语言/`：<安装目录>/版本管理器/ + 版本<号>/{执行文件,标准库,精简编译器,许可证.文档}）
//   4) 写 全局.配置：安装路径 / 开发套件版本 / 默认路径字段
//   5) 打印 PATH 指引
//
// 非交互用法（CI / 脚本）：
//   智语言开发套件0.0.1.exe --目标 D:\智语言 --静默
#![allow(non_snake_case)]
// 双击启动时不要弹出黑色控制台窗口（Windows GUI 子系统）。
// 带参数运行（--目标/--静默）时仍可写 stdout —— 被父进程重定向时句柄照常继承，CI 拿得到输出。
#![cfg_attr(windows, windows_subsystem = "windows")]

// 中文模块名必须显式给出路径（rustc 不允许按非 ASCII 标识符自动找文件）
#[cfg(windows)]
#[path = "图形界面.rs"]
mod 图形界面;

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 尾记录魔数（8 字节，位于文件最末尾）
const 载荷魔数: &[u8; 8] = b"ZHILANGI";
/// 尾记录长度：[偏移 8][长度 8][魔数 8]
const 尾记录长: u64 = 24;

fn main() {
    let 参数: Vec<String> = std::env::args().skip(1).collect();
    let 无参数 = 参数.is_empty(); // 双击启动的特征：无任何参数
    let 静默 = 有开关(&参数, "--静默");
    let 指定目标 = 取选项值(&参数, "--目标");

    if 有开关(&参数, "--帮助") || 有开关(&参数, "-h") {
        // 只写 stdout，不弹消息框：--帮助 是命令行用法（CI 里也可能被调用），
        // 模态框会把自动化卡住；双击安装（无参数）走图形向导，不需要看帮助。
        println!("{}", 用法文本());
        return;
    }

    // 双击（无参数、非静默）→ 图形安装向导；带参数 → 控制台/静默模式（CI 用）
    #[cfg(windows)]
    if 无参数 && !静默 && 指定目标.is_none() {
        let 结果 = match 读取内嵌载荷() {
            Ok((载荷, 版本号)) => 图形界面::运行(
                std::sync::Arc::new(载荷),
                版本号,
                pm::目录::默认安装路径(),
            ),
            Err(错误) => Err(错误),
        };
        match 结果 {
            Ok(()) => {}
            Err(错误) => {
                图形界面::提示错误(&错误);
                std::process::exit(1);
            }
        }
        return;
    }

    match 执行安装(静默, 指定目标) {
        Ok(()) => {
            等待退出(无参数);
        }
        Err(错误) => {
            eprintln!();
            eprintln!("[智语言] 安装失败：{}", 错误);
            eprintln!("[智语言] 未做任何系统配置修改。可重试，或用 --帮助 查看用法。");
            等待退出(无参数);
            std::process::exit(1);
        }
    }
}

fn 执行安装(静默: bool, 指定目标: Option<PathBuf>) -> Result<(), String> {
    println!("[智语言] ============================================");
    println!("[智语言]  智语言开发套件 安装程序 v{}", env!("CARGO_PKG_VERSION"));
    println!("[智语言] ============================================");
    println!();

    let (载荷, 版本号) = 读取内嵌载荷()?;
    println!(
        "[智语言] 安装包内容：开发套件 {}（{:.1} MB）",
        版本号,
        载荷.len() as f64 / 1048576.0
    );

    let 安装根 = 选择安装目录(静默, 指定目标)?;
    println!();
    println!("[智语言] 安装目录：{}", 安装根.display());

    if 安装根.exists() && !安装根.is_dir() {
        return Err(format!("目标路径已存在且不是目录：{}", 安装根.display()));
    }
    if 已装有套件(&安装根) && !静默 {
        let 回答 = 询问(&format!(
            "[智语言] 该目录已包含智语言套件，继续将覆盖同名文件（{}/版本{}）。继续？[Y/n] ",
            安装根.display(),
            版本号
        ))?;
        if !(回答.is_empty() || 回答.eq_ignore_ascii_case("y") || 回答.eq_ignore_ascii_case("yes")
            || 回答 == "是")
        {
            return Err("用户取消安装".to_string());
        }
    }

    println!("[智语言] 正在解压…");
    执行安装到目录(载荷.as_slice(), &安装根, &版本号, &|_| {})?;
    println!("[智语言] 全局配置：{}", pm::目录::全局配置文件().display());
    打印结果(&安装根, &版本号);
    Ok(())
}

/// 真正落盘的动作：解压 → 合并 → 写全局配置。控制台与图形界面共用这一份。
/// `进度(百分比 0..=100)` 供界面驱动进度条（控制台传空闭包）。
///
/// ⚠ 这里**不允许打印**：图形模式由工作线程调用它，而 GUI 子系统下没有控制台，
///   `println!` 写失败会 panic 掉工作线程 ⇒ 界面永远停在"正在解压…"（实测踩过）。
pub(crate) fn 执行安装到目录(
    载荷: &[u8],
    安装根: &Path,
    版本号: &str,
    进度: &dyn Fn(u32),
) -> Result<(), String> {
    解压到(载荷, 安装根, 版本号, 进度)?;
    写入全局配置(安装根, 版本号)?;
    进度(100);
    Ok(())
}

// ============ 读取内嵌载荷 ============

/// 从自身 exe 尾部读取内嵌 ZIP 载荷，并按包内顶层 `版本<号>/` 目录推断版本号。
fn 读取内嵌载荷() -> Result<(Vec<u8>, String), String> {
    let 自身 = std::env::current_exe().map_err(|e| format!("无法定位安装程序自身：{}", e))?;
    let mut 文件 = File::open(&自身)
        .map_err(|e| format!("无法打开安装程序（{}）：{}", 自身.display(), e))?;
    let 总长 = 文件
        .metadata()
        .map_err(|e| format!("无法读取安装程序信息：{}", e))?
        .len();
    if 总长 < 尾记录长 {
        return Err("安装程序不完整（缺少内嵌安装包）".to_string());
    }
    let mut 尾 = [0u8; 24];
    文件
        .seek(SeekFrom::End(-(尾记录长 as i64)))
        .and_then(|_| 文件.read_exact(&mut 尾))
        .map_err(|e| format!("读取安装程序尾记录失败：{}", e))?;
    if &尾[16..24] != 载荷魔数 {
        return Err(
            "这不是有效的智语言安装程序（未找到内嵌安装包）。\n\
             提示：请勿只复制 exe 的一部分，也不要把它与其它文件拼接。"
                .to_string(),
        );
    }
    let 偏移 = u64::from_le_bytes(尾[0..8].try_into().unwrap());
    let 长度 = u64::from_le_bytes(尾[8..16].try_into().unwrap());
    if 偏移.saturating_add(长度) > 总长 {
        return Err(format!(
            "安装程序损坏：载荷区间越界（偏移 {} 长度 {}，文件 {} 字节）",
            偏移, 长度, 总长
        ));
    }
    let mut 数据 = vec![0u8; 长度 as usize];
    文件
        .seek(SeekFrom::Start(偏移))
        .and_then(|_| 文件.read_exact(&mut 数据))
        .map_err(|e| format!("读取内嵌安装包失败：{}", e))?;

    let 版本号 = launcher::开发套件安装器::包内版本(&数据).unwrap_or_else(|| {
        env!("CARGO_PKG_VERSION").to_string()
    });
    Ok((数据, 版本号))
}

// ============ 交互：选择安装目录（规范 6.1.5） ============

fn 选择安装目录(静默: bool, 指定目标: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = 指定目标 {
        return Ok(绝对化(&清理输入路径(&p.to_string_lossy())));
    }
    let 推荐 = pm::目录::默认安装路径();
    if 静默 {
        return Ok(推荐);
    }
    let 系统 = base::平台::系统安装路径();

    println!();
    println!("[智语言] 请选择安装目录：");
    println!("[智语言]   [1] 推荐（当前用户，无需管理员权限）：{}", 推荐.display());
    println!("[智语言]   [2] 系统（所有用户，需要管理员权限）：{}", 系统.display());
    println!("[智语言]   [3] 自定义路径（可把文件夹直接拖进本窗口）");
    loop {
        let 行 = 询问("[智语言] 请选择 [1/2/3]（回车 = 1）：")?;
        match 行.as_str() {
            "" | "1" => return Ok(推荐.clone()),
            "2" => return Ok(系统.clone()),
            "3" => {
                let 输入 = 询问("[智语言] 请输入安装路径：")?;
                if 输入.is_empty() {
                    println!("[智语言] 路径不能为空，请重新选择。");
                    continue;
                }
                return Ok(绝对化(&清理输入路径(&输入)));
            }
            _ => println!("[智语言] 请输入 1、2 或 3。"),
        }
    }
}

/// 拖拽进控制台的路径形如 `"D:\智语言"`（带引号、可能带行尾空格），也可能带 `\\?\` 前缀。
fn 清理输入路径(原文: &str) -> String {
    let 去空白 = 原文.trim();
    let 去引号 = 去空白.trim_matches('"').trim_matches('\'').trim();
    let 去前缀 = 去引号.strip_prefix(r"\\?\").unwrap_or(去引号);
    去前缀.to_string()
}

/// 相对路径一律绝对化（安装路径是跨进程持久化状态，存相对路径会随 cwd 漂移）
fn 绝对化(路径: &str) -> PathBuf {
    let p = PathBuf::from(路径);
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    }
}

fn 已装有套件(根: &Path) -> bool {
    根.join("版本管理器").is_dir()
}

// ============ 解压与落盘 ============

/// 先解压到同级临时目录，校验顶层结构后再合并进安装根：
/// 中途失败（磁盘满 / 权限不足）不会在安装根留下半截文件。
fn 解压到(
    载荷: &[u8],
    安装根: &Path,
    版本号: &str,
    进度: &dyn Fn(u32),
) -> Result<(), String> {
    let 父 = 安装根
        .parent()
        .ok_or_else(|| format!("非法安装路径：{}", 安装根.display()))?;
    fs::create_dir_all(父).map_err(|e| format!("创建目录失败 {}：{}", 父.display(), e))?;
    let 根名 = 安装根
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "智语言".to_string());
    let 临时 = 父.join(format!("{}.安装临时", 根名));
    let _ = fs::remove_dir_all(&临时);
    fs::create_dir_all(&临时).map_err(|e| format!("创建临时目录失败：{}", e))?;

    // 解压占整个安装过程的绝大多数时间，进度条按 0..90% 映射（剩下 10% 留给合并与写配置）
    let 结果 = launcher::开发套件安装器::解压字节_带进度(载荷, &临时, &mut |已处理, 总数| {
        if 总数 > 0 {
            进度((已处理 * 90 / 总数) as u32);
        }
    });
    if let Err(e) = 结果 {
        let _ = fs::remove_dir_all(&临时);
        return Err(format!("解压失败：{}", e));
    }
    进度(92);

    // 结构校验：载荷 ZIP 自带一层 `智语言/`（launcher::包内顶层目录），先剥掉再判内容。
    // 安装根扮演的就是那个 `智语言/` 目录本身（<安装目录>/版本管理器/…），
    // 故这里合并的是「包根」而不是整个解压临时目录。
    let 包根 = launcher::开发套件安装器::解包根(&临时);
    if !包根.join("版本管理器").is_dir() {
        let _ = fs::remove_dir_all(&临时);
        return Err("安装包结构异常：缺少 版本管理器/ 目录".to_string());
    }
    if !包根.join(format!("版本{}", 版本号)).is_dir() {
        let _ = fs::remove_dir_all(&临时);
        return Err(format!("安装包结构异常：缺少 版本{}/ 目录", 版本号));
    }

    if let Err(e) = 合并目录(&包根, 安装根) {
        let _ = fs::remove_dir_all(&临时);
        return Err(format!("写入安装目录失败：{}", e));
    }
    let _ = fs::remove_dir_all(&临时);
    Ok(())
}

/// 把 临时目录 的内容合并到 目标目录（同名文件覆盖）。
/// 同卷优先 rename（快、原子），跨卷回退为复制。
fn 合并目录(源: &Path, 目标: &Path) -> std::io::Result<()> {
    fs::create_dir_all(目标)?;
    for 条目 in fs::read_dir(源)? {
        let 条目 = 条目?;
        let 源路径 = 条目.path();
        let 目标路径 = 目标.join(条目.file_name());
        if 源路径.is_dir() {
            if 目标路径.exists() && !目标路径.is_dir() {
                fs::remove_file(&目标路径)?;
            }
            合并目录(&源路径, &目标路径)?;
        } else {
            if 目标路径.exists() {
                fs::remove_file(&目标路径)?;
            }
            if fs::rename(&源路径, &目标路径).is_err() {
                fs::copy(&源路径, &目标路径)?;
                let _ = fs::remove_file(&源路径);
            }
        }
    }
    Ok(())
}

// ============ 全局配置（规范 6.1.10：安装程序自动填入实际路径） ============

fn 写入全局配置(安装根: &Path, 版本号: &str) -> Result<(), String> {
    let mut 配置 = pm::全局配置::读取配置();
    let 旧版本 = 配置.开发套件.开发套件版本.clone();
    if !旧版本.is_empty() && 旧版本 != 版本号 {
        配置.开发套件.回退记录 = 旧版本;
    }
    配置.开发套件.开发套件版本 = 版本号.to_string();
    配置.开发套件.安装路径 = 安装根.to_string_lossy().to_string();
    配置.填充默认路径();
    let 路径 = pm::目录::全局配置文件();
    配置
        .保存()
        .map_err(|e| format!("保存全局配置失败（{}）：{}", 路径.display(), e))?;
    Ok(())
}

// ============ 结果输出 ============

fn 打印结果(安装根: &Path, 版本号: &str) {
    let 管理器 = 安装根
        .join("版本管理器")
        .join(base::平台::可执行文件名("智语言"));
    let 工具包 = 安装根
        .join(format!("版本{}", 版本号))
        .join("执行文件")
        .join(base::平台::可执行文件名("智语言"));

    println!();
    println!("[智语言] ============================================");
    println!("[智语言]  安装完成");
    println!("[智语言] ============================================");
    println!("[智语言] 安装目录：{}", 安装根.display());
    println!("[智语言] ├─ 版本管理器：{}", 管理器.display());
    println!("[智语言] └─ 工具包 {}：{}", 版本号, 工具包.display());

    let 加入路径 = 管理器
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    println!();
    println!("[智语言] 下一步（让命令在任意位置可用）：");
    println!("[智语言]   1) 把下面这个目录加入环境变量 PATH（用户变量即可）：");
    println!("[智语言]      {}", 加入路径);
    println!("[智语言]   2) 重新打开一个终端，执行：智语言 版本");
    println!("[智语言]   3) 验证解释模式：智语言 运行 你的脚本.智");
    println!();
    println!("[智语言] 不配置 PATH 也能用，直接运行：");
    println!("[智语言]   \"{}\" 版本", 管理器.display());
}

fn 用法文本() -> String {
    format!(
        "智语言开发套件 安装程序 v{版本}\n\
         \n\
         用法：\n\
         \x20 双击运行                         图形安装向导（选择目录 → 安装 → 完成）\n\
         \x20 ...exe --目标 <目录> --静默      直接安装到指定目录，不询问（CI / 脚本）\n\
         \x20 ...exe --帮助                    显示本帮助\n\
         \n\
         安装结果与「便携版 ZIP 解压」完全一致（解压得到的 智语言/ 就是这里的 <安装目录>）：\n\
         \x20 <安装目录>/版本管理器/智语言(.exe)\n\
         \x20 <安装目录>/版本<号>/执行文件|标准库|精简编译器|许可证.文档\n",
        版本 = env!("CARGO_PKG_VERSION")
    )
}

// ============ 输入输出小工具 ============

fn 询问(提示: &str) -> Result<String, String> {
    print!("{}", 提示);
    let _ = std::io::stdout().flush();
    读一行()
}

fn 读一行() -> Result<String, String> {
    let mut 输入 = String::new();
    match std::io::stdin().read_line(&mut 输入) {
        Ok(_) => Ok(输入.trim_end_matches(['\r', '\n']).trim().to_string()),
        Err(e) => Err(format!("读取输入失败：{}", e)),
    }
}

/// 双启动时（无参数）暂停，避免控制台窗口一闪而过看不到结果
fn 等待退出(启用: bool) {
    if !启用 {
        return;
    }
    println!();
    print!("[智语言] 按回车键退出…");
    let _ = std::io::stdout().flush();
    let _ = 读一行();
}

fn 有开关(参数: &[String], 名: &str) -> bool {
    参数.iter().any(|p| p == 名)
}

fn 取选项值(参数: &[String], 名: &str) -> Option<PathBuf> {
    let 位置 = 参数.iter().position(|p| p == 名)?;
    let 值 = 参数.get(位置 + 1)?;
    if 值.trim().is_empty() {
        None
    } else {
        Some(PathBuf::from(值))
    }
}
