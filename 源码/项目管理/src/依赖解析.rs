// 文件: 工程管理/src/依赖解析.rs
// 职责: 解析 项目依赖.配置 锁 + 项目.配置 → 产出"模块解析方案"（双模式共享的导入真相源）
// 规范 6.2.3 路径规则 + 6.2.2 依赖类型上下文限制

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::目录;
use crate::配置读取器;

/// 项目 `<根>/外部语言/` 下的**一级子目录**（每个子目录通常对应一个外部语言依赖，
/// 如 `外部语言/嵌套结构库/嵌套结构库.dll` —— 规范 §6.5.4 的标准布局）。
///
/// 【为何需要】工具链有两处按「目录 + 库名」解析外部库，而 rustc 的 `-L` 是**非递归**的：
///   · 编译期：`#[link(name=..)]` 需在 `-L` 目录中找到同名 .dll/.lib；
///   · 运行期：shim / 可执行文件加载外部库、以及**非 ASCII 库名的安全名副本**部署
///     （`z_lib__uXXXX_…_.dll`），都按「目录 + 库名」搜索。
/// 若只把 `<根>/外部语言` 入列，则 `<库名>/` 内的库两头都落空。实测（2026-09-25）：
/// 解释模式**首次**运行因找不到库而落不下安全名副本 ⇒ shim 加载 `os_err=126`；
/// 编译模式 exe 启动 `0xC0000135`（导入表按安全名记录，文件缺失）。
pub fn 外部语言一级子目录(根: &Path) -> Vec<PathBuf> {
    let 外部语言 = 根.join("外部语言");
    let mut 结果 = Vec::new();
    if let Ok(条目列表) = std::fs::read_dir(&外部语言) {
        for 条目 in 条目列表.flatten() {
            let 路径 = 条目.path();
            if 路径.is_dir() {
                结果.push(路径);
            }
        }
    }
    结果
}

/// 依赖类型（对应 项目.配置 的三个段）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 依赖类型 {
    运行,
    开发,
    构建,
}

/// 运行上下文（导入检查用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 运行上下文 {
    编译源码,  // 只允许 [运行依赖]
    运行测试,  // 允许 [运行依赖] + [开发依赖]
    构建脚本,  // 允许 [构建依赖]
}

impl 运行上下文 {
    /// 检查是否允许导入指定类型的库
    pub fn 允许导入(&self, 库类型: 依赖类型) -> bool {
        match (self, 库类型) {
            (运行上下文::编译源码, 依赖类型::运行) => true,
            (运行上下文::运行测试, 依赖类型::运行) => true,
            (运行上下文::运行测试, 依赖类型::开发) => true,
            (运行上下文::构建脚本, 依赖类型::构建) => true,
            _ => false,
        }
    }
}

/// 依赖声明类型（项目.配置 [运行/开发/构建依赖] 中 类型 字段）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 声明类型 {
    /// 不写 类型 = 智语言库
    智语言库,
    /// 类型 = "外部语言"：外部语言库/文件（本地手动管理）
    外部语言,
    /// 类型 = "远程"：远程外部库（镜像源自动下载）
    远程,
}

/// 项目.配置 中声明的单个依赖条目（代号 → 声明）
#[derive(Debug, Clone)]
pub struct 依赖声明 {
    pub 类型: 声明类型,
    /// 镜像源真实库名。代号和原始名相同时 = 代号
    pub 原始名: String,
    /// 版本约束（本地库必填；远程不填，版本在源地址中）
    pub 版本约束: Option<String>,
    /// 管理方式："自动" / "手动"（外部语言与远程必须填）
    pub 管理方式: String,
    /// 所属依赖段（运行/开发/构建），用于导入上下文权限检查
    pub 段类型: 依赖类型,
}

/// 外部语言/远程 映射条目（供编译命令按扩展名驱动处理）
#[derive(Debug, Clone)]
pub struct 外部语言条目 {
    /// 代码中 `导入` 使用的代号
    pub 代号: String,
    /// 原始名（锁文件表头）
    pub 原始名: String,
    /// 绝对物理路径（补全后）。远程条目可能为空（版本在源地址）
    pub 路径: Option<PathBuf>,
    /// 精确版本号（锁文件记录）
    pub 版本: String,
    /// 管理方式
    pub 管理方式: String,
    /// 源地址（仅远程条目有）
    pub 源地址: String,
}

/// 模块解析方案：库条目列表 + 解析顺序 + 标准库根 + 项目源码根
/// 解释模式（虚拟机/模块加载器）与编译模式（代码生成）共用同一张表，导入语义天然一致
#[derive(Debug, Clone)]
pub struct 模块解析方案 {
    /// (原始名, 版本, 绝对源码根路径, 依赖类型) — 支持多版本共存
    pub 库源码映射: Vec<(String, String, PathBuf, 依赖类型)>,
    pub 解析顺序: Vec<String>,
    pub 标准库根: PathBuf,
    pub 标准库源码根: PathBuf,
    pub 项目源码根: PathBuf,
    /// 代号 → 原始名（两级映射，供 `导入 代号` 解析到锁文件条目）
    pub 代号映射: std::collections::HashMap<String, String>,
    /// 外部语言/远程 条目（不进库源码映射，供编译命令/库命令使用）
    pub 外部语言映射: Vec<外部语言条目>,
    /// 代号 → 声明（项目.配置，供库命令更新决策）
    pub 声明映射: std::collections::HashMap<String, 依赖声明>,
}

