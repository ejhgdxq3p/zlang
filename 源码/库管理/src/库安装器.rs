// 文件: 库管理/src/库安装器.rs
// 职责: 库管理器，安装、卸载、更新、封装、发布
// 双模式共享：安装后自动更新 项目依赖.配置 锁文件
//   锁文件 → 依赖解析.rs::构建模块解析方案 → 解释/编译共用

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use super::镜像源客户端;
use super::库版本解析;
use pm::配置读取器;
use pm::依赖解析::声明类型;

/// 锁文件 = 原始名 → 多版本条目列表（规范 6.2.3 多版本处理：每版本独立条目）
type 锁文件 = HashMap<String, Vec<锁条目>>;

/// 锁条目（规范 6.2.3 项目依赖.配置 字段：版本/路径/哈希/源地址，无 管理方式——
/// 管理方式 属 项目.配置 声明层，锁文件不参与决策）
#[derive(Debug, Clone)]
struct 锁条目 {
    版本: String,
    路径: String,
    哈希: String,
    源地址: String,
}

pub struct 安装器 {
    pub 项目根: PathBuf,       // 项目根目录（含 配置/项目.配置）
    pub 本地库目录: PathBuf,   // = 项目根/库/
}

impl 安装器 {
    pub fn 新建(项目根目录: PathBuf) -> Self {
        // ⚠ 这里**不能**预先建 `<项目根>/库/`：安装器也被只读子命令（搜索/信息/已安装/查看依赖）
        //   构造，而无条件 create_dir_all 会让这些命令在用户工作目录里留下一个空 `库/` 目录 ✗
        //   （实测缺陷 D5）。目录改为**懒创建**：真正写入的路径（安装/安装从路径/全局安装）
        //   会 `create_dir_all(目标)`，父目录一并建出 ✓
        let 本地库目录 = 项目根目录.join("库");
        安装器 { 项目根: 项目根目录, 本地库目录 }
    }

    /// 安装库：下载 → 解封 → 写锁文件 → 递归安装传递依赖（规范 6.3.6.5 传递依赖自动处理）
    /// 若 库名 为存在的 .库 文件或库项目目录，则按路径本地安装（规范 6.6.35：库 安装 <路径>）
    /// 按类型分流（读取 项目.配置 声明）：外部语言（手动）→ 提示手动放置；其余 → 镜像源
    pub fn 安装(&self, 库名: &str, 版本约束: Option<&str>) -> Result<(), String> {
        // —— 路径安装优先：支持本地 .库 文件或库项目目录 ——
        let 候选路径 = Path::new(库名);
        if 候选路径.exists() && (候选路径.is_dir() || 候选路径.extension().map_or(false, |e| e == "库")) {
            return self.安装从路径(候选路径, false);
        }
        // —— 类型分流：查询 项目.配置 声明 ——
        if let Some((类型, 原始名, _)) = self.查询依赖声明类型(库名) {
            match 类型 {
                声明类型::外部语言 => {
                    // 外部语言（手动）：工具链不自动下载。检查路径是否已放置
                    return self.检查外部语言路径(&原始名, 库名);
                }
                声明类型::远程 => {
                    // 远程：镜像源下载（按原始名），路径由锁文件记录
                    eprintln!("[智语言] 远程依赖 {}（原始名 {}）：从镜像源下载", 库名, 原始名);
                }
                声明类型::智语言库 => {}
            }
        }
        let 安装名 = self.解析安装名(库名);
        let 约束 = 版本约束.unwrap_or(">= 0.0.0");
        let 可用版本 = 镜像源客户端::查询版本(&安装名)?;
        let 最佳版本 = 库版本解析::选择最佳版本(&可用版本, 约束)?;
        let mut 已解析 = std::collections::HashSet::new();
        self.安装远程库(&安装名, &最佳版本, 版本约束.is_some(), false, &mut 已解析)
    }

    /// 全局安装：安装到全局目录的库/，路径前缀为 .智语言/
    /// 若 库名 为存在的 .库 文件或库项目目录，则按路径本地全局安装（规范 6.6.35：库 全局 安装 <路径>）
    pub fn 安装_全局(&self, 库名: &str, 版本约束: Option<&str>) -> Result<(), String> {
        let 候选路径 = Path::new(库名);
        if 候选路径.exists() && (候选路径.is_dir() || 候选路径.extension().map_or(false, |e| e == "库")) {
            return self.安装从路径(候选路径, true);
        }
        let 安装名 = self.解析安装名(库名);
        let 约束 = 版本约束.unwrap_or(">= 0.0.0");
        let 可用版本 = 镜像源客户端::查询版本(&安装名)?;
        let 最佳版本 = 库版本解析::选择最佳版本(&可用版本, 约束)?;
        let mut 已解析 = std::collections::HashSet::new();
        self.安装远程库(&安装名, &最佳版本, 版本约束.is_some(), true, &mut 已解析)
    }

    /// 查询 项目.配置 中某库（代号或原始名）的声明类型 → (类型, 原始名, 版本约束)
    fn 查询依赖声明类型(&self, 库名: &str) -> Option<(声明类型, String, Option<String>)> {
        use pm::依赖解析;
        let (声明映射, _) = 依赖解析::解析依赖声明(&self.项目根);
        // 先按代号匹配，再按原始名匹配
        if let Some(声明) = 声明映射.get(库名) {
            return Some((声明.类型, 声明.原始名.clone(), 声明.版本约束.clone()));
        }
        if let Some((代号, 声明)) = 声明映射.iter().find(|(_, d)| d.原始名 == 库名) {
            let _ = 代号;
            return Some((声明.类型, 声明.原始名.clone(), 声明.版本约束.clone()));
        }
        None
    }

    /// 解析安装名：代号 → 原始名（有声明时）
    fn 解析安装名(&self, 库名: &str) -> String {
        self.查询依赖声明类型(库名)
            .map(|(_, 原始名, _)| 原始名)
            .unwrap_or_else(|| 库名.to_string())
    }

    /// 外部语言（手动）检查：确认路径已放置，无则报错提示
    fn 检查外部语言路径(&self, 原始名: &str, 代号: &str) -> Result<(), String> {
        // 优先查 项目依赖.配置 锁（路径字段）
        let 锁路径 = self.项目根.join("配置").join("项目依赖.配置");
        let mut 已找到 = false;
        if let Ok(锁) = 读取锁文件(&锁路径) {
            if let Some(列表) = 锁.get(原始名) {
                for 条目 in 列表 {
                    if !条目.路径.is_empty() {
                        let 绝对 = 补全外部路径(&条目.路径, &self.项目根);
                        if 绝对.exists() {
                            已找到 = true;
                        }
                    }
                }
            }
        }
        // 兜底：扫描 外部语言/ 目录下按代号/原始名匹配的目录或文件
        if !已找到 {
            let 外部语言目录 = self.项目根.join("外部语言");
            if 外部语言目录.exists() {
                if let Ok(条目) = fs::read_dir(&外部语言目录) {
                    for e in 条目.flatten() {
                        let 名 = e.file_name().to_string_lossy().to_string();
                        if 名 == 原始名 || 名 == 代号 || 名.starts_with(原始名) || 名.starts_with(代号) {
                            已找到 = true;
                            break;
                        }
                    }
                }
            }
        }
        if 已找到 {
            eprintln!("[智语言] 外部语言 {} 已放置（手动管理）", 原始名);
            Ok(())
        } else {
            Err(format!(
                "外部语言依赖 {} 未找到本地路径。请将库文件放入 项目/外部语言/{} 目录，或在 项目依赖.配置 中配置 路径 字段后重新安装",
                代号, 原始名
            ))
        }
    }

