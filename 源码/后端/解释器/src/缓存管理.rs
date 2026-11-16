// 文件: 后端/解释器/src/缓存管理.rs
// 职责: 基于内容哈希的字节码缓存，支持内存和磁盘双层缓存
// 缓存策略: 对源文件内容计算哈希(SipHash-1-3)，任何内容变化都会导致哈希变化
// 优势: 不依赖文件时间戳/大小，避免了同一秒修改漏检问题

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use pm::目录;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct 缓存键 {
    pub 源文件路径: String,
    pub 源文件哈希: String,
    pub 工具链版本: String,
}

pub struct 缓存管理器 {
    pub 缓存目录: PathBuf,
    pub 内存缓存: HashMap<缓存键, Vec<u8>>,
    pub 最大内存缓存数: usize,
}

impl 缓存管理器 {
    /// 创建系统全局缓存管理器（路径：全局目录/产出/字节码文件/）
    pub fn 新建() -> Self {
        let 缓存目录 = Self::全局缓存目录().join("产出").join("字节码文件");
        fs::create_dir_all(&缓存目录).ok();
        缓存管理器 {
            缓存目录,
            内存缓存: HashMap::new(),
            最大内存缓存数: 32,
        }
    }

    /// 创建项目级缓存管理器（产物在项目根/产出/字节码文件/）
    pub fn 新建_项目级(项目根目录: PathBuf) -> Self {
        let 缓存目录 = 项目根目录.join("产出").join("字节码文件");
        fs::create_dir_all(&缓存目录).ok();
        缓存管理器 {
            缓存目录,
            内存缓存: HashMap::new(),
            最大内存缓存数: 32,
        }
    }

    /// 系统全局目录（统一真相源，规范 6.1.2）
    fn 全局缓存目录() -> PathBuf {
        目录::全局目录()
    }

    /// 计算源文件的内容哈希（基于SipHash-1-3）
    /// 任何内容变化(改值/加注释/加空格/同大小替换)都会产生不同的哈希
    /// 不依赖文件元数据(大小/修改时间)，避免了文件系统时间精度问题
    pub fn 计算文件哈希(路径: &str) -> Option<String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let 内容 = fs::read(路径).ok()?;
        let mut 哈希器 = DefaultHasher::new();
        内容.hash(&mut 哈希器);
        Some(format!("{:x}", 哈希器.finish()))
    }

    /// 计算参与编译的全部源文件的组合内容哈希（精准增量构建签名）
    ///
    /// 覆盖范围（任一文件内容变化都导致哈希变化，从而缓存失效）：
    /// - 入口源文件本身
    /// - 单文件模式：入口递归导入的所有非内置 `.智` 模块（标准库/项目/依赖库）
    /// - 项目模式：项目 `源码/` 目录全部 `.智` 文件 + 全部依赖库 `.智` 源文件
    ///
    /// 与 `计算文件哈希` 仅哈希入口单文件不同，本函数能精准识别
    /// 「被导入模块 / 依赖库」的改动，避免命中陈旧缓存（见 重点测试点）。
    pub fn 计算源集合哈希(入口路径: &str, 编译模式: bool) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let 路径 = Path::new(入口路径);
        let mut 文件集合: HashSet<String> = HashSet::new();
        文件集合.insert(入口路径.to_string());

        if let Some(项目根) = pm::依赖解析::检测项目根(路径) {
            // 项目模式：收集 源码/ 全部 .智 + 依赖库全部 .智
            递归收集智文件(&项目根.join("源码"), &mut 文件集合);
            if let Ok(方案) = pm::依赖解析::构建模块解析方案(&项目根, 编译模式) {
                for (_, _, 库源码根, _) in &方案.库源码映射 {
                    递归收集智文件(库源码根, &mut 文件集合);
                }
            }
        } else {
            // 单文件模式：解析入口导入，递归收集被导入模块
            收集导入模块源文件(路径, 编译模式, &mut 文件集合);
        }

        // 组合哈希：按路径排序保证确定性
        let mut 文件列表: Vec<String> = 文件集合.into_iter().collect();
        文件列表.sort();
        let mut 哈希器 = DefaultHasher::new();
        for 文件 in &文件列表 {
            if let Ok(内容) = fs::read(文件) {
                文件.hash(&mut 哈希器);
                内容.hash(&mut 哈希器);
            }
        }
        format!("{:x}", 哈希器.finish())
    }

    /// [已废弃] 基于文件大小+修改时间秒的元数据哈希
    /// 保留此函数用于向后兼容和参考，但不建议新代码使用
    /// 问题: 同一秒内修改大小不变的文件会产生相同哈希，导致缓存漏检
    pub fn 计算元数据哈希(路径: &str) -> Option<String> {
        let 元数据 = fs::metadata(路径).ok()?;
        let 大小 = 元数据.len();
        let 修改时间 = 元数据
            .modified()
            .unwrap_or(SystemTime::now())
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Some(format!("{:x}{:x}", 大小, 修改时间))
    }

    pub fn 查找缓存(&mut self, 键: &缓存键) -> Option<Vec<u8>> {
        if let Some(字节码) = self.内存缓存.get(键) {
            return Some(字节码.clone());
        }
        let 文件路径 = self.缓存文件路径(键);
        if 文件路径.exists() {
            if let Ok(数据) = fs::read(&文件路径) {
                if 数据.len() >= 4 && &数据[0..4] == &0x43534C42u32.to_le_bytes() {
                    self.添加到内存缓存(键.clone(), 数据.clone());
                    return Some(数据);
                }
            }
        }
        None
    }

    pub fn 存储缓存(&mut self, 键: &缓存键, 字节码: Vec<u8>) {
        let 文件路径 = self.缓存文件路径(键);
        if let Some(父目录) = 文件路径.parent() {
            fs::create_dir_all(父目录).ok();
        }
        fs::write(&文件路径, &字节码).ok();
        self.添加到内存缓存(键.clone(), 字节码);
    }

    fn 添加到内存缓存(&mut self, 键: 缓存键, 字节码: Vec<u8>) {
        if self.内存缓存.len() >= self.最大内存缓存数 {
            let 第一个键 = self.内存缓存.keys().next().cloned();
            if let Some(k) = 第一个键 {
                self.内存缓存.remove(&k);
            }
        }
        self.内存缓存.insert(键, 字节码);
    }

    pub fn 失效缓存(&mut self, 源文件路径: &str) {
        let 待删除: Vec<缓存键> = self
            .内存缓存
            .keys()
            .filter(|k| k.源文件路径 == 源文件路径)
            .cloned()
            .collect();
        for 键 in 待删除 {
            self.内存缓存.remove(&键);
            let 文件路径 = self.缓存文件路径(&键);
            fs::remove_file(文件路径).ok();
        }
    }

    pub fn 清空所有缓存(&mut self) {
        self.内存缓存.clear();
        if self.缓存目录.exists() {
            fs::remove_dir_all(&self.缓存目录).ok();
            fs::create_dir_all(&self.缓存目录).ok();
        }
    }

    pub fn 缓存统计(&self) -> (usize, usize) {
        let 磁盘文件数 = if self.缓存目录.exists() {
            fs::read_dir(&self.缓存目录)
                .map(|d| d.count())
                .unwrap_or(0)
        } else {
            0
        };
        (self.内存缓存.len(), 磁盘文件数)
    }

    fn 缓存文件路径(&self, 键: &缓存键) -> PathBuf {
        let 文件名 = format!(
            "{}_{}.字节码",
            &键.源文件哈希[..8.min(键.源文件哈希.len())],
            键.工具链版本
        );
        self.缓存目录.join(文件名)
    }
}