/// 导入层级（规范 §2.7.5 三层分派口径）。
///
/// 【为什么需要】规范对「模块来自哪里」有**三种互斥形态**，其来源与寻址方式各不相同：
///   · 核心库：语言内置（8 个全局函数 + 文本/数组/映射/异常错误的内置方法）——**无需导入**（§4.1）；
///   · 标准库：`导入 数学`、`导入 数学/几何/圆形`（无库名前缀，语言内置、编译器直接识别）——**免依赖声明**；
///   · 第三方库：`导入 库名/模块路径`（库名 = `项目.配置` 代号，§6.2.2）——**须依赖声明**；
///   · 项目内：`导入 服务/工具`（基于本作用域 `源码/` 根）——路径定位。
/// 与 Rust(`std` vs crate)/Go(标准库 vs module path)/Python(标准库 vs site-packages) 同构。
/// ⇒ 解析必须**按形态分派**，不能把各层根拼成一个扁平列表靠「顺序首命中」决胜负（那会破坏层隔离）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum 模块层级 {
    /// 第三方库层：`导入 库名/模块路径`，库名命中 `项目.配置` 代号（经 代号→原始名→锁条目）。
    第三方库 {
        代号: String,
        原始名: String,
        版本: String,
    },
    /// 项目内层：基于本作用域 `源码/` 根（项目文件 = 项目源码根；库文件 = **该库**源码根）。
    项目内,
}

/// 模块定位结果（按形态分派后的物理落点）。标准库层由前端先行分派（其单一真相在 `标准库元数据`）。
#[derive(Debug, Clone)]
pub struct 模块定位 {
    /// 命中的 `.智` 文件
    pub 文件: PathBuf,
    /// 归属层级
    pub 层级: 模块层级,
    /// 剥离库名前缀后的相对路径段（项目内层 = 原路径）
    pub 相对段: Vec<String>,
}

impl 模块解析方案 {
    /// 代号 → 原始名（无映射时返回原样，兼容同名库）
    pub fn 解析原始名<'a>(&'a self, 库名: &'a str) -> &'a str {
        self.代号映射.get(库名).map_or(库名, |s| s.as_str())
    }

    /// 精确版本查询（库名可为代号）
    pub fn 查询库源码根_精确(&self, 库名: &str, 版本: &str) -> Option<&PathBuf> {
        let 原始名 = self.解析原始名(库名);
        self.库源码映射
            .iter()
            .find(|(n, v, _, _)| n == 原始名 && v == 版本)
            .map(|(_, _, p, _)| p)
    }

    /// 无版本查询（取已装**最高**版本；库名可为代号）
    /// ⚠ 2026-10-22：原实现取"锁文件第一条" ⇒ 多版本共存时加载最旧版本（`库 更新` 后仍跑旧代码）。
    pub fn 查询库源码根(&self, 库名: &str) -> Option<&PathBuf> {
        let 原始名 = self.解析原始名(库名);
        self.库源码映射
            .iter()
            .filter(|(n, _, _, _)| n == 原始名)
            .max_by(|a, b| crate::版本号::比较版本(&a.1, &b.1))
            .map(|(_, _, p, _)| p)
    }

    /// 带上下文权限检查的查询：返回路径 + 检查是否允许导入
    pub fn 查询并检查权限(&self, 库名: &str, 上下文: 运行上下文) -> Result<&PathBuf, String> {
        let 原始名 = self.解析原始名(库名);
        let 条目 = self.库源码映射
            .iter()
            .filter(|(n, _, _, _)| n == 原始名)
            .max_by(|a, b| crate::版本号::比较版本(&a.1, &b.1))
            .ok_or_else(|| format!("库\"{}\"未在项目依赖中声明", 库名))?;
        if !上下文.允许导入(条目.3) {
            let 类型名 = match 条目.3 {
                依赖类型::运行 => "运行依赖",
                依赖类型::开发 => "开发依赖",
                依赖类型::构建 => "构建依赖",
            };
            return Err(format!(
                "库\"{}\"是{}，不可在当前上下文中导入。帮助: 将其移至正确的依赖段，或仅在对应上下文中使用",
                库名, 类型名
            ));
        }
        Ok(&条目.2)
    }

    /// 查询外部语言条目（库名可为代号；返回该原始名的全部版本条目）
    pub fn 查询外部语言(&self, 库名: &str) -> Vec<&外部语言条目> {
        let 原始名 = self.解析原始名(库名);
        self.外部语言映射
            .iter()
            .filter(|e| e.原始名 == 原始名 || e.代号 == 库名)
            .collect()
    }

