// 文件: 基础功能/src/运行时/外部调用.rs
// 职责: 为运行时和标准库提供操作系统接口，通过 FFI 导出

use std::fs;
use std::process::Command;

#[export_name = "系统文件读取"]
pub extern "C" fn 系统文件读取(路径: *const u8, 长度: i64) -> i64 {
    let 路径字符串 = 读取字符串(路径, 长度);
    match fs::read_to_string(路径字符串) {
        Ok(内容) => { let 内容字节 = 内容.into_bytes(); Box::into_raw(Box::new(内容字节)) as i64 }
        Err(_) => 0,
    }
}

#[export_name = "系统文件写入"]
pub extern "C" fn 系统文件写入(路径: *const u8, 路径长: i64, 内容: *const u8, 内容长: i64) -> i64 {
    let 路径字符串 = 读取字符串(路径, 路径长);
    let 内容字符串 = 读取字符串(内容, 内容长);
    fs::write(路径字符串, 内容字符串).is_ok() as i64
}

#[export_name = "系统文件存在"]
pub extern "C" fn 系统文件存在(路径: *const u8, 长度: i64) -> i64 {
    let 路径字符串 = 读取字符串(路径, 长度);
    fs::metadata(路径字符串).is_ok() as i64
}

#[export_name = "系统文件大小"]
pub extern "C" fn 系统文件大小(路径: *const u8, 长度: i64) -> i64 {
    let 路径字符串 = 读取字符串(路径, 长度);
    fs::metadata(路径字符串).map(|m| m.len() as i64).unwrap_or(0)
}

#[export_name = "系统创建目录"]
pub extern "C" fn 系统创建目录(路径: *const u8, 长度: i64) -> i64 {
    let 路径字符串 = 读取字符串(路径, 长度);
    fs::create_dir_all(路径字符串).is_ok() as i64
}

#[export_name = "系统文件删除"]
pub extern "C" fn 系统文件删除(路径: *const u8, 长度: i64) -> i64 {
    let 路径字符串 = 读取字符串(路径, 长度);
    fs::remove_file(路径字符串).is_ok() as i64
}

#[export_name = "系统执行命令"]
pub extern "C" fn 系统执行命令(命令: *const u8, 长度: i64) -> i64 {
    let 命令字符串 = 读取字符串(命令, 长度);
    // 跨平台外壳：Windows cmd /C，POSIX sh -c（统一真相，与 标准库 系统.执行命令 一致）
    let (程序, 标志) = crate::平台::外壳();
    let 输出 = Command::new(程序).arg(标志).arg(命令字符串).output();
    match 输出 {
        Ok(输出) => {
            let 结果 = String::from_utf8_lossy(&输出.stdout).to_string();
            let 结果字节 = 结果.into_bytes();
            Box::into_raw(Box::new(结果字节)) as i64
        }
        Err(_) => 0,
    }
}

#[export_name = "系统获取环境变量"]
pub extern "C" fn 系统获取环境变量(名称: *const u8, 长度: i64) -> i64 {
    let 名称字符串 = 读取字符串(名称, 长度);
    match std::env::var(名称字符串) {
        Ok(值) => { let 值字节 = 值.into_bytes(); Box::into_raw(Box::new(值字节)) as i64 }
        Err(_) => 0,
    }
}

#[export_name = "系统时间戳"]
pub extern "C" fn 系统时间戳() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[export_name = "系统生成唯一编号"]
pub extern "C" fn 系统生成唯一编号() -> i64 {
    let uuid = uuid::Uuid::new_v4().to_string();
    let 字节 = uuid.into_bytes();
    Box::into_raw(Box::new(字节)) as i64
}

fn 读取字符串(指针: *const u8, 长度: i64) -> String {
    unsafe {
        let 切片 = std::slice::from_raw_parts(指针, 长度 as usize);
        String::from_utf8_lossy(切片).to_string()
    }
}
