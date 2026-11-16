// 图形安装向导（Windows 原生 Win32 控件；零第三方 GUI 依赖，保持"纯 Rust + 系统 API"）
//
// 交互：一个窗口 = 选目录（可浏览）→ 点「安装」→ 进度条 → 「完成」。
// 线程模型：界面线程只跑消息循环；解压放到工作线程，用 PostMessage(WM_APP+n) 回推进度，
//          避免 195MB 解压把窗口卡成"未响应"。
//
// 只在双击（无参数）时启用；带 --目标/--静默 时仍走控制台的静默路径（CI 用）。
#![allow(non_snake_case)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

// ============ Win32 基础类型 ============
type HWND = isize;
type HINSTANCE = isize;
type LPCWSTR = *const u16;
type WPARAM = usize;
type LPARAM = isize;
type LRESULT = isize;
type UINT = u32;
type BOOL = i32;

#[repr(C)]
struct 矩形 {
    左: i32,
    上: i32,
    右: i32,
    下: i32,
}

#[repr(C)]
struct 消息 {
    窗口: HWND,
    类型: UINT,
    w: WPARAM,
    l: LPARAM,
    时间: u32,
    点x: i32,
    点y: i32,
}

#[repr(C)]
struct 窗口类 {
    样式: UINT,
    过程: Option<unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>,
    类附加: i32,
    窗附加: i32,
    实例: HINSTANCE,
    图标: isize,
    光标: isize,
    背景: isize,
    菜单名: LPCWSTR,
    类名: LPCWSTR,
}

#[repr(C)]
struct 公共控件信息 {
    大小: u32,
    标志: u32,
}

#[repr(C)]
struct 浏览信息 {
    属主: HWND,
    根: *mut u16,
    显示名: *mut u16,
    标题: LPCWSTR,
    标志: UINT,
    回调: isize,
    参数: LPARAM,
    图标索引: i32,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(名: LPCWSTR) -> HINSTANCE;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(类: *const 窗口类) -> u16;
    fn CreateWindowExW(
        扩展样式: u32,
        类名: LPCWSTR,
        标题: LPCWSTR,
        样式: u32,
        左: i32,
        上: i32,
        宽: i32,
        高: i32,
        父: HWND,
        菜单: isize,
        实例: HINSTANCE,
        参数: *mut core::ffi::c_void,
    ) -> HWND;
    fn DefWindowProcW(窗口: HWND, 类型: UINT, w: WPARAM, l: LPARAM) -> LRESULT;
    fn DestroyWindow(窗口: HWND) -> BOOL;
    fn ShowWindow(窗口: HWND, 命令: i32) -> BOOL;
    fn UpdateWindow(窗口: HWND) -> BOOL;
    fn GetMessageW(消息: *mut 消息, 窗口: HWND, 最小: UINT, 最大: UINT) -> BOOL;
    fn TranslateMessage(消息: *const 消息) -> BOOL;
    fn DispatchMessageW(消息: *const 消息) -> LRESULT;
    fn PostQuitMessage(码: i32);
    fn SendMessageW(窗口: HWND, 类型: UINT, w: WPARAM, l: LPARAM) -> LRESULT;
    fn PostMessageW(窗口: HWND, 类型: UINT, w: WPARAM, l: LPARAM) -> BOOL;
    fn SetWindowTextW(窗口: HWND, 文本: LPCWSTR) -> BOOL;
    fn GetWindowTextW(窗口: HWND, 缓冲: *mut u16, 长度: i32) -> i32;
    fn EnableWindow(窗口: HWND, 启用: BOOL) -> BOOL;
    fn AdjustWindowRect(矩形: *mut 矩形, 样式: u32, 有菜单: BOOL) -> BOOL;
    fn GetSystemMetrics(索引: i32) -> i32;
    fn MessageBoxW(窗口: HWND, 内容: LPCWSTR, 标题: LPCWSTR, 类型: UINT) -> i32;
    fn SetForegroundWindow(窗口: HWND) -> BOOL;
    fn LoadCursorW(实例: HINSTANCE, 名: LPCWSTR) -> isize;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetStockObject(对象: i32) -> isize;
}

#[link(name = "comctl32")]
extern "system" {
    fn InitCommonControlsEx(信息: *const 公共控件信息) -> BOOL;
}