    /// 生成有序搜索路径（双模式导入解析唯一真相）：
    ///   标准库根 → 标准库源码根 → 项目根 → 项目源码根 → 当前目录
    /// 🆕 项目根：使 `导入 外部语言/数据库` 能命中 项目根/外部语言/数据库.智（FFI 外部声明文件）
    ///
    /// ⚠ 2026-10-15（**三层分派收口**，规范 §2.7.5）：**不再**把 `库源码映射` 的每个源码根**扁平入列**。
    ///   规范是三层模型（核心库免导入 / 标准库免声明导入 / 第三方库 `导入 库名/模块路径` 须声明 /
    ///   项目内基于本作用域 `源码/` 根），三层靠**形态分派**判定；旧实现把库根平铺进本列表后，
    ///   靠「扁平顺序 + 首个命中」决定语义，产生两处**偏离规范**的行为：
    ///     ① 任意文件写**无前缀库内模块名**（如 `导入 示例工具库`）会**偶然命中**库文件
    ///        ⇒ 规范未定义的通道；且使 §2.7.5「不同库同名模块必须用别名，否则报错」无从判定
    ///        （两个库同名模块的书写键相同 ⇒ 静默取先者）；
    ///     ② 规范写法 `导入 库名/模块` 的首段被当作**目录名**去撞 ⇒ 报「无法找到模块【库名.模块】」。
    ///   ⇒ 现改为：标准库/项目内走本列表（层内定位）；第三方库由 `定位模块` 按
    ///     「库名（`项目.配置` 代号）⇒ 该库源码根」**定向**解析；库内文件的项目内导入以其
    ///     **本库** `源码/` 根为基准（`库归属`）⇒ 库仍是独立解析作用域，可改名、可作传递依赖复用。
    pub fn 搜索路径列表(&self) -> Vec<PathBuf> {
        let mut 列表 = Vec::new();
        let mut 已见: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        加入路径(&mut 列表, &mut 已见, &self.标准库根);
        加入路径(&mut 列表, &mut 已见, &self.标准库源码根);
        if let Some(项目根) = self.项目源码根.parent() {
            加入路径(&mut 列表, &mut 已见, 项目根);
        }
        加入路径(&mut 列表, &mut 已见, &self.项目源码根);
        if let Ok(当前目录) = std::env::current_dir() {
            加入路径(&mut 列表, &mut 已见, &当前目录);
        }
        列表
    }

    /// 第三方库条目查询（规范 §2.7.5 / §6.2.2）：`导入 库名/模块路径` 的**库名 = `项目.配置` 表头（代号）**。
    /// 返回 `(代号, 原始名, 版本, 源码根)`：代号→原始名→锁条目（§6.2.3）三跳。
    pub fn 第三方库条目<'a>(&'a self, 库名: &str) -> Option<(String, String, String, &'a PathBuf)> {
        let 原始名 = self.解析原始名(库名).to_string();
        // 取**最高**已装版本（构建方案时已按声明约束择优去重；此处为防御性双保险）
        self.库源码映射
            .iter()
            .filter(|(名, _, _, _)| *名 == 原始名)
            .max_by(|a, b| crate::版本号::比较版本(&a.1, &b.1))
            .map(|(名, 版本, 根, _)| (库名.to_string(), 名.clone(), 版本.clone(), 根))
    }

    /// 按规范 §2.7.5 **三层分派**定位模块文件（标准库层由调用方先行分派，见 `模块层级` 注释）：
    ///   ① 首段命中已声明依赖（代号/原始名）⇒ **第三方库层**：剥离库名，在该库 `源码/` 根下按相对段定位；
    ///   ② 否则 ⇒ **项目内层**：在 `项目内候选根`（作用域 `源码/` 根、当前文件目录、项目根…）下按整段定位。
    /// 层与层之间**不靠搜索顺序竞争**：首段命中库名即第三方库层，绝不回落到项目内层去撞。
    pub fn 定位模块(
        &self,
        模块路径: &[String],
        项目内候选根: &[PathBuf],
    ) -> Result<模块定位, String> {
        if 模块路径.is_empty() {
            return Err("导入路径为空".to_string());
        }
        let 首段 = 模块路径[0].as_str();
        // ── ① 第三方库层 ──
        if let Some((代号, 原始名, 版本, 库根)) = self.第三方库条目(首段) {
            let 相对段: Vec<String> = 模块路径[1..].to_vec();
            if 相对段.is_empty() {
                return Err(format!(
                    "第三方库【{}】必须按「库名/模块路径」书写，不能只写库名；例如 `导入 {}/模块名`",
                    代号, 代号
                ));
            }
            let mut 文件 = 库根.clone();
            for 段 in &相对段 {
                文件.push(段);
            }
            文件.set_extension("智");
            if 文件.is_file() {
                return Ok(模块定位 {
                    文件,
                    层级: 模块层级::第三方库 { 代号, 原始名, 版本 },
                    相对段,
                });
            }
            return Err(format!(
                "第三方库【{}】（原始名 {} {}）中找不到模块【{}】（已查 {}）",
                代号,
                原始名,
                版本,
                相对段.join("/"),
                文件.display()
            ));
        }
        // ── ② 项目内层 ──
        for 根 in 项目内候选根 {
            let mut 文件 = 根.clone();
            for 段 in 模块路径 {
                文件.push(段);
            }
            文件.set_extension("智");
            if 文件.is_file() {
                return Ok(模块定位 {
                    文件,
                    层级: 模块层级::项目内,
                    相对段: 模块路径.to_vec(),
                });
            }
        }
        Err(format!("无法找到模块【{}】", 模块路径.join("/")))
    }

    /// 文件所属的**库源码根**（`None` = 不属于任何依赖库 ⇒ 项目/单文件作用域）。
    /// 用途：库 = **独立解析作用域** —— 库内文件的项目内导入以其本库 `源码/` 根为基准（规范 §2.7.5
    /// 「项目内使用基于 `源码/` 的绝对路径」对库同样适用），因此库内模块互导**无需**写库名前缀。
    pub fn 库归属(&self, 路径: &Path) -> Option<&PathBuf> {
        self.库源码映射
            .iter()
            .map(|(_, _, 根, _)| 根)
            .find(|根| 路径.starts_with(根))
    }
}

