// 开发套件安装器.rs - 开发套件安装/卸载（规范 6.1.5/6.1.6）
//   内建 ZIP 解压（stored 方法，与 zip crate 兼容）
//   内建 SHA-256 校验（与 sha2 crate 逐比特一致）
//   注：ZIP 构造（构造存储zip）在 库管理 crate，本文件只负责解压/安装。
use std::fs;
use std::path::{Path, PathBuf};
use libmgr::校验;

/// 安装指定版本的开发套件
/// ZIP 结构（规范 6.1.1 安装后目录，解压即装；**顶层统一带一层 `智语言/`**）：
///   智语言/版本管理器/智语言(.exe on Windows, 无后缀 on POSIX)  ← 版本管理器（单例，首次安装写入，后续跳过）
///   智语言/版本<版本号>/        ← 该版本的工具包（执行文件/标准库/工具链/许可证.文档）
/// 顶层目录名单一真相 = `launcher::包内顶层目录`；解包后由 `解包根` 剥掉这层再判内容。
pub fn 安装(版本号: &str, 本地包: Option<&Path>, 安装目录: Option<&Path>) -> Result<(), String> {
    let 安装根 = match 安装目录 {
        // ★ 2026-11-03 修复：自定义安装路径必须**绝对化**（见 绝对化路径 注释）。
        Some(d) => 绝对化路径(d),
        None => pm::目录::安装路径(),
    };
    // latest/最新 需先向镜像源解析成真实版本号，否则会创建出名为「版本latest」的目录。
    // ⚠ 2026-10-03 修复：本地包安装**原先**直接把字面量 `latest` 当版本号（注释称"由包内元数据决定"
    //   但实际没有任何元数据解析）⇒ `智语言 安装 <文件路径>`（帮助文档明确列出的用法）会装出
    //   `版本latest/` 目录、并把激活版本写成 `latest`，随后 `版本` 打印 latest、任何转发都报
    //   「开发套件版本 latest 未安装」✗。现改为**从包内容推断**顶层 `版本<版本号>/` 目录名。
    let 版本号: String = match 本地包 {
        None => crate::开发套件镜像源::解析版本别名(版本号)?,
        Some(p) => {
            if 版本号.is_empty() || 版本号 == "latest" || 版本号 == "最新" {
                if !p.exists() { return Err(format!("本地文件不存在: {}", p.display())); }
                let 数据 = fs::read(p).map_err(|e| format!("无法打开ZIP: {}", e))?;
                包内版本(&数据).ok_or_else(|| {
                    format!(
                        "无法从本地包推断版本号（包内未找到 top-level 版本<版本号>/ 目录），请显式指定：智语言 安装 <版本号> <包路径>"
                    )
                })?
            } else {
                版本号.to_string()
            }
        }
    };
    let 版本号 = &版本号;
    let 工具包根 = 安装根.join(format!("版本{}", 版本号));
    if 工具包根.exists() && 工具包根.join("执行文件").exists() {
        return Err(format!("版本{} 已安装，不覆盖。", 版本号));
    }
    let (zip路径, sha文本) = match 本地包 {
        Some(p) => {
            if !p.exists() { return Err(format!("本地文件不存在: {}", p.display())); }
            let sf = p.with_extension("zip.sha256");
            // ⚠ 2026-10-03 修复：**必须取首 token**（与下载路径 `下载工具包` 一致）。
            //   标准 sha256sum/.sha256 文件形如 `<哈希>  <文件名>`，服务端产出的也是这个格式；
            //   原先把整行原样交给 `校验sha256文本`（它拿整串与计算值逐字比较）⇒ 必然失败，
            //   且报错里"期望"与"实际"看起来一模一样（期望含文件名尾巴），用户完全无从下手 ✗。
            //   实测：本地包安装 100% 报「SHA256校验失败」。
            let sha = if sf.exists() {
                fs::read_to_string(&sf)
                    .unwrap_or_default()
                    .trim_start_matches('\u{feff}')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
            } else {
                String::new()
            };
            (p.to_path_buf(), sha)
        }
        None => {
            eprintln!("[智语言] 正在下载智语言开发套件{}…", 版本号);
            let (包, sha) = crate::开发套件镜像源::下载工具包(版本号, &pm::目录::开发套件下载目录())?;
            (包, sha)
        }
    };
    if !sha文本.is_empty() {
        eprintln!("[智语言] 正在校验完整性…");
        校验::校验sha256文本(&zip路径, &sha文本)?;
    }

    eprintln!("[智语言] 正在解压…");
    let 数据 = fs::read(&zip路径).map_err(|e| format!("无法打开ZIP: {}", e))?;

    // 先解压到临时目录，再选择性复制，避免覆盖正在运行的版本管理器
    let 临时 = 安装根.join(format!("{}_临时", 版本号));
    if 临时.exists() { let _ = fs::remove_dir_all(&临时); }
    // ⚠ 2026-10-03 修复：**临时目录在失败路径上必须清理**。
    //   原实现各失败点用 `?` 直接返回 ⇒ 会留下 `版本X_临时/`（内含半截文件）。
    //   实测：deflate 包安装失败后残留 `版本3.0.0_临时`（校验失败不残留，因为校验在解压之前），
    //   且该残留会一直留着（扫描已安装版本时版本号格式校验会排除它 ⇒ 不会被误当成已装版本，
    //   但属纯垃圾，并把"安装失败不留残留"的预期打破）。
    if let Err(e) = 解压字节(&数据, &临时) {
        let _ = fs::remove_dir_all(&临时);
        return Err(e);
    }

    // ==== 版本管理器：版本比较决定 覆盖升级 / 跳过保留 / 首次安装 ====
    // 包内统一带一层 `智语言/`（launcher::包内顶层目录），解包后剥掉这层再谈内容
    let 包根 = 解包根(&临时);
    let 引导程序目标 = pm::目录::引导程序可执行文件();
    let 管理器源 = 包根.join("版本管理器");
    if 管理器源.is_dir() {
        let 需要安装 = if !引导程序目标.exists() {
            true // 首次安装
        } else {
            match 读版本文档(&管理器源.join("版本.文档")) {
                None => {
                    eprintln!("[智语言] 安装包无版本管理器元数据，已安装则跳过");
                    false
                }
                Some(新版本) => match 读版本文档(&pm::目录::引导程序目录().join("版本.文档")) {
                    Some(旧版本) if 新版本 > 旧版本 => {
                        eprintln!(
                            "[智语言] 版本管理器 v{} → v{} 升级",
                            crate::开发套件版本解析::格式版本(旧版本),
                            crate::开发套件版本解析::格式版本(新版本)
                        );
                        true
                    }
                    Some(旧版本) => {
                        eprintln!("[智语言] 版本管理器已是最新（v{}），跳过覆盖", crate::开发套件版本解析::格式版本(旧版本));
                        false
                    }
                    None => {
                        // 老安装无 版本.文档：迁移读旧 全局.配置 键补写安装痕迹
                        match 迁移读旧配置版本() {
                            Some(旧版本) => {
                                let _ = fs::write(
                                    pm::目录::引导程序目录().join("版本.文档"),
                                    format!("版本管理器版本 = {}\n", crate::开发套件版本解析::格式版本(旧版本)),
                                );
                                if 新版本 > 旧版本 {
                                    eprintln!(
                                        "[智语言] 版本管理器 v{} → v{} 升级",
                                        crate::开发套件版本解析::格式版本(旧版本),
                                        crate::开发套件版本解析::格式版本(新版本)
                                    );
                                    true
                                } else {
                                    eprintln!("[智语言] 版本管理器已是最新（v{}），跳过覆盖", crate::开发套件版本解析::格式版本(旧版本));
                                    false
                                }
                            }
                            None => {
                                eprintln!("[智语言] 已安装版本管理器版本未知，保守跳过覆盖");
                                false
                            }
                        }
                    }
                },
            }
        };
        if 需要安装 {
            // 失败同样要清理临时目录（否则残留 `版本X_临时`）
            if let Err(e) = 安装版本管理器(&管理器源, &引导程序目标) {
                let _ = fs::remove_dir_all(&临时);
                return Err(e);
            }
        }
    }

    // 工具包本体：复制 包根/版本<版本号> → 安装根/版本<版本号>
    // 失败同样清理临时目录（`create_dir_all` 与 `复制目录` 都是 io::Result，可 `and_then` 串联）
    let 本体源 = 包根.join(format!("版本{}", 版本号));
    let 复制结果 = if 本体源.is_dir() {
        fs::create_dir_all(&工具包根).and_then(|_| 复制目录(&本体源, &工具包根))
    } else {
        // 兼容旧 ZIP（执行文件/ 等平铺在根）：整个包根内容直接作为该版本
        fs::create_dir_all(&工具包根).and_then(|_| 复制目录(&包根, &工具包根))
    };
    if let Err(e) = 复制结果 {
        let _ = fs::remove_dir_all(&临时);
        return Err(format!("安装工具包失败: {}", e));
    }
    let _ = fs::remove_dir_all(&临时);

    let mut 配置 = pm::全局配置::读取配置();
    let 旧 = 配置.开发套件.开发套件版本.clone();
    if !旧.is_empty() && &旧 != 版本号 { 配置.开发套件.回退记录 = 旧; }
    配置.开发套件.开发套件版本 = 版本号.to_string();
    // 写回配置的一律是**绝对路径**（安装根 已在入口绝对化）——安装路径是持久化状态，
    // 存相对路径会导致「换一个当前目录执行就装到别处」，用户完全无从察觉（实测踩过）。
    if 安装目录.is_some() { 配置.开发套件.安装路径 = 安装根.to_string_lossy().to_string(); }
    配置.填充默认路径();
    配置.保存().map_err(|e| format!("保存全局配置失败: {}", e))?;
    eprintln!("[智语言] 安装至：{}", 工具包根.display());
    eprintln!("[智语言] 已切换至 {}", 版本号);
    Ok(())
}