#[link(name = "shell32")]
extern "system" {
    fn SHBrowseForFolderW(信息: *const 浏览信息) -> *mut u16;
    fn SHGetPathFromIDListW(标识: *const u16, 缓冲: *mut u16) -> BOOL;
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(保留: *mut core::ffi::c_void, 模式: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(指针: *mut core::ffi::c_void);
}

// ============ 常量 ============
const WM_CREATE: UINT = 0x0001;
const WM_DESTROY: UINT = 0x0002;
const WM_CLOSE: UINT = 0x0010;
const WM_COMMAND: UINT = 0x0111;
const WM_SETFONT: UINT = 0x0030;
const WM_APP: UINT = 0x8000;
const 消息_进度: UINT = WM_APP + 1;
const 消息_完成: UINT = WM_APP + 2;

const WS_CAPTION: u32 = 0x00C0_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const ES_AUTOHSCROLL: u32 = 0x0080;
const IDC_ARROW: usize = 32512;

const PBM_SETPOS: UINT = 0x0402;
const PBM_SETRANGE32: UINT = 0x0406;

const 默认字体: i32 = 17; // DEFAULT_GUI_FONT
const 背景_按钮面: isize = 16; // COLOR_BTNFACE + 1
const SW_SHOW: i32 = 5;
const ICC_PROGRESS_CLASS: u32 = 0x0000_0020;
const BIF_RETURNONLYFSDIRS: UINT = 0x0001;
const BIF_EDITBOX: UINT = 0x0010;
const BIF_NEWDIALOGSTYLE: UINT = 0x0040;
const MB_YESNO: UINT = 0x0000_0004;
const MB_ICONERROR: UINT = 0x0000_0010;
const MB_OK: UINT = 0x0000_0000;
const IDYES: i32 = 6;
const COINIT_APARTMENTTHREADED: u32 = 0x2;

// 控件 ID
const ID_说明: i32 = 100;
const ID_编辑: i32 = 101;
const ID_浏览: i32 = 102;
const ID_安装: i32 = 103;
const ID_取消: i32 = 104;
const ID_进度: i32 = 105;
const ID_状态: i32 = 106;

// 客户区尺寸与控件位置
const 客户宽: i32 = 560;
const 客户高: i32 = 236;

// ============ 状态 ============
struct 界面 {
    主窗: HWND,
    编辑框: HWND,
    浏览钮: HWND,
    安装钮: HWND,
    取消钮: HWND,
    进度条: HWND,
    状态栏: HWND,
    安装根: PathBuf,
    已装完: bool,
    安装中: bool,
    错误: Option<String>,
}

static 界面状态: OnceLock<Mutex<界面>> = OnceLock::new();
static 载荷: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
static 版本号: OnceLock<String> = OnceLock::new();

fn 界面() -> &'static Mutex<界面> {
    界面状态.get().expect("界面状态未初始化")
}

fn 载荷取() -> Arc<Vec<u8>> {
    载荷.get().expect("载荷未初始化").clone()
}

fn 版本取() -> String {
    版本号.get().expect("版本号未初始化").clone()
}

// ============ 对外入口 ============

/// 显示图形安装向导，装到用户选择的目录。成功 = 用户看到「安装完成」并关闭窗口。
pub fn 运行(数据: Arc<Vec<u8>>, 版本: String, 默认路径: PathBuf) -> Result<(), String> {
    let _ = 载荷.set(数据);
    let _ = 版本号.set(版本);
    let _ = 界面状态.set(Mutex::new(界面 {
        主窗: 0,
        编辑框: 0,
        浏览钮: 0,
        安装钮: 0,
        取消钮: 0,
        进度条: 0,
        状态栏: 0,
        安装根: 默认路径.clone(),
        已装完: false,
        安装中: false,
        错误: None,
    }));

    unsafe {
        CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let 实例 = GetModuleHandleW(std::ptr::null());
        let 类名 = wide("智语言安装向导");
        初始化公共控件();
        if RegisterClassW(&窗口类 {
            样式: 0,
            过程: Some(窗口过程),
            类附加: 0,
            窗附加: 0,
            实例,
            图标: 0,
            光标: LoadCursorW(0, IDC_ARROW as LPCWSTR),
            背景: 背景_按钮面,
            菜单名: std::ptr::null(),
            类名: 类名.as_ptr(),
        }) == 0
        {
            CoUninitialize();
            return Err("创建窗口类失败".to_string());
        }

        let 标题 = wide(&format!(
            "智语言开发套件 {} 安装程序",
            env!("CARGO_PKG_VERSION")
        ));
        let 样式 = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut 外框 = 矩形 {
            左: 0,
            上: 0,
            右: 客户宽,
            下: 客户高,
        };
        AdjustWindowRect(&mut 外框, 样式, 0);
        let 宽 = 外框.右 - 外框.左;
        let 高 = 外框.下 - 外框.上;
        let 屏宽 = GetSystemMetrics(0);
        let 屏高 = GetSystemMetrics(1);
        let 窗 = CreateWindowExW(
            0,
            类名.as_ptr(),
            标题.as_ptr(),
            样式,
            (屏宽 - 宽) / 2,
            (屏高 - 高) / 2,
            宽,
            高,
            0,
            0,
            实例,
            std::ptr::null_mut(),
        );
        if 窗 == 0 {
            CoUninitialize();
            return Err("创建安装窗口失败".to_string());
        }
        创建控件(窗, &默认路径.to_string_lossy());
        {
            let mut 状态 = 界面().lock().unwrap();
            状态.主窗 = 窗;
        }
        ShowWindow(窗, SW_SHOW);
        UpdateWindow(窗);
        SetForegroundWindow(窗);

        // 消息循环：装完（WM_DESTROY → PostQuitMessage）才退出
        let mut 消息: 消息 = std::mem::zeroed();
        while GetMessageW(&mut 消息, 0, 0, 0) > 0 {
            TranslateMessage(&消息);
            DispatchMessageW(&消息);
        }
        CoUninitialize();

        let 状态 = 界面().lock().unwrap();
        if let Some(错误) = &状态.错误 {
            return Err(错误.clone());
        }
        Ok(())
    }
}