    /// 远程安装单个库并递归安装其运行依赖（传递依赖自动处理）。
    /// `已解析` 记录已处理过的「库名@版本」，防止循环依赖无限递归。
    fn 安装远程库(
        &self,
        库名: &str,
        版本: &str,
        是手动: bool,
        全局: bool,
        已解析: &mut std::collections::HashSet<String>,
    ) -> Result<(), String> {
        let 键 = format!("{}@{}", 库名, 版本);
        if !已解析.insert(键.clone()) {
            return Ok(()); // 已在安装中/已装，跳过（防循环）
        }
        if 全局 {
            println!("正在全局安装 {}...", 库名);
        } else {
            println!("正在安装 {}...", 库名);
        }
        // ★ 2026-10-22（用户视角缺口修复）：选中版本若已被作者**弃用**，必须提示。
        //   弃用机制的意图是「避免新用户误装」（`库 信息` 早已打印警告），但 `库 安装` / `库 更新`
        //   在选中弃用版本时**毫无提示**（实测：区间内含已弃用 1.1.0 ⇒ 静默装上）。
        //   此处不阻止安装（规范未要求拒绝，且既存项目可能已依赖该版本），只把服务端原文提示出来。
        Self::提示弃用版本(库名, 版本);
        let 库文件 = 镜像源客户端::下载库(库名, 版本)?;
        // ★★ 2026-09-20 缺口修复（真实现缺陷；致「插件联动专项测试」4 项长期失败）★★
        //   `下载库` 的契约是 `{库名: 整个 .库 包的 ZIP 原始字节}` ——
        //   见 `镜像源客户端.rs` 第 401-411 行，以及 `tests/集成.rs` 第 180-182 行
        //   （`assert!(下载.contains_key(&库名))`）——**不是**「相对路径 → 内容」的已解包映射。
        //   而旧代码把这个映射直接当「文件名 → 内容」逐个写进 `库/<名><版本>/源码/`
        //   ⇒ 把**整包 ZIP 字节**当成一个"源码文件"落盘，且文件名只有库名、**没有 `.智` 扩展名**：
        //       实证落盘：`库/联动库N0.2.0/源码/联动库N`（普通文件、无扩展名、内容是 PK\x03\x04）
        //       包内真相：`源码/联动库N.智` ✓（用 ZipArchive 列条目确认）
        //   ⇒ 模块加载器按 `源码/<库名>.智` 查找（`模块加载器.rs` 第 142 行 `format!("{}.智", …)`）
        //     必然找不到 ⇒ 报「加载用户模块失败：无法找到模块【库名】」；
        //     且 `配置/` 一并缺失 ⇒ `查询库元数据`、依赖解析、库内声明全部失效。
        //   修法：与**本地路径安装**（下方 `安装从路径`，第 207 行起）**对齐走同一条链路** ——
        //     把下载到的 .库 包写临时文件后交给它，从而一并获得：.库 解包（配置/文档/源码/外部语言）、
        //     库外部语言锁改写、库内打包的智语言依赖解封、精确写锁
        //     （规范 6.3.2「.库 应含 配置/、文档/、源码/」、6.3.5「安装时处理」、6.2.3 锁文件）。
        let 包字节 = 库文件
            .get(库名)
            .ok_or_else(|| format!("下载结果缺少库【{}】", 库名))?
            .clone();
        let 临时包 = std::env::temp_dir().join(format!("智语言_远程安装_{}_{}.库", 库名, 版本));
        fs::write(&临时包, &包字节)
            .map_err(|e| format!("写入临时库文件失败 {}: {}", 临时包.display(), e))?;
        let 安装结果 = self.安装从路径(&临时包, 全局);
        let _ = fs::remove_file(&临时包);
        // 安装结果（版本/路径/哈希）已由 安装从路径 写入 项目依赖.配置（规范 6.2.3）
        安装结果?;

        let _ = 是手动; // 显式指定版本 = 手动锁定；自动解析 = 自动。锁文件不记 管理方式（规范 6.2.3）

        // ── 传递依赖：递归安装（规范 §6.3.6.5 传递依赖自动处理）──
        // ⚠ 2026-10-03 实测修复两个真实缺陷（原先靠镜像源 API 反查依赖，丢信息）：
        //   ① **对象形式依赖装不上**：原实现 `镜像源客户端::查询版本依赖` → `取依赖分类` 只返回**键名**。
        //      对 `别名 = { 原始名 = "真库名", 版本 = "..." }`（规范 §6.2.2），键名是**代号**，
        //      拿代号去镜像源查包 ⇒ `GET /库/<代号>` 404 ⇒ `?` 把**整条安装**带失败 ✗。
        //   ② **库声明的版本约束被忽略**：原实现对传递依赖硬编码 `选择最佳版本(..., ">= 0.0.0")`，
        //      于是中间库声明 `底层库 = ">= 1.0.0, < 2.0.0"` 时，底层库会被装成 **2.0.0**（越界）✗。
        //   ⇒ 改为读**已解包的本库自身** `配置/项目.配置`（权威来源：对象形式的 `原始名` 在此、
        //      文本/对象两种写法的 `版本约束` 也都在此），两项缺陷一并解决，且不必改服务端 API 契约。
        let 库根 = if 全局 { pm::目录::全局库根() } else { self.本地库目录.clone() };
        let 本库目录 = 库根.join(format!("{}{}", 库名, 版本));
        let (声明映射, _) = pm::依赖解析::解析依赖声明(&本库目录);
        for (代号, 声明) in 声明映射 {
            // 只递归安装「运行依赖」中的智语言库（外部语言=手动放置、远程=不走镜像源包，均不在此处理）
            if !matches!(声明.段类型, pm::依赖解析::依赖类型::运行) {
                continue;
            }
            if !matches!(声明.类型, pm::依赖解析::声明类型::智语言库) {
                continue;
            }
            // 对象形式：原始名在 声明.原始名；文本形式：原始名缺省即键名
            let 依赖库 = if 声明.原始名.trim().is_empty() { 代号.clone() } else { 声明.原始名.clone() };
            if 依赖库.trim().is_empty() {
                continue;
            }
            // 用库自己声明的版本约束（缺省才放宽为 >= 0.0.0）
            let 约束 = 声明.版本约束.clone().unwrap_or_else(|| ">= 0.0.0".to_string());
            let 可用版本 = 镜像源客户端::查询版本(&依赖库)?;
            let 最佳 = 库版本解析::选择最佳版本(&可用版本, &约束).map_err(|e| {
                format!("传递依赖 {}@{} 的版本约束【{}】无法满足：{}", 依赖库, 版本, 约束, e)
            })?;
            self.安装远程库(&依赖库, &最佳, false, 全局, 已解析)?;
        }
        Ok(())
    }