/// 递归收集目录下所有 `.智` 源文件（含子目录）
fn 递归收集智文件(目录: &Path, 集合: &mut HashSet<String>) {
    if let Ok(条目) = fs::read_dir(目录) {
        for 项 in 条目.flatten() {
            let 路径 = 项.path();
            if 路径.is_dir() {
                递归收集智文件(&路径, 集合);
            } else if 路径.extension().and_then(|e| e.to_str()) == Some("智") {
                集合.insert(路径.to_string_lossy().to_string());
            }
        }
    }
}

/// 解析入口文件，递归收集其导入的所有非内置 `.智` 模块源文件
fn 收集导入模块源文件(入口路径: &Path, 编译模式: bool, 集合: &mut HashSet<String>) {
    let 源码 = match fs::read_to_string(入口路径) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut 诊断 = frontend::诊断收集器::诊断收集器::新建();
    let 词条 = frontend::词法分析::词法分析器::新建(
        &源码,
        入口路径.to_string_lossy().to_string(),
        &mut 诊断,
    )
    .扫描所有词条();
    if 诊断.有错误() {
        return;
    }
    let mut 语法 = frontend::语法分析::语法分析器::新建(&词条, &mut 诊断);
    let mut 程序 = 语法.解析程序();
    if 诊断.有错误() {
        return;
    }

    // 规范 §2.7.5 三层分派：项目内文件可写 `导入 库名/模块路径`，故缓存收集同样需要依赖解析方案
    let 解析方案 = pm::依赖解析::检测项目根(入口路径)
        .and_then(|项目根| pm::依赖解析::构建模块解析方案(&项目根, 编译模式).ok());
    let mut 加载器 = match 解析方案 {
        Some(方案) => frontend::模块加载器::模块加载器::新建_带方案(
            方案,
            frontend::模块加载器::模块加载器::默认加载路径(编译模式),
        ),
        None => frontend::模块加载器::模块加载器::新建(
            frontend::模块加载器::模块加载器::默认加载路径(编译模式),
        ),
    };
    let 入口目录 = 入口路径.parent();
    let 闭包 = &mut |源码: &str, 文件路径: &Path| -> Result<frontend::语法分析::程序, String> {
        let mut 临时诊断 = frontend::诊断收集器::诊断收集器::新建();
        let 词条 = frontend::词法分析::词法分析器::新建(
            源码,
            文件路径.to_string_lossy().to_string(),
            &mut 临时诊断,
        )
        .扫描所有词条();
        if 临时诊断.有错误() {
            return Err("词法分析失败".to_string());
        }
        let mut g = frontend::语法分析::语法分析器::新建(&词条, &mut 临时诊断);
        let 程序 = g.解析程序();
        if 临时诊断.有错误() {
            return Err("语法分析失败".to_string());
        }
        Ok(程序)
    };
    if 加载器
        .加载依赖模块(&mut 程序.导入列表, 入口目录, 闭包, &mut std::collections::HashSet::new())
        .is_ok()
    {
        for 数据 in 加载器.已加载缓存.values() {
            集合.insert(数据.文件路径.clone());
        }
    }
}