fn 加入路径(列表: &mut Vec<PathBuf>, 已见: &mut std::collections::HashSet<PathBuf>, 路径: &Path) {
    if 路径.exists() {
        let 规范 = 路径.canonicalize().unwrap_or_else(|_| 路径.to_path_buf());
        if 已见.insert(规范.clone()) {
            列表.push(规范);
        }
    }
}

/// 从起始路径向上查找包含 配置/项目.配置 的项目根（双模式共用）
pub fn 检测项目根(起始路径: &Path) -> Option<PathBuf> {
    let mut 当前 = if 起始路径.is_file() {
        起始路径.parent()?.to_path_buf()
    } else {
        起始路径.to_path_buf()
    };
    loop {
        if 当前.join("配置").join("项目.配置").is_file() {
            return Some(当前);
        }
        if !当前.pop() {
            return None;
        }
    }
}

/// 构建模块解析方案（读取 项目.配置 声明 + 项目依赖.配置 锁）
/// 两级映射：代号（项目.配置 表头）→ 原始名（锁文件表头）→ 路径（锁文件路径字段）
/// 智语言库进入 库源码映射；外部语言/远程 条目进入 外部语言映射（不参与导入解析）。
/// 支持多版本共存：Vec<(原始名, 版本, PathBuf, 依赖类型)>
/// 标准库根双模式同目录（标准库/ 仅 .智 源码一份，2026-08-03 定稿），导入语义一致。
pub fn 构建模块解析方案(项目根: &Path, 编译模式: bool) -> Result<模块解析方案, String> {
    let 标准库根 = 目录::标准库根(编译模式);
    let 标准库源码根 = 目录::标准库源码根();
    let 项目源码根 = 项目根.join("源码");
    let mut 库映射: Vec<(String, String, PathBuf, 依赖类型)> = Vec::new();
    let mut 解析顺序: Vec<String> = Vec::new();
    let mut 外部语言映射: Vec<外部语言条目> = Vec::new();

    // 1. 解析 项目.配置 依赖声明（代号 → 声明），并建立 代号 → 原始名 映射
    let (mut 声明映射, 代号映射) = 解析依赖声明(项目根);
    // 1.1 合并直接库依赖（如 图形界面 库）所声明的传递外部语言/远程条目，
    //     使消费方无需直接声明（如 GTK4），符合「库打包外部语言、消费方只依赖库」的传递依赖模型
    //     （与 npm/Cargo/Maven 一致：消费方只列直接依赖，库的内部依赖由库自身声明）。
    let mut 传递声明 = HashMap::new();
    收集传递外部语言声明(项目根, &mut 传递声明);
    for (原始名, 声明项) in 传递声明 {
        if !声明映射.values().any(|d| d.原始名 == 原始名) {
            声明映射.insert(原始名.clone(), 声明项);
        }
    }

    // 2. 读取 项目依赖.配置 锁（表头 = 原始名）
    let 锁路径 = 项目根.join("配置").join("项目依赖.配置");
    if let Ok(内容) = std::fs::read_to_string(&锁路径) {
        let 文档 = 配置读取器::解析配置(&内容);
        for 段 in &文档.段 {
            let 原始名 = &段.段名;
            let 版本 = 配置读取器::取文本(段, "版本").unwrap_or_else(|| "?".into());
            let 路径串 = 配置读取器::取文本(段, "路径").unwrap_or_default().trim().to_string();
            let 源地址 = 配置读取器::取文本(段, "源地址").unwrap_or_default();

            // 判断声明类型：取引用该原始名的第一个声明（代号可能多个指向同一原始名）
            let 该声明 = 声明映射.values().find(|d| d.原始名 == *原始名);
            let 类型 = 该声明.map_or(声明类型::智语言库, |d| d.类型);

            match 类型 {
                声明类型::外部语言 | 声明类型::远程 => {
                    // 外部语言/远程：不进智源库映射，收集到外部语言映射
                    // 补全路径（可能为空 → None；远程条目通常无本地路径）
                    let 绝对 = if 路径串.is_empty() {
                        None
                    } else {
                        let p = 补全依赖路径(&路径串, 项目根);
                        Some(p)
                    };
                    // 代号 = 引用该原始名的声明代号（多个时取第一个）
                    let 代号 = 声明映射.iter()
                        .find(|(_, d)| d.原始名 == *原始名)
                        .map(|(代号, _)| 代号.clone())
                        .unwrap_or_else(|| 原始名.clone());
                    // 管理方式 只读 项目.配置 声明（锁文件不记录该字段，规范 6.2.3）
                    let 管理方式 = 该声明.map_or("自动".into(), |d| d.管理方式.clone());
                    外部语言映射.push(外部语言条目 {
                        代号,
                        原始名: 原始名.clone(),
                        路径: 绝对,
                        版本: 版本.clone(),
                        管理方式,
                        源地址: 源地址.clone(),
                    });
                }
                声明类型::智语言库 => {
                    if 路径串.is_empty() {
                        // 锁条目无路径（如旧格式）→ 跳过，警告
                        eprintln!("[智语言] 警告：依赖库 {} 锁条目缺少路径字段", 原始名);
                        continue;
                    }
                    let 绝对 = 补全依赖路径(&路径串, 项目根);
                    if !绝对.exists() {
                        eprintln!("[智语言] 警告：依赖库路径不存在: {}（请重新安装库 {}）", 绝对.display(), 原始名);
                        continue;
                    }
                    if let Some(根) = 定位库源码根(&绝对) {
                        // 依赖类型（运行/开发/构建）从声明所属段推导
                        let 段类型 = 声明映射.values()
                            .find(|d| d.原始名 == *原始名)
                            .map(|d| d.段类型)
                            .unwrap_or(依赖类型::运行);
                        库映射.push((原始名.clone(), 版本, 根, 段类型));
                        解析顺序.push(原始名.clone());
                    }
                }
            }
        }
    }

    // ★ 2026-10-22 多版本共存**择优**（真实缺陷修复）：
    //   锁文件允许同一库多个版本段（历史安装 + 传递依赖多版本共存），
    //   但同一项目对同一库的 `导入` 解析必须**唯一**且是**最佳已装版本**；
    //   否则 `库 更新` 装上了新版本，运行时仍按锁文件第一条（最旧）加载旧代码 ⇒ 更新对用户无效。
    //   规则：按 `项目.配置` 声明约束选最佳；无约束取最高版本；版本不可解析（非三段数字）则保留首个。
    {
        let mut 按库分组: HashMap<String, Vec<usize>> = HashMap::new();
        for (下标, 条目) in 库映射.iter().enumerate() {
            按库分组.entry(条目.0.clone()).or_default().push(下标);
        }
        if 按库分组.values().any(|v| v.len() > 1) {
            let mut 保留: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for (原始名, 下标集) in &按库分组 {
                if 下标集.len() == 1 {
                    保留.insert(下标集[0]);
                    continue;
                }
                let 约束 = 声明映射
                    .values()
                    .find(|d| &d.原始名 == 原始名)
                    .and_then(|d| d.版本约束.clone())
                    .unwrap_or_default();
                let 版本列表: Vec<String> = 下标集.iter().map(|&i| 库映射[i].1.clone()).collect();
                let 选定 = crate::版本号::选择最佳版本(&版本列表, &约束)
                    .ok()
                    .or_else(|| {
                        版本列表
                            .iter()
                            .max_by(|a, b| crate::版本号::比较版本(a, b))
                            .cloned()
                    })
                    .unwrap_or_default();
                match 下标集.iter().find(|&&i| 库映射[i].1 == 选定) {
                    Some(&i) => {
                        保留.insert(i);
                        eprintln!(
                            "[智语言] 依赖库 {} 多版本共存 ⇒ 使用 {}（声明约束：{}）",
                            原始名,
                            选定,
                            if 约束.is_empty() { "无" } else { 约束.as_str() }
                        );
                    }
                    None => { 保留.insert(下标集[0]); }
                }
            }
            库映射 = 库映射
                .into_iter()
                .enumerate()
                .filter(|(i, _)| 保留.contains(i))
                .map(|(_, 条目)| 条目)
                .collect();
        }
    }

    Ok(模块解析方案 {
        库源码映射: 库映射,
        解析顺序,
        标准库根,
        标准库源码根,
        项目源码根,
        代号映射,
        外部语言映射,
        声明映射,
    })
}