    /// 从本地路径安装库（.库 文件或库项目目录），写入 库/<名称><版本>/（或全局），并更新锁文件。
    /// 2026-08-10 修订：保持库的完整项目形态（配置/文档/源码 全部解封到 库/<名称><版本>/，符合规范 6.3.2
    /// .库 应含 配置/、文档/、源码/）；库内打包的 外部语言/ 按规范 6.3.5「安装时处理」解封到使用者项目
    /// 对应目录（外部语言/）；库内打包的智语言依赖 库/ 解封到使用者项目 库/。
    fn 安装从路径(&self, 路径: &Path, 全局: bool) -> Result<(), String> {
        let (名称, 版本, 库根目录, 需清理临时) = if 路径.is_dir() {
            // 库项目目录：读 配置/项目.配置 取名称/版本；库根目录即项目目录本身
            let 配置 = 读取名称版本(&路径.join("配置").join("项目.配置"))?;
            (配置.0, 配置.1, 路径.to_path_buf(), false)
        } else {
            // .库 文件：解压到临时目录，优先读 配置/项目清单.配置
            let 数据 = fs::read(路径).map_err(|e| format!("读取库文件失败: {}", e))?;
            let 临时名 = 路径.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let 临时 = std::env::temp_dir().join(format!("智语言_安装_{}", 临时名));
            let _ = fs::remove_dir_all(&临时);
            fs::create_dir_all(&临时).map_err(|e| format!("创建临时目录失败 {}: {}", 临时.display(), e))?;
            self.解压存储zip(&数据, &临时)?;
            let 清单 = 临时.join("配置").join("项目清单.配置");
            // ★ 2026-10-22：`.库` 包**必须**含清单（服务端上传校验强制、客户端 `库 封装` 必产）。
            //   原先缺失时降级为「从临时文件名解析名称/版本」⇒ 装出来的目录名会退化成
            //   `智语言_远程安装_<名>_<版本>` 这样的临时名（实测），属**静默错值** ⇒ 改为明确拒绝。
            if !清单.exists() {
                let _ = fs::remove_dir_all(&临时);
                return Err("不是有效的 .库 包：缺少 配置/项目清单.配置（包不完整或被篡改）".to_string());
            }
            let (名称, 版本) = 读取名称版本(&清单)?;
            (名称, 版本, 临时, true)
        };

        if !库根目录.join("源码").exists() {
            return Err(format!("库源码目录不存在: {}", 库根目录.join("源码").display()));
        }

        let 库根 = if 全局 { pm::目录::全局库根() } else { self.本地库目录.clone() };
        let 目标 = 库根.join(format!("{}{}", 名称, 版本));
        fs::create_dir_all(&目标).map_err(|e| format!("创建目录失败 {}: {}", 目标.display(), e))?;

        // ① 配置/ 文档/ 源码/ 外部语言/ 四目录解封到 库/<名称><版本>/（保持完整项目形态，库=自包含单元）
        // 外部语言保留在库内（符合业界标准：Python site-packages/包名、Java jar、Node node_modules/包名
        // 均把 native 依赖放在包自己的目录，不外挪到使用者项目；2026-08-10 用户拍板）。
        for 子目录 in ["配置", "文档", "源码", "外部语言"] {
            let 源子目录 = 库根目录.join(子目录);
            if 源子目录.exists() {
                if let Err(e) = 复制目录树(&源子目录, &目标.join(子目录)) {
                    eprintln!("[智语言] 警告：库 {} 解封失败 {} → {}: {}", 子目录, 源子目录.display(), 目标.join(子目录).display(), e);
                }
            }
        }

        // ② 库声明的外部语言路径改写：把库的 外部语言/ 相对路径改写到「库内」并写入引用方锁文件。
        // 例如库声明 外部语言/GTK4/，安装在 库/图形界面库1.0.0/ 下后，引用方锁文件路径写
        // 库/图形界面库1.0.0/外部语言/GTK4/，使引用方编译从库内定位 GTK（不再外拷到项目根 外部语言/）。
        self.改写库外部语言锁(&目标, 全局, &名称, &版本)?;

        // ③ 库内打包的智语言依赖（库/）解封到引用方 库/（规范 6.3.5 对应）
        let 库依赖源 = 库根目录.join("库");
        if 库依赖源.exists() {
            let 库依赖目标 = if 全局 {
                pm::目录::全局库根()
            } else {
                self.本地库目录.clone()
            };
            if let Err(e) = 复制目录树(&库依赖源, &库依赖目标) {
                eprintln!("[智语言] 警告：库依赖解封失败 {} → {}: {}", 库依赖源.display(), 库依赖目标.display(), e);
            } else {
                println!("[智语言] 库依赖已解封到 {}", 库依赖目标.display());
            }
        }

        // 路径安装记录精确版本/路径/哈希（锁文件不记 管理方式，规范 6.2.3）
        self.更新锁文件(&名称, &版本, 全局)?;

        // 清理 .库 解压产生的临时目录
        if 需清理临时 {
            let _ = fs::remove_dir_all(&库根目录);
        }
        println!("{}@{} 安装完成", 名称, 版本);
        Ok(())
    }

    /// 若该版本已被弃用，打印与 `库 信息` 同款警告（**不阻止安装**）。
    /// 网络/解析失败 ⇒ 静默跳过（提示属附加信息，不得因此让安装失败）。
    fn 提示弃用版本(库名: &str, 版本: &str) {
        let 信息 = match 镜像源客户端::查询库信息(库名) {
            Ok(i) => i,
            Err(_) => return,
        };
        if let Some(条) = 信息.弃用.iter().find(|d| d.版本号 == 版本 && d.是否弃用) {
            eprintln!("[智语言] 警告：版本 {} 已弃用（该版本由作者标记，建议改用替代版本）", 版本);
            if !条.替代库.is_empty() {
                eprintln!("[智语言]   替代库：{}", 条.替代库);
            }
            if !条.弃用提示.is_empty() {
                eprintln!("[智语言]   弃用提示：{}", 条.弃用提示);
            }
        }
    }

    /// 解压 stored 方法的 ZIP 数据到目标目录（智语言 .库 均为 stored 压缩，无需 zip crate 依赖）
    fn 解压存储zip(&self, 数据: &[u8], 目标: &Path) -> Result<(), String> {
        let 映射 = Self::解压存储zip到映射(数据)?;
        for (名, 内容) in 映射 {
            // ★ 2026-10-22 安全修复：ZIP 条目名**不可信**（镜像源投毒 / CDN 缓存了别人的包 /
            //   内网中间设备改写）。`Path::join` 遇到绝对路径（`/x`、`C:/x`）会**替换整个路径**，
            //   遇到 `..` 段会**逃出目标目录** ⇒ 任意文件写入。
            //   实测恶意包会尝试写 `C:/逃逸绝对路径.txt`，仅因 OS 权限被拒才没成功
            //   （换一台机器或在可写目录就会成功 ✗）。
            //   ⇒ 逐段清洗（拒绝 `..`、拒绝含盘符/冒号段、跳过空段与 `.`），并二次校验仍在目标内。
            let 相对 = Self::安全解包相对路径(&名)?;
            let 目标路径 = 目标.join(&相对);
            if !目标路径.starts_with(目标) {
                return Err(format!("库文件包含越界路径（已拒绝）：{}", 名));
            }
            if let Some(父) = 目标路径.parent() {
                fs::create_dir_all(父)
                    .map_err(|e| format!("创建解压目录失败 {}: {}", 父.display(), e))?;
            }
            fs::write(&目标路径, 内容)
                .map_err(|e| format!("写入解压文件失败 {}: {}", 目标路径.display(), e))?;
        }
        Ok(())
    }

/// 把 ZIP 条目名清洗成**受约束的相对路径**（解包落盘的唯一入口校验）。
///
/// 拒绝三类越界形态（`.库` 条目名不可信）：
///   ① `..` 段（`../x`、`a/../../x`、`..\x` 归一后同）；
///   ② 绝对路径 / 盘符（`/x`、`C:/x`、`c:x` —— 段内含冒号即拒）；
///   ③ 空路径（清洗后无有效段）。
/// 空段与 `.` 段直接跳过（`a//b`、`./a` 归一为 `a/b`）。
fn 安全解包相对路径(名: &str) -> Result<PathBuf, String> {
    let 归一 = 名.replace('\\', "/");
    let mut 安全 = PathBuf::new();
    for 段 in 归一.split('/') {
        if 段.is_empty() || 段 == "." {
            continue;
        }
        if 段 == ".." {
            return Err(format!(
                "库文件包含越界路径（拒绝了 `..` 段）：{}（.库 条目不得指向包外）",
                名
            ));
        }
        if 段.contains(':') {
            return Err(format!(
                "库文件包含绝对路径（已拒绝）：{}（.库 条目必须是包内相对路径）",
                名
            ));
        }
        安全.push(段);
    }
    if 安全.as_os_str().is_empty() {
        return Err(format!("库文件包含无效条目名（清洗后为空）：{}", 名));
    }
    Ok(安全)
}

/// 解压 stored 方法的 .库 ZIP 数据到内存映射（文件名 → 字节），供镜像源下载复用
/// 智语言 .库 均为 stored（不压缩）条目，无需 zip crate 依赖
pub fn 解压存储zip到映射(数据: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    // 说明：本函数只做"字节 → 内存映射"，**不做路径约束**；落盘前必须经 `安全解包相对路径`。
    let mut 映射: HashMap<String, Vec<u8>> = HashMap::new();
    let n = 数据.len();
    let mut 偏移 = 0usize;
    while 偏移 + 4 <= n {
        if &数据[偏移..偏移 + 4] != b"PK\x03\x04" {
            // 已越过本地文件头区（到达中央目录 PK\x01\x02 或文件尾），停止
            break;
        }
        if 偏移 + 30 > n {
            return Err("库文件格式损坏：本地文件头不完整".to_string());
        }
        let 方法 = u16::from_le_bytes([数据[偏移 + 8], 数据[偏移 + 9]]);
        let 压缩大小 = u32::from_le_bytes([
            数据[偏移 + 18], 数据[偏移 + 19], 数据[偏移 + 20], 数据[偏移 + 21],
        ]) as usize;
        let 名长 = u16::from_le_bytes([数据[偏移 + 26], 数据[偏移 + 27]]) as usize;
        let 扩展长 = u16::from_le_bytes([数据[偏移 + 28], 数据[偏移 + 29]]) as usize;
        let 名起 = 偏移 + 30;
        if 名起 + 名长 > n {
            return Err("库文件格式损坏：文件名越界".to_string());
        }
        let 名 = String::from_utf8_lossy(&数据[名起..名起 + 名长]).to_string();
        let 内容起 = 名起 + 名长 + 扩展长;
        if 方法 != 0 {
            return Err(format!("不支持的库文件压缩方式（{}），仅支持 stored", 方法));
        }
        if 内容起 + 压缩大小 > n {
            return Err("库文件格式损坏：文件内容越界".to_string());
        }
        let 内容 = 数据[内容起..内容起 + 压缩大小].to_vec();
        映射.insert(名, 内容);
        偏移 = 内容起 + 压缩大小;
    }
    // ⚠ 2026-10-03：非 ZIP 数据（首字节不是 `PK\x03\x04`）会被上面的 `break` 静默跳过 ⇒ 得到**空映射**
    //   并当作成功返回 ⇒ 下游只能报出含混的「库源码目录不存在」。合法的 .库 必含
    //   配置/项目清单.配置 + 源码/*.智（服务端也强制 [文件] 非空）⇒ 空映射必为损坏/非 .库 数据。
    //   这里明确拒绝，给出可读错误（下载被截断/代理插了 HTML/镜像源损坏等场景）。
    if 映射.is_empty() {
        return Err("不是有效的 .库 包：未找到任何 ZIP 条目（下载可能被截断或响应不是 .库）".to_string());
    }
    Ok(映射)
}