pub fn 提示错误(内容: &str) {
    unsafe {
        MessageBoxW(
            0,
            wide(内容).as_ptr(),
            wide("智语言安装程序").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

// ============ 控件创建与布局 ============

unsafe fn 初始化公共控件() {
    let 信息 = 公共控件信息 {
        大小: std::mem::size_of::<公共控件信息>() as u32,
        标志: ICC_PROGRESS_CLASS,
    };
    InitCommonControlsEx(&信息);
}

unsafe fn 创建控件(主窗: HWND, 默认路径: &str) {
    let 字体 = GetStockObject(默认字体);
    let 控件样式 = WS_CHILD | WS_VISIBLE;

    let 说明 = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        wide("选择安装目录，然后点击「安装」。安装完成后，把 版本管理器 目录加入 PATH 即可在任意位置使用 智语言 命令。").as_ptr(),
        控件样式,
        16,
        16,
        528,
        38,
        主窗,
        ID_说明 as isize,
        0,
        std::ptr::null_mut(),
    );
    let 标签 = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        wide("安装目录：").as_ptr(),
        控件样式,
        16,
        62,
        80,
        20,
        主窗,
        0,
        0,
        std::ptr::null_mut(),
    );
    let 编辑 = CreateWindowExW(
        0,
        wide("EDIT").as_ptr(),
        wide(默认路径).as_ptr(),
        控件样式 | WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL,
        16,
        84,
        440,
        26,
        主窗,
        ID_编辑 as isize,
        0,
        std::ptr::null_mut(),
    );
    let 浏览 = CreateWindowExW(
        0,
        wide("BUTTON").as_ptr(),
        wide("浏览…").as_ptr(),
        控件样式 | WS_TABSTOP,
        464,
        84,
        80,
        26,
        主窗,
        ID_浏览 as isize,
        0,
        std::ptr::null_mut(),
    );
    let 进度 = CreateWindowExW(
        0,
        wide("msctls_progress32").as_ptr(),
        std::ptr::null(),
        控件样式,
        16,
        124,
        528,
        20,
        主窗,
        ID_进度 as isize,
        0,
        std::ptr::null_mut(),
    );
    let 状态 = CreateWindowExW(
        0,
        wide("STATIC").as_ptr(),
        wide("准备就绪").as_ptr(),
        控件样式,
        16,
        152,
        528,
        36,
        主窗,
        ID_状态 as isize,
        0,
        std::ptr::null_mut(),
    );
    let 安装 = CreateWindowExW(
        0,
        wide("BUTTON").as_ptr(),
        wide("安装").as_ptr(),
        控件样式 | WS_TABSTOP,
        364,
        192,
        86,
        30,
        主窗,
        ID_安装 as isize,
        0,
        std::ptr::null_mut(),
    );
    let 取消 = CreateWindowExW(
        0,
        wide("BUTTON").as_ptr(),
        wide("取消").as_ptr(),
        控件样式 | WS_TABSTOP,
        458,
        192,
        86,
        30,
        主窗,
        ID_取消 as isize,
        0,
        std::ptr::null_mut(),
    );

    for 控件 in [说明, 标签, 编辑, 浏览, 进度, 状态, 安装, 取消] {
        SendMessageW(控件, WM_SETFONT, 字体 as WPARAM, 1);
    }
    SendMessageW(进度, PBM_SETRANGE32, 0, 100);
    SendMessageW(进度, PBM_SETPOS, 0, 0);

    let mut 界面锁 = 界面().lock().unwrap();
    界面锁.编辑框 = 编辑;
    界面锁.浏览钮 = 浏览;
    界面锁.安装钮 = 安装;
    界面锁.取消钮 = 取消;
    界面锁.进度条 = 进度;
    界面锁.状态栏 = 状态;
}

unsafe fn 设置状态文字(文本: &str) {
    let 句柄 = 界面().lock().unwrap().状态栏;
    SetWindowTextW(句柄, wide(文本).as_ptr());
}

unsafe fn 设置进度(百分比: u32) {
    let 句柄 = 界面().lock().unwrap().进度条;
    SendMessageW(句柄, PBM_SETPOS, 百分比 as WPARAM, 0);
}

// ============ 窗口过程 ============

unsafe extern "system" fn 窗口过程(
    窗口: HWND,
    类型: UINT,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    match 类型 {
        WM_COMMAND => {
            let 命令 = (w & 0xFFFF) as i32;
            match 命令 {
                ID_浏览 => 点击浏览(),
                ID_安装 => 点击安装(),
                ID_取消 => 点击取消(窗口),
                _ => {}
            }
            0
        }
        消息_进度 => {
            设置进度(w as u32);
            0
        }
        消息_完成 => {
            安装结束(w != 0);
            0
        }
        WM_CLOSE => {
            let 安装中 = 界面().lock().unwrap().安装中;
            if 安装中 {
                // 安装进行中不允许关窗，避免留下半截目录
                设置状态文字("正在安装，请稍候…（完成后可关闭窗口）");
            } else {
                DestroyWindow(窗口);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_CREATE => 0,
        _ => DefWindowProcW(窗口, 类型, w, l),
    }
}

// ============ 交互动作 ============

unsafe fn 点击浏览() {
    let 当前 = 取编辑文本();
    let 窗口 = 界面().lock().unwrap().主窗;
    if let Some(路径) = 选择目录(窗口, &当前) {
        let 编辑 = 界面().lock().unwrap().编辑框;
        SetWindowTextW(编辑, wide(&路径.to_string_lossy()).as_ptr());
        设置状态文字("准备就绪");
    }
}

unsafe fn 点击取消(窗口: HWND) {
    let 安装中 = 界面().lock().unwrap().安装中;
    if !安装中 {
        DestroyWindow(窗口);
    }
}

unsafe fn 点击安装() {
    let (已装完, 安装中) = {
        let 状态 = 界面().lock().unwrap();
        (状态.已装完, 状态.安装中)
    };
    let 窗口 = {
        let 状态 = 界面().lock().unwrap();
        状态.主窗
    };
    if 已装完 || 安装中 {
        // 装完后按钮是「完成」：直接关窗
        DestroyWindow(窗口);
        return;
    }

    let 原始 = 取编辑文本();
    if 原始.trim().is_empty() {
        设置状态文字("请选择安装目录");
        return;
    }
    let 安装根 = super::绝对化(&super::清理输入路径(&原始));
    if 安装根.exists() && !安装根.is_dir() {
        设置状态文字("该路径已存在且不是目录，请换一个目录");
        return;
    }
    if 安装根.join("版本管理器").is_dir() {
        let 问 = format!(
            "目录已包含智语言套件：\n{}\n\n继续将覆盖同名文件，是否继续？",
            安装根.display()
        );
        // ⚠ 必须先把窗口句柄取出来、释放掉状态锁再弹模态框：
        //    模态框会继续派发本窗口的消息，若此时仍持有 Mutex 会自锁死（Mutex 不可重入）。
        let 答 = MessageBoxW(
            窗口,
            wide(&问).as_ptr(),
            wide("确认覆盖").as_ptr(),
            MB_YESNO,
        );
        if 答 != IDYES {
            return;
        }
    }

    {
        let mut 状态 = 界面().lock().unwrap();
        状态.安装中 = true;
        状态.安装根 = 安装根.clone();
        状态.错误 = None;
        EnableWindow(状态.编辑框, 0);
        EnableWindow(状态.浏览钮, 0);
        EnableWindow(状态.安装钮, 0);
        EnableWindow(状态.取消钮, 0);
        SetWindowTextW(状态.安装钮, wide("安装中…").as_ptr());
        SetWindowTextW(状态.状态栏, wide("正在解压安装包…").as_ptr());
        SendMessageW(状态.进度条, PBM_SETPOS, 0, 0);
    }

    let 数据 = 载荷取();
    let 版本 = 版本取();
    std::thread::spawn(move || {
        let 回调根 = 安装根.clone();
        let 结果 = super::执行安装到目录(数据.as_slice(), &回调根, &版本, &|百分比| unsafe {
            PostMessageW(窗口, 消息_进度, 百分比 as WPARAM, 0);
        });
        match 结果 {
            Ok(()) => {
                unsafe { PostMessageW(窗口, 消息_完成, 1, 0) };
            }
            Err(错误) => {
                {
                    let mut 状态 = 界面().lock().unwrap();
                    状态.错误 = Some(错误);
                }
                unsafe { PostMessageW(窗口, 消息_完成, 0, 0) };
            }
        }
    });
}

unsafe fn 安装结束(成功: bool) {
    let (根, 管理器目录) = {
        let 状态 = 界面().lock().unwrap();
        let 管理器 = 状态
            .安装根
            .join("版本管理器")
            .join(base::平台::可执行文件名("智语言"));
        (
            状态.安装根.clone(),
            管理器
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
    };

    if 成功 {
        let 提示 = format!(
            "安装完成：{}\n\n下一步：把下面这个目录加入 PATH（用户变量即可）\n{}\n\n然后重开终端执行：智语言 版本",
            根.display(),
            管理器目录
        );
        let mut 状态 = 界面().lock().unwrap();
        状态.安装中 = false;
        状态.已装完 = true;
        SetWindowTextW(状态.安装钮, wide("完成").as_ptr());
        EnableWindow(状态.安装钮, 1);
        EnableWindow(状态.取消钮, 1);
        SetWindowTextW(状态.取消钮, wide("关闭").as_ptr());
        SetWindowTextW(状态.状态栏, wide(&提示).as_ptr());
        SendMessageW(状态.进度条, PBM_SETPOS, 100, 0);
    } else {
        let 错误 = {
            let 状态 = 界面().lock().unwrap();
            状态.错误.clone().unwrap_or_else(|| "安装失败".to_string())
        };
        let mut 状态 = 界面().lock().unwrap();
        状态.安装中 = false;
        SetWindowTextW(状态.安装钮, wide("重试").as_ptr());
        EnableWindow(状态.安装钮, 1);
        EnableWindow(状态.编辑框, 1);
        EnableWindow(状态.浏览钮, 1);
        EnableWindow(状态.取消钮, 1);
        SetWindowTextW(状态.状态栏, wide(&format!("安装失败：{}", 错误)).as_ptr());
        SendMessageW(状态.进度条, PBM_SETPOS, 0, 0);
    }
}

// ============ 小工具 ============

unsafe fn 取编辑文本() -> String {
    let 句柄 = 界面().lock().unwrap().编辑框;
    // ⚠ 不要用 GetWindowTextW(hwnd, null, 0) 取长度：nMaxCount=0 时 API 直接返回 0
    //   （实测踩过：安装按钮永远只提示"请选择安装目录"，而框里明明有路径）。
    let mut 缓冲 = vec![0u16; 4096];
    let 实际 = GetWindowTextW(句柄, 缓冲.as_mut_ptr(), 缓冲.len() as i32);
    if 实际 <= 0 {
        return String::new();
    }
    从宽(&缓冲[..实际 as usize])
}

unsafe fn 选择目录(属主: HWND, _初始: &str) -> Option<PathBuf> {
    let 标题 = wide("选择智语言安装目录");
    let mut 显示 = vec![0u16; 260];
    let 信息 = 浏览信息 {
        属主,
        根: std::ptr::null_mut(),
        显示名: 显示.as_mut_ptr(),
        标题: 标题.as_ptr(),
        标志: BIF_RETURNONLYFSDIRS | BIF_EDITBOX | BIF_NEWDIALOGSTYLE,
        回调: 0,
        参数: 0,
        图标索引: 0,
    };
    let 标识 = SHBrowseForFolderW(&信息);
    if 标识.is_null() {
        return None;
    }
    let mut 缓冲 = vec![0u16; 1024];
    let 成功 = SHGetPathFromIDListW(标识, 缓冲.as_mut_ptr());
    CoTaskMemFree(标识 as *mut core::ffi::c_void);
    if 成功 == 0 {
        return None;
    }
    Some(PathBuf::from(从宽(&缓冲)))
}

fn wide(文本: &str) -> Vec<u16> {
    文本.encode_utf16().chain(std::iter::once(0)).collect()
}

fn 从宽(缓冲: &[u16]) -> String {
    let 结尾 = 缓冲.iter().position(|&c| c == 0).unwrap_or(缓冲.len());
    String::from_utf16_lossy(&缓冲[..结尾])
}