/// 解析 项目.配置 中的 [运行依赖]/[开发依赖]/[构建依赖] 三个段
/// 返回 (声明映射: 代号 → 依赖声明, 代号映射: 代号 → 原始名)
/// 依赖值支持两种形式：
///   旧形式（兼容）：`库名 = ">= 1.0.0"`
///   新形式（规范 6.2.2）：`代号 = { 类型 = "...", 原始名 = "...", 版本 = "...", 管理方式 = "..." }`
pub fn 解析依赖声明(项目根: &Path) -> (HashMap<String, 依赖声明>, HashMap<String, String>) {
    let mut 声明映射 = HashMap::new();
    let mut 代号映射 = HashMap::new();
    let 配置路径 = 项目根.join("配置").join("项目.配置");
    let 文档 = match 配置读取器::解析文件(&配置路径) {
        Ok(d) => d,
        Err(_) => return (声明映射, 代号映射),
    };

    // §6.5.4：外部语言依赖既可用通用段 `[运行依赖]+类型="外部语言"`，
    // 也可用专名段 `[外部语言运行依赖]`/`[外部语言构建依赖]`/`[外部语言开发依赖]`（语义等价，缺省即外部语言）。
    let 段映射 = [
        ("运行依赖", 依赖类型::运行, false),
        ("开发依赖", 依赖类型::开发, false),
        ("构建依赖", 依赖类型::构建, false),
        ("外部语言运行依赖", 依赖类型::运行, true),
        ("外部语言开发依赖", 依赖类型::开发, true),
        ("外部语言构建依赖", 依赖类型::构建, true),
    ];

    for (段名, 类型, 强制外部语言) in &段映射 {
        if let Some(段) = 配置读取器::取段(&文档, 段名) {
            for (键, 值) in &段.键值 {
                let (声明, 原始名) = match 值 {
                    配置读取器::配置值::表(条目) => {
                        let 条目类型 = match 配置读取器::表取文本(条目, "类型").as_deref() {
                            Some("外部语言") => 声明类型::外部语言,
                            Some("远程") => 声明类型::远程,
                            _ => if *强制外部语言 { 声明类型::外部语言 } else { 声明类型::智语言库 },
                        };
                        let 原始名 = 配置读取器::表取文本(条目, "原始名").unwrap_or_else(|| 键.clone());
                        let 版本约束 = 配置读取器::表取文本(条目, "版本");
                        let 管理方式 = 配置读取器::表取文本(条目, "管理方式").unwrap_or_else(|| "自动".into());
                        (
                            依赖声明 {
                                类型: 条目类型,
                                原始名: 原始名.clone(),
                                版本约束,
                                管理方式,
                                段类型: *类型,
                            },
                            原始名,
                        )
                    }
                    配置读取器::配置值::文本(v) => {
                        // 旧形式：`库名 = "版本约束"`
                        let 版本约束 = if v.is_empty() || v == "*" { None } else { Some(v.clone()) };
                        (
                            依赖声明 {
                                类型: if *强制外部语言 { 声明类型::外部语言 } else { 声明类型::智语言库 },
                                原始名: 键.clone(),
                                版本约束,
                                管理方式: "自动".into(),
                                段类型: *类型,
                            },
                            键.clone(),
                        )
                    }
                    _ => continue,
                };
                声明映射.insert(键.clone(), 声明);
                代号映射.insert(键.clone(), 原始名);
            }
        }
    }
    (声明映射, 代号映射)
}