/// 把用户传入的自定义安装路径规范化为**绝对路径**。
///
/// ★ 2026-11-03 修复：此前把用户参数**原样**写进 `全局.配置 [开发套件] 安装路径`。
///   实测 `智语言 安装 <zip> 日志\脚本\临时\安装实验` 之后，配置里存的是相对路径，
///   此后所有安装都按「进程当前目录」解析它 ⇒ 换个目录执行就装到别处（安装位置漂移），
///   而且日志里打印的也是相对路径，用户完全看不出问题。
///   安装路径是**跨进程持久化状态**，必须在入口绝对化，落盘/使用都用绝对路径。
fn 绝对化路径(p: &Path) -> std::path::PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(p)
    }
}

/// 解包后的「包根」：发布 ZIP 自带一层 `智语言/`（`launcher::包内顶层目录`），
/// 判包内结构前必须先把这层剥掉；同时兼容更早的、无顶层目录的 ZIP。
pub fn 解包根(解压目录: &Path) -> PathBuf {
    let 带层 = 解压目录.join(crate::包内顶层目录);
    if 带层.is_dir() {
        带层
    } else {
        解压目录.to_path_buf()
    }
}

/// 从本地安装包**内容**推断版本号：扫描 ZIP 中央目录，取顶层目录 `版本<版本号>/` 的名字。
/// 供 `智语言 安装 <文件路径>`（帮助文档列出的用法、未显式给版本号）使用——包内元数据即该顶层目录。
/// 任何解析失败返回 None（不 panic），由调用方给出可操作错误。
/// 安装程序（自解压）复用本函数：它拿到的是内嵌载荷字节，同样要从包内产物推断版本号。
pub fn 包内版本(数据: &[u8]) -> Option<String> {
    if 数据.len() < 22 {
        return None;
    }
    let 结束记录 = (0..=(数据.len() - 22))
        .rev()
        .find(|&偏移| &数据[偏移..偏移 + 4] == b"PK\x05\x06")?;
    let 条目数 = 读u16(数据, 结束记录 + 10) as usize;
    let mut 偏移 = 读u32(数据, 结束记录 + 16) as usize;
    for _ in 0..条目数 {
        if 偏移 + 46 > 数据.len() || &数据[偏移..偏移 + 4] != b"PK\x01\x02" {
            return None;
        }
        let 名长 = 读u16(数据, 偏移 + 28) as usize;
        let 额长 = 读u16(数据, 偏移 + 30) as usize;
        let 注长 = 读u16(数据, 偏移 + 32) as usize;
        if 偏移 + 46 + 名长 > 数据.len() {
            return None;
        }
        let 全名 = String::from_utf8_lossy(&数据[偏移 + 46..偏移 + 46 + 名长]).to_string();
        // 包内统一带一层 `智语言/`，剥掉这层后才看顶层是不是 `版本<号>/`
        let 名称 = 全名
            .strip_prefix(crate::包内顶层目录)
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(&全名);
        if let Some(后) = 名称.strip_prefix("版本") {
            let 版本段 = 后.split('/').next().unwrap_or("");
            if 是版本号形状(版本段) {
                return Some(版本段.to_string());
            }
        }
        偏移 = 偏移 + 46 + 名长 + 额长 + 注长;
    }
    None
}