    /// 更新锁文件：读取/创建 配置/项目依赖.配置，追加或更新指定库条目
    /// 使用 配置读取器 序列化/反序列化
    /// 读取锁中某库首条条目的版本，供 CLI 卸载/路径判断（规范 6.2.3：锁文件不参与决策）
    pub fn 读取锁条目(&self, 库名: &str) -> Option<String> {
        let 锁路径 = self.项目根.join("配置").join("项目依赖.配置");
        读取锁文件(&锁路径).ok().and_then(|锁| {
            锁.get(库名).and_then(|列表| 列表.first()).map(|e| e.版本.clone())
        })
    }

    /// 读取锁中某库首条条目的（版本, 路径），供 CLI 判断已有路径
    pub fn 读取锁条目和路径(&self, 库名: &str) -> Option<(String, String)> {
        let 锁路径 = self.项目根.join("配置").join("项目依赖.配置");
        读取锁文件(&锁路径).ok().and_then(|锁| {
            锁.get(库名).and_then(|列表| 列表.first()).map(|e| (e.版本.clone(), e.路径.clone()))
        })
    }

    /// 读取已安装库的元数据（名称, 类型, 版本），来源：库内 配置/项目清单.配置
    pub fn 查询库元数据(&self, 库名: &str) -> Option<(String, String, String)> {
        let 候选 = vec![
            self.本地库目录.clone(),
            pm::目录::全局库根(),
        ];
        for 库根 in 候选 {
            if let Ok(条目) = fs::read_dir(&库根) {
                for e in 条目.flatten() {
                    let p = e.path();
                    if let Some(名) = p.file_name().and_then(|n| n.to_str()) {
                        if 名.starts_with(库名) {
                            let 清单 = p.join("配置").join("项目清单.配置");
                            if let Ok(s) = fs::read_to_string(&清单) {
                                let 文档 = 配置读取器::解析配置(&s);
                                if let Some(段) = 配置读取器::取段(&文档, "清单") {
                                    let 名称 = 配置读取器::取文本(段, "名称").unwrap_or_default();
                                    let 版本 = 配置读取器::取文本(段, "版本").unwrap_or_default();
                                    let 类型 = 配置读取器::取文本(段, "类型").unwrap_or_default();
                                    if !名称.is_empty() {
                                        return Some((名称, 类型, 版本));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn 更新锁文件(&self, 库名: &str, 版本: &str, 是全局级: bool) -> Result<(), String> {
        let 锁路径 = self.项目根.join("配置").join("项目依赖.配置");

        // 读取现有锁数据 → HashMap<原始名, Vec<锁条目>>
        let mut 锁 = 读取锁文件(&锁路径)?;

        // 路径：全局级带 .智语言/ 前缀，项目级不带
        let 路径 = if 是全局级 {
            format!(".智语言/库/{}{}/源码/", 库名, 版本)
        } else {
            format!("库/{}{}/源码/", 库名, 版本)
        };

        // 哈希
        let 哈希 = if 是全局级 {
            let 全局库根 = pm::目录::全局库根();
            计算目录哈希(&全局库根.join(format!("{}{}/源码", 库名, 版本)))?
        } else {
            计算目录哈希(&self.本地库目录.join(format!("{}{}/源码", 库名, 版本)))?
        };

        // 更新条目（多版本共存：同版本覆盖，异版本追加）
        let 条目列表 = 锁.entry(库名.to_string()).or_default();
        let 已有索引 = 条目列表.iter().position(|e| e.版本 == 版本);
        let 新条目 = 锁条目 {
            版本: 版本.to_string(),
            路径,
            哈希,
            源地址: String::new(),
        };
        match 已有索引 {
            Some(i) => 条目列表[i] = 新条目,
            None => 条目列表.push(新条目),
        }

        写入锁文件(&锁路径, &锁)?;
        Ok(())
    }

    /// 外部语言/远程 依赖写锁：按类型写 版本/路径/哈希/源地址（规范 6.2.3）
    /// 类型说明：外部语言=写路径（外部语言/xxx），远程=写源地址
    pub fn 更新锁文件外部(&self, 原始名: &str, 类型: &str, 版本: &str, 路径: &str, 源地址: &str, 哈希: &str) -> Result<(), String> {
        let 锁路径 = self.项目根.join("配置").join("项目依赖.配置");
        let mut 锁 = 读取锁文件(&锁路径)?;
        let 条目列表 = 锁.entry(原始名.to_string()).or_default();
        // 同版本覆盖
        let 已有索引 = 条目列表.iter().position(|e| e.版本 == 版本);
        let 新条目 = 锁条目 {
            版本: 版本.to_string(),
            路径: 路径.to_string(),
            哈希: 哈希.to_string(),
            源地址: 源地址.to_string(),
        };
        match 已有索引 {
            Some(i) => 条目列表[i] = 新条目,
            None => 条目列表.push(新条目),
        }
        写入锁文件(&锁路径, &锁)?;
        let _ = 类型;
        Ok(())
    }

    /// 🆕 2026-08-10: 库的外部语言路径改写——库安装在 库/<名称><版本>/ 后，其声明的外部语言
    /// （如 gtk4，路径 外部语言/GTK4/）实际位于 库/<名称><版本>/外部语言/GTK4/。
    /// 把库的 项目依赖.配置 中外部语言条目的路径改写到「库内相对路径」并写入引用方锁文件，
    /// 使引用方编译/部署从库内定位外部语言（库=自包含单元，不外拷到项目根 外部语言/）。
    fn 改写库外部语言锁(&self, 库目标目录: &Path, 全局: bool, 库名: &str, 版本: &str) -> Result<(), String> {
        let 库锁路径 = 库目标目录.join("配置").join("项目依赖.配置");
        let 引用锁路径 = self.项目根.join("配置").join("项目依赖.配置");
        if !库锁路径.exists() || !引用锁路径.exists() {
            return Ok(());
        }
        let 库锁 = match 读取锁文件(&库锁路径) {
            Ok(锁) => 锁,
            Err(_) => return Ok(()),
        };
        let mut 引用锁 = 读取锁文件(&引用锁路径)?;
        // 库内相对前缀：库/图形界面库1.0.0/外部语言/（全局为 .智语言/库/.../外部语言/）
        let 库内前缀 = if 全局 {
            format!(".智语言/库/{}{}/外部语言/", 库名, 版本)
        } else {
            format!("库/{}{}/外部语言/", 库名, 版本)
        };
        // 遍历库的 项目依赖.配置：外部语言条目（路径以 外部语言/ 开头）改写为库内路径
        for (原始名, 条目列表) in &库锁 {
            for 条目 in 条目列表 {
                let 是外部语言路径 = 条目.路径.starts_with("外部语言/") || 条目.路径.starts_with("外部语言\\");
                if !是外部语言路径 {
                    continue;
                }
                let 相对 = 条目.路径.trim_start_matches("外部语言/").trim_start_matches("外部语言\\");
                let 新路径 = format!("{}{}", 库内前缀, 相对);
                // 写入引用方锁文件（保留原 版本/哈希/源地址）
                let 引用条目列表 = 引用锁.entry(原始名.clone()).or_default();
                let 已有索引 = 引用条目列表.iter().position(|e| e.版本 == 条目.版本);
                let 新条目 = 锁条目 {
                    版本: 条目.版本.clone(),
                    路径: 新路径.clone(),
                    哈希: 条目.哈希.clone(),
                    源地址: 条目.源地址.clone(),
                };
                match 已有索引 {
                    Some(i) => 引用条目列表[i] = 新条目,
                    None => 引用条目列表.push(新条目),
                }
                println!("[智语言] 库外部语言 {}({}) 路径已指向库内: {}", 原始名, 条目.版本, 新路径);
            }
        }
        写入锁文件(&引用锁路径, &引用锁)
    }

    pub fn 卸载(&self, 库名: &str) -> Result<(), String> {
        // 从锁文件解析已安装版本：安装目录名为「库名+版本」
        let 目录名 = match self.读取锁条目(库名) {
            Some(版本) => format!("{}{}", 库名, 版本),
            None => 库名.to_string(),
        };
        let 库目录 = self.本地库目录.join(&目录名);
        if 库目录.exists() {
            fs::remove_dir_all(&库目录)
                .map_err(|e| format!("卸载失败: {}", e))?;
            println!("{} 已卸载", 库名);
            self.从锁文件删除(库名)?;
        } else {
            // 目录名不同（多版本）时也尝试删除所有匹配目录
            let mut 删除任意 = false;
            if let Ok(条目) = fs::read_dir(&self.本地库目录) {
                for e in 条目.flatten() {
                    let 名 = e.file_name().to_string_lossy().to_string();
                    // ⚠ 精确匹配（不能用 starts_with：会连带删掉名字以 库名 开头的其它库）
                    if 目录名属于库(&名, 库名) && e.path().is_dir() {
                        fs::remove_dir_all(e.path()).ok();
                        删除任意 = true;
                    }
                }
            }
            if 删除任意 {
                println!("{} 已卸载", 库名);
                self.从锁文件删除(库名)?;
            } else {
                println!("{} 未安装", 库名);
            }
        }
        Ok(())
    }

    /// 全局卸载：删除全局库目录下的匹配库，不更新项目锁文件
    pub fn 卸载_全局(&self, 库名: &str) -> Result<(), String> {
        let 全局库根 = pm::目录::全局库根();
        if !全局库根.exists() { return Err("全局库目录不存在".to_string()); }
        let mut 已删除 = false;
        if let Ok(条目) = fs::read_dir(&全局库根) {
            for e in 条目.flatten() {
                let 名 = e.file_name().to_string_lossy().to_string();
                // ⚠ 精确匹配（原先无条件 starts_with ⇒ 卸载「数学」会把「数学工具」一起删掉）✗
                if 目录名属于库(&名, 库名) && e.path().is_dir() {
                    fs::remove_dir_all(e.path()).map_err(|e| format!("卸载失败: {}", e))?;
                    println!("已卸载全局库: {}", 名);
                    已删除 = true;
                }
            }
        }
        if !已删除 { println!("未找到全局库: {}", 库名); }
        // 全局卸载不更新项目锁文件
        Ok(())
    }

    fn 从锁文件删除(&self, 库名: &str) -> Result<(), String> {
        let 锁路径 = self.项目根.join("配置").join("项目依赖.配置");
        if !锁路径.exists() {
            return Ok(());
        }
        let mut 锁 = 读取锁文件(&锁路径)?;
        // 多版本：删除该库名的全部条目（规范 6.3.3 卸载 = 从锁定文件删除该库的条目）
        // 若锁中有多版本且已安装目录是「库名+版本」，卸载时也删除对应版本目录
        if let Some(条目列表) = 锁.get(库名) {
            // 删除本地库目录（全部版本）
            for 条目 in 条目列表 {
                let 目录名 = format!("{}{}", 库名, 条目.版本);
                let 库目录 = self.本地库目录.join(&目录名);
                if 库目录.exists() {
                    let _ = fs::remove_dir_all(&库目录);
                }
            }
        }
        锁.remove(库名);
        写入锁文件(&锁路径, &锁)?;
        Ok(())
    }

    /// 更新库：未指定版本时按约束自动解析最新（自动管理方式）；
    /// 指定版本时锁定到该版本（手动管理方式）。
    pub fn 更新(&self, 库名: &str, 版本: Option<&str>) -> Result<(), String> {
        self.安装(库名, 版本)
    }

    /// 全局更新库：委托给 安装_全局()
    pub fn 更新_全局(&self, 库名: &str, 版本: Option<&str>) -> Result<(), String> {
        self.安装_全局(库名, 版本)
    }

    /// 封装项目为 .库 文件（ZIP格式）
    /// 命令行含库: --包含库 选项（忽略 [封装库].库）
    /// 命令行含外部语言: --包含外部语言 选项（忽略 [封装库].外部语言）
    pub fn 封装(&self, 项目路径: &PathBuf, 命令行含库: bool, 命令行含外部语言: bool) -> Result<PathBuf, String> {
        println!("正在封装项目...");
        let 配置路径 = 项目路径.join("配置").join("项目.配置");
        if !配置路径.exists() {
            return Err("未找到 配置/项目.配置 文件".to_string());
        }

        let 配置内容 = fs::read_to_string(&配置路径)
            .map_err(|e| format!("读取项目配置失败: {}", e))?;
        let 配置文档 = 配置读取器::解析配置(&配置内容);
        let 版本 = 配置读取器::取段(&配置文档, "项目")
            .and_then(|段| 配置读取器::取文本(段, "版本"))
            .unwrap_or_else(|| "0.1.0".to_string());

        // ★ 库名取自 **项目.配置 的「名称」**（规范 §6.2.1 名称=全局唯一；§6.3.5 封装流程第 1 步「验证 名称/版本」）。
        //   ⚠ 2026-10-03 修复：原实现取自**文件夹名** ⇒ 与 `发布` 的上传 URL 取值口径不一致，
        //   而服务端 `发布库.js` 以「包内 项目.配置.名称 == 请求库名(URL)」做一致性校验
        //   ⇒ 一旦**文件夹名 ≠ 名称**，发布必报「包内库名 … 与请求库名 … 不一致」；
        //   文件夹名含英文时还会撞「库名只能包含中文汉字、数字和下划线」⇒ 合法库也发不出去 ✗。
        //   现统一：名称优先，缺失/为空才回退文件夹名。
        let 库名 = 配置读取器::取段(&配置文档, "项目")
            .and_then(|段| 配置读取器::取文本(段, "名称"))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                项目路径
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("未知库")
                    .to_string()
            });

        // ── 读取 [封装库] 配置段 ──
        let 封装库段 = 配置读取器::取段(&配置文档, "封装库");
        let 指定库 = 封装库段.and_then(|段| 配置读取器::取数组(段, "库"));
        let 指定外部语言 = 封装库段.and_then(|段| 配置读取器::取数组(段, "外部语言"));

        let mut 文件列表 = Vec::new();
        self.收集源文件(项目路径, &mut 文件列表, false)?;

        // ── 按规则决定打包哪些依赖 ──
        // 规则：命令行参数优先于 [封装库] 配置
        // --包含库 时打包全部智语言库，否则只列 [封装库].库
        let 需打包库 = if 命令行含库 {
            // 打包所有库依赖
            let 库目录 = 项目路径.join("库");
            if 库目录.exists() {
                self.收集源文件(&库目录, &mut 文件列表, false)?;
                // 返回所有库名以便路径改写
                let mut 库列表 = Vec::new();
                if let Ok(条目) = fs::read_dir(&库目录) {
                    for e in 条目.flatten() {
                        if e.path().is_dir() {
                            if let Some(名) = e.file_name().to_str().map(|s| s.to_string()) {
                                库列表.push(名);
                            }
                        }
                    }
                }
                库列表
            } else {
                Vec::new()
            }
        } else if let Some(arr) = 指定库 {
            // 只打包 [封装库] 中列出的库
            let 库目录 = 项目路径.join("库");
            let mut 已打包 = Vec::new();
            for 库值 in arr {
                if let 配置读取器::配置值::文本(库名指定) = 库值 {
                    let 库源码 = 库目录.join(库名指定);
                    if 库源码.exists() {
                        self.收集源文件(&库源码, &mut 文件列表, false)?;
                        已打包.push(库名指定.clone());
                    }
                }
            }
            已打包
        } else {
            Vec::new() // 默认不打包
        };

        // 外部语言裁剪开关：默认裁剪（只留运行期必需文件）。
        // 需要连编译期资料一起分发（如库的使用者要自行编译 C 代码）时，
        // 在 [封装库] 写 `保留全部外部语言文件 = 真` 关闭裁剪。
        let 保留全部外部语言文件 = 封装库段
            .and_then(|段| 配置读取器::取逻辑(段, "保留全部外部语言文件"))
            .unwrap_or(false);

        // --包含外部语言 时打包全部外部语言，否则只列 [封装库].外部语言
        let _需打包外部语言 = if 命令行含外部语言 {
            let 外部目录 = 项目路径.join("外部语言");
            if 外部目录.exists() {
                // 外部语言运行时目录：全部文件（含 DLL/.so 二进制 + 数据）都要打包进库
                let mut 外部文件 = Vec::new();
                self.收集源文件(&外部目录, &mut 外部文件, true)?;
                self.追加外部语言文件(外部文件, &mut 文件列表, 保留全部外部语言文件);
            }
            true
        } else if let Some(arr) = 指定外部语言 {
            let 外部目录 = 项目路径.join("外部语言");
            for 外部值 in arr {
                if let 配置读取器::配置值::文本(名) = 外部值 {
                    let 外部源码 = 外部目录.join(名);
                    if 外部源码.exists() {
                        let mut 外部文件 = Vec::new();
                        self.收集源文件(&外部源码, &mut 外部文件, true)?;
                        self.追加外部语言文件(外部文件, &mut 文件列表, 保留全部外部语言文件);
                    }
                }
            }
            true
        } else {
            false
        };

        let 临时目录 = std::env::temp_dir().join(format!("智语言_封装_{}", 库名));
        fs::create_dir_all(&临时目录)
            .map_err(|e| format!("创建临时目录失败: {}", e))?;

        for 文件路径 in &文件列表 {
            let 相对路径 = 文件路径.strip_prefix(项目路径).unwrap_or(文件路径);
            let 目标路径 = 临时目录.join(相对路径);
            if let Some(父目录) = 目标路径.parent() {
                fs::create_dir_all(父目录).ok();
            }
            fs::copy(文件路径, &目标路径)
                .map_err(|e| format!("复制文件失败: {}", e))?;
        }

        // 生成项目清单：配置/项目清单.配置，含每个文件的 sha256
        let mut 清单文件段 = String::new();
        for 文件路径 in &文件列表 {
            let 相对 = 文件路径
                .strip_prefix(项目路径)
                .unwrap_or(文件路径)
                .to_string_lossy()
                .replace('\\', "/");
            let 文件哈希 = 计算文件sha256(文件路径)?;
            清单文件段.push_str(&format!("{} = \"{}\"\n", 相对, 文件哈希));
        }
        let 清单内容 = format!(
            "[清单]\n名称 = \"{}\"\n类型 = \"智语言库\"\n版本 = \"{}\"\n文件数 = {}\n\n[文件]\n{}",
            库名, 版本, 文件列表.len(), 清单文件段
        );
        let 清单目录 = 临时目录.join("配置");
        fs::create_dir_all(&清单目录)
            .map_err(|e| format!("创建清单目录失败: {}", e))?;
        fs::write(清单目录.join("项目清单.配置"), &清单内容)
            .map_err(|e| format!("写入项目清单失败: {}", e))?;

        // ── 路径改写：被打包的依赖从全局路径改为项目内路径 ──
        if !需打包库.is_empty() {
            let 锁路径 = 项目路径.join("配置").join("项目依赖.配置");
            if 锁路径.exists() {
                if let Ok(锁内容) = fs::read_to_string(&锁路径) {
                    let 锁文档 = 配置读取器::解析配置(&锁内容);
                    let mut 有改动 = false;
                    let mut 新段 = Vec::new();
                    for 段 in &锁文档.段 {
                        let 路径值 = 配置读取器::取文本(段, "路径").unwrap_or_default();
                        // 若路径以 .智语言/库/ 开头（全局路径），且该库已被打包，改为 库/ 前缀
                        if 路径值.starts_with(".智语言/库/") {
                            // 提取库名（路径格式：.智语言/库/<库名><版本>/源码/）
                            let 路径库名版本 = 路径值.trim_start_matches(".智语言/库/");
                            let 是否被打包 = 需打包库.iter().any(|打包名| 路径库名版本.starts_with(打包名));
                            if 是否被打包 {
                                let 新路径 = format!("库/{}", 路径库名版本);
                                let mut 新键值 = Vec::new();
                                for (k, v) in &段.键值 {
                                    if k == "路径" {
                                        新键值.push((k.clone(), 配置读取器::配置值::文本(新路径.clone())));
                                    } else {
                                        新键值.push((k.clone(), v.clone()));
                                    }
                                }
                                新段.push(配置读取器::配置段 {
                                    段名: 段.段名.clone(),
                                    键值: 新键值,
                                });
                                有改动 = true;
                                continue;
                            }
                        }
                        新段.push(段.clone());
                    }
                    if 有改动 {
                        let 新文档 = 配置读取器::配置文档 {
                            顶层: 锁文档.顶层.clone(),
                            段: 新段,
                        };
                        let 序列化 = 配置读取器::序列化配置文档(&新文档);
                        let 最终内容 = format!("// 此文件由工具链自动生成，请勿手动编辑\n\n{}", 序列化);
                        fs::write(&锁路径, &最终内容).ok();
                    }
                }
            }
        }

        let 库文件名 = format!("{}{}.库", 库名, 版本);
        let 输出路径 = std::env::current_dir()
            .unwrap_or(PathBuf::from("."))
            .join(&库文件名);

        self.压缩目录(&临时目录, &输出路径)?;
        fs::remove_dir_all(&临时目录).ok();
        println!("封装完成: {}", 输出路径.display());
        Ok(输出路径)
    }

    pub fn 发布(&self, 项目路径: &PathBuf, _跳过检查: bool) -> Result<(), String> {
        let 库文件路径 = self.封装(项目路径, false, false)?;
        // ★ 2026-10-03 修复：库名与版本**必须同源取自 项目.配置**（名称/版本）。
        //   服务端一致性校验 = 「包内 项目.配置.名称 == 请求 URL 库名」（发布库.js:127-135）
        //   ⇒ 客户端若用**文件夹名**拼 URL，文件夹名 ≠ 名称时发布必 400「不一致」✗。
        //   封装 已改为按 名称 生成清单/产物名，此处同步按 名称 拼 URL，两者口径一致。
        let (库名, 版本) = {
            let 配置路径 = 项目路径.join("配置").join("项目.配置");
            let 内容 = fs::read_to_string(&配置路径).unwrap_or_default();
            let 文档 = 配置读取器::解析配置(&内容);
            let 项目段 = 配置读取器::取段(&文档, "项目");
            let 名 = 项目段
                .and_then(|段| 配置读取器::取文本(段, "名称"))
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| {
                    项目路径.file_name().and_then(|n| n.to_str()).unwrap_or("未知库").to_string()
                });
            let 版 = 项目段
                .and_then(|段| 配置读取器::取文本(段, "版本"))
                .unwrap_or_else(|| "0.1.0".to_string());
            (名, 版)
        };
        let 库名 = 库名.as_str();
        let 版本 = 版本.as_str();
        let 库数据 = fs::read(&库文件路径).map_err(|e| format!("读取库文件失败: {}", e))?;
        match 镜像源客户端::发布(库名, 版本, &库数据) {
            Ok(()) => { eprintln!("[智语言] 发布完成：{} {}", 库名, 版本); Ok(()) }
            Err(e) => Err(format!("发布失败: {}", e)),
        }
    }

    /// 收集源文件。含二进制=true 时纳入目录下全部文件（含 DLL/.so/.dylib 等外部语言二进制与数据），
    /// 用于 --包含外部语言 打包整个 外部语言/ 运行时目录（2026-08-10 修复：此前只收 .智 导致 GTK 打不进库）。
    /// 含二进制=false 时保持旧行为：仅 .智 + 配置/文档。
    fn 收集源文件(&self, 目录: &PathBuf, 文件列表: &mut Vec<PathBuf>, 含二进制: bool) -> Result<(), String> {
        if !目录.exists() {
            return Ok(());
        }
        let 条目 = fs::read_dir(目录).map_err(|e| format!("读取目录失败: {}", e))?;
        for 条目 in 条目 {
            let 条目 = 条目.map_err(|e| format!("读取条目失败: {}", e))?;
            let 路径 = 条目.path();
            if 路径.is_dir() {
                let 目录名 = 路径
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                // 跳过：隐藏目录、库（显式包含时才收集）、产出、测试
                if 目录名.starts_with('.') || 目录名 == "库" || 目录名 == "产出" || 目录名 == "测试" {
                    continue;
                }
                self.收集源文件(&路径, 文件列表, 含二进制)?;
            } else {
                // 配置/ 与 文档/ 下的所有文件一律纳入；含二进制 时其余目录所有文件纳入；
                // 否则仅纳入 .智 源文件
                let 是智源 = 路径.extension().map(|e| e == "智").unwrap_or(false);
                if 含二进制 || 是智源 || Self::处于配置或文档目录(&路径) {
                    文件列表.push(路径);
                }
            }
        }
        Ok(())
    }

    /// 把收集到的外部语言文件追加进封装列表，默认按「运行期必需」裁剪。
    ///
    /// 背景：外部语言目录常直接来自上游 SDK 完整解包（如 GTK4 含 745MB 调试符号 .pdb、
    /// 3000+ 编译期头文件、上百个开发工具 exe），全量打包会让 .库 膨胀到 GB 级，
    /// 而库的使用者只需要运行期文件。裁剪后同时打印节省量，行为对用户可见。
    fn 追加外部语言文件(
        &self,
        外部文件: Vec<PathBuf>,
        文件列表: &mut Vec<PathBuf>,
        保留全部: bool,
    ) {
        if 保留全部 {
            文件列表.extend(外部文件);
            return;
        }
        let 原数量 = 外部文件.len();
        let mut 原字节: u64 = 0;
        let mut 剔除字节: u64 = 0;
        let mut 剔除数量 = 0usize;
        for 路径 in 外部文件 {
            let 大小 = fs::metadata(&路径).map(|m| m.len()).unwrap_or(0);
            原字节 += 大小;
            if Self::是运行期必需(&路径) {
                文件列表.push(路径);
            } else {
                剔除字节 += 大小;
                剔除数量 += 1;
            }
        }
        if 剔除数量 > 0 {
            println!(
                "[智语言] 外部语言裁剪：剔除 {}/{} 个非运行期文件，节省 {:.1} MB（如需完整分发，在 [封装库] 设 保留全部外部语言文件 = 真）",
                剔除数量,
                原数量,
                剔除字节 as f64 / 1024.0 / 1024.0
            );
            let _ = 原字节;
        }
    }

    /// 判断外部语言目录下的单个文件是否为「运行期必需」。
    ///
    /// 跨平台：保留三平台全部动态库形态（.dll/.so/.dylib）与静态库（.lib/.a），
    /// 不因当前构建平台而裁掉其他平台的产物——库可能被跨平台分发。
    fn 是运行期必需(路径: &Path) -> bool {
        let 扩展名 = 路径
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // ① 明确剔除：调试符号与编译期中间产物
        const 剔除扩展名: [&str; 6] = ["pdb", "ilk", "exp", "obj", "o", "map"];
        if 剔除扩展名.contains(&扩展名.as_str()) {
            return false;
        }

        // ② 明确保留：三平台动态库/静态库（跨平台分发不按当前平台裁剪）
        const 库扩展名: [&str; 5] = ["dll", "so", "dylib", "lib", "a"];
        if 库扩展名.contains(&扩展名.as_str()) {
            return true;
        }
        // Linux 版本化 .so（libfoo.so.2.4.0）扩展名不是 so，需按文件名判断
        let 文件名 = 路径
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if 文件名.contains(".so.") {
            return true;
        }

        // ③ 按所在目录裁剪：include/ 头文件与 lib/pkgconfig 仅编译期需要
        let mut 目录名集 = Vec::new();
        let mut 当前 = 路径.to_path_buf();
        while let Some(父) = 当前.parent() {
            当前 = 父.to_path_buf();
            if let Some(名) = 当前.file_name().and_then(|n| n.to_str()) {
                目录名集.push(名.to_ascii_lowercase());
            } else {
                break;
            }
        }
        // include/ 下全部为编译期头文件
        if 目录名集.iter().any(|名| 名 == "include") {
            return false;
        }
        // pkgconfig/ 与 cmake/ 为构建系统描述文件
        if 目录名集
            .iter()
            .any(|名| 名 == "pkgconfig" || 名 == "cmake")
        {
            return false;
        }

        // ④ 开发工具可执行文件：bin/ 下的 exe 多为上游附带的命令行工具，运行期不需要
        //    （运行期需要的是同目录 DLL，已在 ② 保留）
        if 扩展名 == "exe" && 目录名集.iter().any(|名| 名 == "bin") {
            return false;
        }

        // ⑤ 其余（资源、图标、schema、locale、配置等数据文件）默认保留：
        //    GTK 等运行期强依赖 share/glib-2.0/schemas、icons 等数据。
        true
    }

    /// 判断给定文件是否位于 配置/ 或 文档/ 目录树内（用于封装时纳入配置与文档）
    fn 处于配置或文档目录(路径: &Path) -> bool {
        let mut 当前 = 路径.to_path_buf();
        while let Some(父) = 当前.parent() {
            当前 = 父.to_path_buf();
            let 名 = 当前.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if 名 == "配置" || 名 == "文档" {
                return true;
            }
        }
        false
    }

    fn 压缩目录(&self, 源目录: &PathBuf, 输出文件: &PathBuf) -> Result<(), String> {
        use crate::存储打包 as 打包;
        let mut 文件列表: Vec<(String, Vec<u8>)> = Vec::new();
        let mut 路径列表: Vec<PathBuf> = Vec::new();
        收集所有文件(源目录, &mut 路径列表)?;
        for p in &路径列表 {
            let 相对 = p.strip_prefix(源目录).unwrap_or(p)
                .to_string_lossy().replace('\\', "/");
            let 内容 = fs::read(p).map_err(|e| format!("读取文件失败: {}", e))?;
            文件列表.push((相对, 内容));
        }
        let 引用列表: Vec<(&str, &[u8])> = 文件列表.iter()
            .map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        let zip字节 = 打包::构造存储zip(&引用列表);
        fs::write(输出文件, &zip字节).map_err(|e| format!("写入库文件失败: {}", e))?;
        Ok(())
    }
}

/// 外部语言路径补全（规范 6.2.3 路径补全规则）：
///   .智语言/ → 全局目录父目录拼接；其他 → 项目目录拼接
fn 补全外部路径(路径串: &str, 项目根: &Path) -> PathBuf {
    if 路径串.starts_with(".智语言/") {
        pm::目录::全局目录().join(路径串.trim_start_matches(".智语言/").trim_start_matches('/'))
    } else if 路径串.starts_with('/') {
        PathBuf::from(路径串)
    } else {
        项目根.join(路径串)
    }
}

/// 从 项目.配置 / 项目清单.配置 解析 名称 / 版本 键值（使用 配置读取器）
fn 读取名称版本(配置路径: &Path) -> Result<(String, String), String> {
    let 文档 = 配置读取器::解析文件(配置路径)
        .map_err(|e| format!("读取配置失败: {}", e))?;
    // 尝试 [项目] 段（项目.配置）
    if let Some(段) = 配置读取器::取段(&文档, "项目") {
        let 名称 = 配置读取器::取文本(段, "名称").ok_or_else(|| {
            format!("配置缺少 名称 字段: {}", 配置路径.display())
        })?;
        let 版本 = 配置读取器::取文本(段, "版本").unwrap_or_else(|| "0.1.0".into());
        return Ok((名称, 版本));
    }
    // 尝试 [清单] 段（项目清单.配置）
    if let Some(段) = 配置读取器::取段(&文档, "清单") {
        let 名称 = 配置读取器::取文本(段, "名称").ok_or_else(|| {
            format!("清单配置缺少 名称 字段: {}", 配置路径.display())
        })?;
        let 版本 = 配置读取器::取文本(段, "版本").unwrap_or_else(|| "0.1.0".into());
        return Ok((名称, 版本));
    }
    Err(format!("配置缺少 [项目] 或 [清单] 段: {}", 配置路径.display()))
}

// 已删除 `从库文件名解析(stem)`（2026-10-22）：
//   它只在「.库 缺 配置/项目清单.配置」时作降级（从临时文件名猜 名称/版本），
//   而那种降级会把安装目录名写成 `智语言_远程安装_<名>_<版本>` 这样的**临时名**（实测），
//   属静默错值。现在缺清单 ⇒ 明确报「不是有效的 .库 包」，该函数不再有调用者。

/// 递归复制目录树
fn 复制目录树(源: &Path, 目标: &Path) -> Result<(), String> {
    fs::create_dir_all(目标).map_err(|e| format!("创建目录失败 {}: {}", 目标.display(), e))?;
    let 条目 = fs::read_dir(源).map_err(|e| format!("读取目录失败: {}", e))?;
    for 条目 in 条目.flatten() {
        let 路径 = 条目.path();
        let 目标路径 = 目标.join(条目.file_name());
        if 路径.is_dir() {
            复制目录树(&路径, &目标路径)?;
        } else {
            fs::copy(&路径, &目标路径).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

/// 判断某个「库目录名」是否**精确属于**指定库（库目录名约定 = `<库名><版本>`，版本 = 主.次.修订）。
///
/// ⚠ 2026-10-03 修复前缀撞车：`卸载`/`卸载_全局` 原先用 `目录名.starts_with(库名)` 匹配，
///   于是卸载 `数学` 会把 `数学工具` 一起删掉（更危险的是全局卸载**无条件**用前缀匹配）✗。
///   正确判据 = 目录名恰好等于 库名（无版本目录）或 库名 + 纯数字三段版本号。
fn 目录名属于库(目录名: &str, 库名: &str) -> bool {
    if 目录名 == 库名 {
        return true;
    }
    match 目录名.strip_prefix(库名) {
        Some(版本) if !版本.is_empty() => {
            let 段: Vec<&str> = 版本.split('.').collect();
            段.len() == 3
                && 段.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        }
        _ => false,
    }
}

/// 使用 配置读取器 从锁文件读取数据 → HashMap<原始名, Vec<锁条目>>（多版本共存）
fn 读取锁文件(锁路径: &Path) -> Result<锁文件, String> {
    if !锁路径.exists() {
        return Ok(HashMap::new());
    }
    let 内容 = fs::read_to_string(锁路径)
        .map_err(|e| format!("读取锁文件失败: {}", e))?;
    let 文档 = 配置读取器::解析配置(&内容);
    let mut 锁: 锁文件 = HashMap::new();
    for 段 in &文档.段 {
        let 条目 = 锁条目 {
            版本: 配置读取器::取文本(段, "版本").unwrap_or_default(),
            路径: 配置读取器::取文本(段, "路径").unwrap_or_default(),
            哈希: 配置读取器::取文本(段, "哈希").unwrap_or_default(),
            源地址: 配置读取器::取文本(段, "源地址").unwrap_or_default(),
        };
        // 多版本共存：同库名追加条目（规范 6.2.3 多版本处理）
        锁.entry(段.段名.clone()).or_default().push(条目);
    }
    Ok(锁)
}

/// 将锁数据序列化为 .配置 格式并写入文件（使用 配置读取器）
/// 多版本：每个版本独立一个段（配置读取器支持同名段按出现顺序追加）
fn 写入锁文件(锁路径: &Path, 锁: &锁文件) -> Result<(), String> {
    // 将 HashMap 转换为配置文档
    let mut 文档 = 配置读取器::配置文档 {
        顶层: vec![],
        段: vec![],
    };
    let mut 库名列表: Vec<&String> = 锁.keys().collect();
    库名列表.sort();
    for 库名 in 库名列表 {
        if let Some(条目列表) = 锁.get(库名) {
            // 版本排序（稳定顺序，便于阅读）
            let mut 排序列表 = 条目列表.clone();
            排序列表.sort_by(|a, b| a.版本.cmp(&b.版本));
            for 条目 in 排序列表.iter() {
                let mut 键值 = vec![
                    ("版本".into(), 配置读取器::配置值::文本(条目.版本.clone())),
                ];
                if !条目.路径.is_empty() {
                    键值.push(("路径".into(), 配置读取器::配置值::文本(条目.路径.clone())));
                }
                if !条目.哈希.is_empty() {
                    键值.push(("哈希".into(), 配置读取器::配置值::文本(条目.哈希.clone())));
                }
                if !条目.源地址.is_empty() {
                    键值.push(("源地址".into(), 配置读取器::配置值::文本(条目.源地址.clone())));
                }
                文档.段.push(配置读取器::配置段 {
                    段名: 库名.clone(),
                    键值,
                });
            }
        }
    }
    let 序列化 = 配置读取器::序列化配置文档(&文档);
    let 最终内容 = format!("// 此文件由工具链自动生成，请勿手动编辑\n\n{}", 序列化);
    fs::create_dir_all(锁路径.parent().unwrap_or(Path::new("."))).ok();
    fs::write(锁路径, &最终内容)
        .map_err(|e| format!("写入锁文件失败: {}", e))?;
    Ok(())
}



/// 计算单个文件的 SHA-256（用于项目清单逐文件校验）
fn 计算文件sha256(路径: &Path) -> Result<String, String> {
    use crate::校验;
    let 内容 = fs::read(路径).map_err(|e| format!("读取文件失败 {}: {}", 路径.display(), e))?;
    Ok(format!("sha256:{}", 校验::计算sha256(&内容)))
}

/// 计算目录哈希（使用 库管理 内建 SHA-256，与 sha2 crate 结果一致）
fn 计算目录哈希(目录: &Path) -> Result<String, String> {
    use crate::校验;
    let mut 文件列表: Vec<PathBuf> = Vec::new();
    收集所有文件(目录, &mut 文件列表)?;
    文件列表.sort();
    // 对排序后的文件路径+内容串联计算 SHA-256
    let mut 串联数据 = Vec::new();
    for 文件路径 in &文件列表 {
        let 相对 = 文件路径.strip_prefix(目录).unwrap_or(文件路径);
        串联数据.extend_from_slice(相对.to_string_lossy().as_bytes());
        let 内容 = fs::read(文件路径)
            .map_err(|e| format!("读取文件失败 {}: {}", 文件路径.display(), e))?;
        串联数据.extend_from_slice(&内容);
    }
    let 哈希 = 校验::计算sha256(&串联数据);
    Ok(format!("sha256:{}", 哈希))
}

fn 收集所有文件(目录: &Path, 文件列表: &mut Vec<PathBuf>) -> Result<(), String> {
    if !目录.exists() {
        return Ok(());
    }
    for 条目 in std::fs::read_dir(目录)
        .map_err(|e| format!("读取目录失败: {}", e))?
        .flatten()
    {
        let 路径 = 条目.path();
        if 路径.is_dir() {
            收集所有文件(&路径, 文件列表)?;
        } else {
            文件列表.push(路径);
        }
    }
    Ok(())
}