/// 收集直接库依赖声明的传递外部语言/远程条目（一层）。
/// 消费方锁中的 智语言库 条目指向已安装的库根，读取其 项目.配置 声明，
/// 把其中的 外部语言/远程 条目汇入 汇总（按 原始名 去重），供消费方免声明解析 GTK 等传递依赖。
/// 仅收集一层（库直接捆绑的外部语言），不递归下级 智语言库（避免路径基准歧义）。
fn 收集传递外部语言声明(项目根: &Path, 汇总: &mut HashMap<String, 依赖声明>) {
    let 锁路径 = 项目根.join("配置").join("项目依赖.配置");
    let 内容 = match std::fs::read_to_string(&锁路径) {
        Ok(c) => c,
        Err(_) => return,
    };
    let 文档 = 配置读取器::解析配置(&内容);
    for 段 in &文档.段 {
        let 路径串 = 配置读取器::取文本(段, "路径").unwrap_or_default();
        if 路径串.is_empty() {
            continue;
        }
        let 绝对 = 补全依赖路径(&路径串, 项目根);
        // 库根 = 路径剥离 /源码/
        let 库根 = match 绝对.parent() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        if !库根.join("配置").join("项目.配置").is_file() {
            continue;
        }
        let (声明, _) = 解析依赖声明(&库根);
        for (_, 声明项) in 声明 {
            if 声明项.类型 == 声明类型::外部语言 || 声明项.类型 == 声明类型::远程 {
                汇总.entry(声明项.原始名.clone()).or_insert(声明项);
            }
        }
    }
}

/// 路径补全（修正文档 6.2.3 路径写入表 bug）
fn 补全依赖路径(路径串: &str, 项目根: &Path) -> PathBuf {
    if 路径串.starts_with(".智语言/") {
        // 全局级：去掉前缀后拼到全局目录
        目录::全局目录().join(路径串.trim_start_matches(".智语言/").trim_start_matches('/'))
    } else if 路径串.starts_with('/') {
        PathBuf::from(路径串)
    } else {
        // 项目级：相对项目目录
        项目根.join(路径串)
    }
}

/// 定位库源码根：优先 <根>/源码/，否则 <根> 本身
fn 定位库源码根(根: &Path) -> Option<PathBuf> {
    if 根.join("源码").is_dir() {
        Some(根.join("源码"))
    } else if 根.is_dir() {
        Some(根.to_path_buf())
    } else {
        None
    }
}