/// 版本号形状校验（宽口径：以数字开头、只含 [A-Za-z0-9._+-]，兼容 `3.0.0-beta.1` 等预发布号）
fn 是版本号形状(文本: &str) -> bool {
    !文本.is_empty()
        && 文本.chars().next().map_or(false, |c| c.is_ascii_digit())
        && 文本
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

/// 读 版本.文档 的版本号（中文键值格式：版本管理器版本 = 1.0.0）
/// 任何解析失败返回 None（不 panic），调用方按"未知"保守处理
fn 读版本文档(路径: &Path) -> Option<(u32, u32, u32)> {
    let 内容 = fs::read_to_string(路径).ok()?;
    for 行 in 内容.lines() {
        let 行 = 行.trim().trim_start_matches('\u{feff}'); // 容错 UTF-8 BOM
        if let Some(后) = 行.strip_prefix("版本管理器版本") {
            // 顺序：先 trim 去掉两侧空白，再去除前导 '='，再 trim（防 "= 1.0.0" 残留 '='）
            let 值 = 后.trim().trim_start_matches('=').trim();
            if let Some(v) = crate::开发套件版本解析::解析版本(值) {
                return Some(v);
            }
        }
    }
    None
}

/// 迁移：老安装（无 版本.文档）回退读旧 全局.配置 键
/// （字段已从 全局配置 结构体删除，此处绕结构体直接解析配置文件找旧键）
fn 迁移读旧配置版本() -> Option<(u32, u32, u32)> {
    let 内容 = fs::read_to_string(pm::目录::全局配置文件()).ok()?;
    let 文档 = pm::配置读取器::解析配置(&内容);
    for (段名, 键名) in [
        ("开发套件", "版本管理器版本"),
    ] {
        if let Some(段) = pm::配置读取器::取段(&文档, 段名) {
            if let Some(值) = pm::配置读取器::取文本(段, 键名) {
                if let Some(v) = crate::开发套件版本解析::解析版本(&值) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// 安装/覆盖版本管理器
/// Unix：直接复制（POSIX 允许覆盖正在执行的 exe）
/// Windows：写 .新 + 辅助进程延迟替换（运行中 exe 被映像锁，主进程退出释放锁后生效）
fn 安装版本管理器(源: &Path, 目标exe: &Path) -> Result<(), String> {
    let 管理器目录 = 目标exe.parent().ok_or("版本管理器目录缺失")?;
    fs::create_dir_all(&管理器目录).map_err(|e| format!("创建版本管理器目录失败: {}", e))?;
    // 版本.文档 无文件锁，直接复制
    if 源.join("版本.文档").exists() {
        fs::copy(源.join("版本.文档"), 管理器目录.join("版本.文档")).ok();
    }
        let 管理器名 = base::平台::可执行文件名("智语言");
    #[cfg(target_os = "windows")]
    {
        if 目标exe.exists() {
            // 升级：正在运行的 exe 被映像锁，写 .新 + 辅助进程延迟替换（主进程退出释放锁后生效，最多 2 次尝试）
            let 新文件 = 管理器目录.join(format!("{}.新", 管理器名));
            fs::copy(源.join(&管理器名), &新文件).map_err(|e| format!("写入版本管理器新文件失败: {}", e))?;
            let 脚本 = format!(
                "timeout /t 2 /nobreak >nul 2>&1 & move /y {n}.新 {n} >nul 2>&1 & if exist {n}.新 (timeout /t 2 /nobreak >nul 2>&1 & move /y {n}.新 {n} >nul 2>&1)",
                n = 管理器名
            );
            let _ = std::process::Command::new("cmd").args(["/c", &脚本]).current_dir(管理器目录).spawn();
            eprintln!("[智语言] 版本管理器升级中，重启版本管理器后生效");
        } else {
            // 首次安装：目标不存在，直接复制
            fs::copy(源.join(&管理器名), 目标exe).map_err(|e| format!("安装版本管理器失败: {}", e))?;
            eprintln!("[智语言] 版本管理器已安装至：{}", 目标exe.display());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // POSIX：无映像锁，直接覆盖；并赋予执行位（动态库 / 可执行脚本需要）
        fs::copy(源.join(&管理器名), 目标exe).map_err(|e| format!("安装版本管理器失败: {}", e))?;
        let _ = base::平台::使可执行(目标exe);
        eprintln!("[智语言] 版本管理器已安装至：{}", 目标exe.display());
    }
    Ok(())
}

/// 卸载指定版本（禁止卸载当前激活）
pub fn 卸载(版本号: &str) -> Result<(), String> {
    let 配置 = pm::全局配置::读取配置();
    if 配置.开发套件.开发套件版本 == 版本号 {
        return Err(format!("不能卸载当前激活版本 {}，请先切换", 版本号));
    }
    let 目录 = pm::目录::版本目录(版本号);
    if !目录.exists() { return Err(format!("版本 {} 未安装", 版本号)); }
    fs::remove_dir_all(&目录).map_err(|e| format!("卸载失败: {}", e))?;
    eprintln!("[智语言] 已卸载 {}", 版本号);
    Ok(())
}

/// 递归复制目录内容（源 → 目标）
fn 复制目录(源: &Path, 目标: &Path) -> std::io::Result<()> {
    for 条目 in fs::read_dir(源)? {
        let 条目 = 条目?;
        let 路径 = 条目.path();
        let 相对 = 路径.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "文件名缺失")
        })?;
        let 目标路径 = 目标.join(相对);
        if 路径.is_dir() {
            fs::create_dir_all(&目标路径)?;
            复制目录(&路径, &目标路径)?;
        } else {
            fs::create_dir_all(&目标)?;
            fs::copy(&路径, &目标路径)?;
        }
    }
    Ok(())
}

// ============ 纯 Rust ZIP 解压（stored 方法） ============
fn 读u16(d: &[u8], o: usize) -> u16 { u16::from_le_bytes([d[o], d[o+1]]) }
fn 读u32(d: &[u8], o: usize) -> u32 { u32::from_le_bytes([d[o], d[o+1], d[o+2], d[o+3]]) }

pub fn 解压字节(数据: &[u8], 目标: &Path) -> Result<(), String> {
    解压字节_带进度(数据, 目标, &mut |_, _| {})
}

/// 解压并回报进度：`进度(已处理条目数, 总条目数)`。
/// 图形安装界面用它驱动进度条（195MB / 300+ 条目，没有进度会让用户以为卡死）。
pub fn 解压字节_带进度(
    数据: &[u8],
    目标: &Path,
    进度: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    let 结束记录位置 = (0..=(数据.len().saturating_sub(22))).rev()
        .find(|&偏移| &数据[偏移..偏移+4] == b"PK\x05\x06")
        .ok_or("无效的ZIP文件（找不到结束记录）")?;
    let 条目数 = 读u16(数据, 结束记录位置 + 10) as usize;
    let mut 中央目录偏移 = 读u32(数据, 结束记录位置 + 16) as usize;
    for 序号 in 0..条目数 {
        if &数据[中央目录偏移..中央目录偏移+4] != b"PK\x01\x02" { return Err("无效的中央目录".into()); }
        let 方法 = 读u16(数据, 中央目录偏移 + 10);
        let 压缩大小 = 读u32(数据, 中央目录偏移 + 20) as usize;
        let 名长 = 读u16(数据, 中央目录偏移 + 28) as usize;
        let 额长 = 读u16(数据, 中央目录偏移 + 30) as usize;
        let 注长 = 读u16(数据, 中央目录偏移 + 32) as usize;
        let 本地偏移 = 读u32(数据, 中央目录偏移 + 42) as usize;
        let 名称 = String::from_utf8_lossy(&数据[中央目录偏移+46..中央目录偏移+46+名长]).to_string();
        if &数据[本地偏移..本地偏移+4] != b"PK\x03\x04" { return Err(format!("无效本地文件头: {}", 名称)); }
        let 本地名长 = 读u16(数据, 本地偏移 + 26) as usize;
        let 本地额长 = 读u16(数据, 本地偏移 + 28) as usize;
        let 起点 = 本地偏移 + 30 + 本地名长 + 本地额长;
        let 条目数据 = &数据[起点..起点 + 压缩大小];
        let 输出 = 目标.join(&名称);
        if 名称.ends_with('/') { fs::create_dir_all(&输出).ok(); }
        else {
            if let Some(p) = 输出.parent() { fs::create_dir_all(p).ok(); }
            match 方法 {
                0 => { fs::write(&输出, 条目数据).map_err(|e| format!("写入失败 {}: {}", 名称, e))?; }
                8 => { return Err(format!("条目 {} 使用deflate压缩，请用stored方式打包", 名称)); }
                _ => { return Err(format!("不支持的压缩方法 {}: {}", 方法, 名称)); }
            }
        }
        中央目录偏移 += 46 + 名长 + 额长 + 注长;
        进度(序号 + 1, 条目数);
    }
    Ok(())
}