/// 解析 项目.配置 [开发套件].开发套件版本 约束（规范 6.1.8 项目版本锁定）
/// 返回形如 ">=0.0.1" / "0.0.1" / "^1.0.0" 的版本约束字符串；无则 None。
pub fn 解析开发套件版本约束(项目根: &Path) -> Option<String> {
    let 路径 = 项目根.join("配置").join("项目.配置");
    let 文档 = 配置读取器::解析文件(&路径).ok()?;
    配置读取器::取段(&文档, "开发套件")
        .and_then(|段| 配置读取器::取文本(段, "开发套件版本"))
}

#[cfg(test)]
mod 测试 {
    use super::*;

    fn 临时项目(配置文本: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        配置文本.hash(&mut h);
        let 目录 = std::env::temp_dir().join(format!("智语言_依赖解析_测试_{}_{}", std::process::id(), h.finish()));
        let _ = std::fs::remove_dir_all(&目录);
        std::fs::create_dir_all(&目录.join("配置")).unwrap();
        std::fs::create_dir_all(&目录.join("源码")).unwrap();
        std::fs::write(目录.join("配置").join("项目.配置"), 配置文本).unwrap();
        目录
    }

    #[test]
    fn 对象格式_智语言库_默认类型() {
        let 根 = 临时项目("[运行依赖]\n数学 = { 版本 = \">= 1.0.0\" }\n");
        let (声明映射, 代号映射) = 解析依赖声明(&根);
        let 声明 = 声明映射.get("数学").unwrap();
        assert_eq!(声明.类型, 声明类型::智语言库);
        assert_eq!(声明.原始名, "数学");
        assert_eq!(声明.版本约束.as_deref(), Some(">= 1.0.0"));
        assert_eq!(声明.段类型, 依赖类型::运行);
        assert_eq!(代号映射.get("数学").map(|s| s.as_str()), Some("数学"));
        let _ = std::fs::remove_dir_all(&根);
    }

    #[test]
    fn 对象格式_外部语言_与_远程() {
        let 根 = 临时项目(
            "[运行依赖]\n\
             数据库 = { 原始名 = \"sqlite3\", 版本 = \"3.42.0\", 管理方式 = \"手动\", 类型 = \"外部语言\" }\n\
             计算 = { 原始名 = \"数学\", 管理方式 = \"自动\", 类型 = \"远程\" }\n",
        );
        let (声明映射, 代号映射) = 解析依赖声明(&根);
        let 数据库 = 声明映射.get("数据库").unwrap();
        assert_eq!(数据库.类型, 声明类型::外部语言);
        assert_eq!(数据库.原始名, "sqlite3");
        assert_eq!(数据库.管理方式, "手动");
        let 计算 = 声明映射.get("计算").unwrap();
        assert_eq!(计算.类型, 声明类型::远程);
        assert_eq!(计算.原始名, "数学");
        assert_eq!(计算.管理方式, "自动");
        // 代号 → 原始名 两级映射
        assert_eq!(代号映射.get("数据库").map(|s| s.as_str()), Some("sqlite3"));
        assert_eq!(代号映射.get("计算").map(|s| s.as_str()), Some("数学"));
        let _ = std::fs::remove_dir_all(&根);
    }

    #[test]
    fn 对象格式_开发构建段类型() {
        let 根 = 临时项目(
            "[开发依赖]\n测试工具 = { 原始名 = \"工具\", 版本 = \"1.0.0\" }\n\
             [构建依赖]\n编译器 = { 版本 = \"2.0.0\" }\n",
        );
        let (声明映射, _) = 解析依赖声明(&根);
        assert_eq!(声明映射.get("测试工具").unwrap().段类型, 依赖类型::开发);
        assert_eq!(声明映射.get("编译器").unwrap().段类型, 依赖类型::构建);
        let _ = std::fs::remove_dir_all(&根);
    }

    #[test]
    fn 旧文本格式_兼容() {
        let 根 = 临时项目("[运行依赖]\n数学 = \">= 1.0.0\"\n");
        let (声明映射, _) = 解析依赖声明(&根);
        let 声明 = 声明映射.get("数学").unwrap();
        assert_eq!(声明.类型, 声明类型::智语言库);
        assert_eq!(声明.原始名, "数学");
        assert_eq!(声明.版本约束.as_deref(), Some(">= 1.0.0"));
        let _ = std::fs::remove_dir_all(&根);
    }

    #[test]
    fn 构建方案_两级映射_外部语言排除() {
        // 构造完整项目：声明 + 锁文件 + 实际库目录
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        "两级映射".hash(&mut h);
        let 根 = std::env::temp_dir().join(format!("智语言_构建方案_测试_{}_{}", std::process::id(), h.finish()));
        let _ = std::fs::remove_dir_all(&根);
        std::fs::create_dir_all(&根.join("配置")).unwrap();
        std::fs::create_dir_all(&根.join("源码")).unwrap();
        // 智语言库：代号 我的数学 → 原始名 数学
        std::fs::create_dir_all(&根.join("库/数学1.0.0/源码")).unwrap();
        // 外部语言库：代码代号 数据库 → 原始名 sqlite3（手动）
        std::fs::create_dir_all(&根.join("外部语言/sqlite3-3.42.0")).unwrap();
        std::fs::write(根.join("外部语言/sqlite3-3.42.0/sqlite3.lib"), b"x").unwrap();

        std::fs::write(
            根.join("配置").join("项目.配置"),
            "[运行依赖]\n\
             我的数学 = { 原始名 = \"数学\", 版本 = \">= 1.0.0\" }\n\
             数据库 = { 原始名 = \"sqlite3\", 版本 = \"3.42.0\", 管理方式 = \"手动\", 类型 = \"外部语言\" }\n",
        ).unwrap();
        std::fs::write(
            根.join("配置").join("项目依赖.配置"),
            "[数学]\n版本 = \"1.0.0\"\n路径 = \"库/数学1.0.0/\"\n\n\
             [sqlite3]\n版本 = \"3.42.0\"\n路径 = \"外部语言/sqlite3-3.42.0/\"\n",
        ).unwrap();

        let 方案 = 构建模块解析方案(&根, true).unwrap();
        // 智语言库：代号 我的数学 解析到 原始名 数学
        assert_eq!(方案.解析原始名("我的数学"), "数学");
        let 源码根 = 方案.查询库源码根("我的数学").expect("我的数学应解析到库");
        assert!(源码根.ends_with("库/数学1.0.0/源码"));
        // 外部语言排除出 库源码映射
        assert!(方案.查询库源码根("数据库").is_none());
        // 外部语言映射：代号/原始名/路径；管理方式 从 项目.配置 声明取（规范 6.2.3，锁文件不记录）
        let 外部条目 = 方案.查询外部语言("数据库");
        assert_eq!(外部条目.len(), 1);
        assert_eq!(外部条目[0].原始名, "sqlite3");
        assert_eq!(外部条目[0].代号, "数据库");
        assert_eq!(外部条目[0].管理方式, "手动");
        assert!(外部条目[0].路径.as_ref().unwrap().ends_with("外部语言/sqlite3-3.42.0"));
        let _ = std::fs::remove_dir_all(&根);
    }

    #[test]
    fn 构建方案_传递外部语言_消费方免声明() {
        // 消费方只依赖 图形界面 库（智语言库），GTK4 由该库打包（传递外部语言）。
        // 验证：消费方 项目.配置 不声明 GTK，GTK 仍经库声明解析进 外部语言映射。
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        "传递外部语言".hash(&mut h);
        let 根 = std::env::temp_dir().join(format!("智语言_传递外部语言_测试_{}_{}", std::process::id(), h.finish()));
        let _ = std::fs::remove_dir_all(&根);
        std::fs::create_dir_all(&根.join("配置")).unwrap();
        std::fs::create_dir_all(&根.join("源码")).unwrap();
        // 直接库依赖（已安装）：图形界面库（其 项目.配置 声明外部语言 gtk4）
        std::fs::create_dir_all(&根.join("库/图形界面库1.0.0/源码")).unwrap();
        std::fs::create_dir_all(&根.join("库/图形界面库1.0.0/配置")).unwrap();
        std::fs::create_dir_all(&根.join("库/图形界面库1.0.0/外部语言/GTK4")).unwrap();
        std::fs::write(
            根.join("库/图形界面库1.0.0/配置").join("项目.配置"),
            "[运行依赖]\n\
             gtk4 = { 原始名 = \"gtk4\", 版本 = \"4.0.0\", 管理方式 = \"手动\", 类型 = \"外部语言\" }\n",
        ).unwrap();
        // 消费方：只声明 图形界面库（智语言库），不声明 GTK
        std::fs::write(
            根.join("配置").join("项目.配置"),
            "[运行依赖]\n\
             图形界面库 = { 版本 = \">= 1.0.0\" }\n",
        ).unwrap();
        // 消费方锁：含 图形界面库（智语言库）+ gtk4（传递外部语言，路径指向库内）
        std::fs::write(
            根.join("配置").join("项目依赖.配置"),
            "[gtk4]\n版本 = \"4.0.0\"\n路径 = \"库/图形界面库1.0.0/外部语言/GTK4/\"\n哈希 = \"\"\n\
             [图形界面库]\n版本 = \"1.0.0\"\n路径 = \"库/图形界面库1.0.0/源码/\"\n哈希 = \"\"\n",
        ).unwrap();

        let 方案 = 构建模块解析方案(&根, true).unwrap();
        // 图形界面库 应解析进 库源码映射
        assert!(方案.查询库源码根("图形界面库").is_some());
        // GTK4 不应进 库源码映射（它是外部语言）
        assert!(方案.查询库源码根("gtk4").is_none());
        // 关键：消费方未声明 GTK，但经 图形界面 库声明解析，GTK 进入 外部语言映射
        let 外部条目 = 方案.查询外部语言("gtk4");
        assert_eq!(外部条目.len(), 1, "传递外部语言 gtk4 应解析进 外部语言映射");
        assert_eq!(外部条目[0].原始名, "gtk4");
        assert_eq!(外部条目[0].管理方式, "手动");
        assert!(外部条目[0].路径.as_ref().unwrap().ends_with("库/图形界面库1.0.0/外部语言/GTK4"));
        let _ = std::fs::remove_dir_all(&根);
    }
}
