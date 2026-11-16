// 文件: 后端/解释器/src/字节码格式.rs
// 职责: 字节码文件二进制格式定义与读写，操作码常量定义
// V3: 新增取地址、间接加载、间接存储、创建闭包操作码（原 `清理调用` 0xE4 已退役，见下）

use std::io::{Read, Write, Result as IoResult};

pub const 魔数: u32 = 0x43534C42;
pub const 当前主版本: u16 = 5;  // V5: 新增「拥有型槽段」（统一帧契约 UFC 释放侧元数据：每函数的拥有型槽索引）
pub const 当前次版本: u16 = 0;

#[derive(Debug, Clone)]
pub struct 字节码文件头 {
    pub 主版本: u16,
    pub 次版本: u16,
    pub 模块名偏移: u32,
    pub 常量池偏移: u32,
    pub 函数表偏移: u32,
    pub 调试信息偏移: u32,
    pub 代码段偏移: u32,
    /// V4: 异常表段偏移（代码段之后；每函数 块起始偏移→(类别,目标块偏移)）。0=旧文件无异常表。
    pub 异常表偏移: u32,
    /// 🆕 V5: **拥有型槽段**偏移（异常表段之后；每函数「拥有型槽」局部索引清单）。
    /// 用途：统一帧契约（UFC）**释放侧** —— 引擎弹帧时按本清单对被弹函数批量 −1
    ///   （`base::值类型::批量释放槽清单`；门控 `智语言_帧级释放`）。0=旧文件无该段 ⇒ 保守不释放。
    /// 为何放进字节码：**避免"跨表按名/按序查"**（VM 函数表名是合成名 `fNN` 且多模块降级会累积序表 ⇒
    ///   命名空间/顺序都可能不一致，实测多数函数因此查不到 ⇒ 释放不生效）。
    pub 拥有型槽段偏移: u32,
    pub 入口函数索引: u32,
}

impl 字节码文件头 {
    pub fn 写入<W: Write>(&self, 写入器: &mut W) -> IoResult<()> {
        写入器.write_all(&魔数.to_le_bytes())?;
        写入器.write_all(&self.主版本.to_le_bytes())?;
        写入器.write_all(&self.次版本.to_le_bytes())?;
        写入器.write_all(&self.模块名偏移.to_le_bytes())?;
        写入器.write_all(&self.常量池偏移.to_le_bytes())?;
        写入器.write_all(&self.函数表偏移.to_le_bytes())?;
        写入器.write_all(&self.调试信息偏移.to_le_bytes())?;
        写入器.write_all(&self.代码段偏移.to_le_bytes())?;
        写入器.write_all(&self.异常表偏移.to_le_bytes())?;
        写入器.write_all(&self.拥有型槽段偏移.to_le_bytes())?;
        写入器.write_all(&self.入口函数索引.to_le_bytes())?;
        Ok(())
    }

    pub fn 读取<R: Read>(读取器: &mut R) -> IoResult<Self> {
        let mut 魔数缓冲 = [0u8; 4];
        读取器.read_exact(&mut 魔数缓冲)?;
        let 主版本 = 读取_u16(读取器)?;
        let 次版本 = 读取_u16(读取器)?;
        let 模块名偏移 = 读取_u32(读取器)?;
        let 常量池偏移 = 读取_u32(读取器)?;
        let 函数表偏移 = 读取_u32(读取器)?;
        let 调试信息偏移 = 读取_u32(读取器)?;
        let 代码段偏移 = 读取_u32(读取器)?;
        let 异常表偏移 = 读取_u32(读取器)?;
        let 拥有型槽段偏移 = 读取_u32(读取器)?;
        let 入口函数索引 = 读取_u32(读取器)?;
        Ok(字节码文件头 {
            主版本, 次版本, 模块名偏移,
            常量池偏移, 函数表偏移, 调试信息偏移, 代码段偏移,
            异常表偏移, 拥有型槽段偏移, 入口函数索引,
        })
    }
}

fn 读取_u16<R: Read>(读取器: &mut R) -> IoResult<u16> {
    let mut 缓冲 = [0u8; 2];
    读取器.read_exact(&mut 缓冲)?;
    Ok(u16::from_le_bytes(缓冲))
}

fn 读取_u32<R: Read>(读取器: &mut R) -> IoResult<u32> {
    let mut 缓冲 = [0u8; 4];
    读取器.read_exact(&mut 缓冲)?;
    Ok(u32::from_le_bytes(缓冲))
}

pub mod 操作码 {
    // 控制流
    pub const 跳转: u8 = 0x00;
    pub const 条件跳转: u8 = 0x01;
    pub const 返回: u8 = 0x02;
    pub const 切换: u8 = 0x03;  // ⚡双模式⚡ 枚举匹配 switch：操作数=值, 分支表[(i64,目标)], 默认分支

    // 算术
    pub const 加法: u8 = 0x40;
    pub const 减法: u8 = 0x41;
    pub const 乘法: u8 = 0x42;
    pub const 除法: u8 = 0x43;
    pub const 整除: u8 = 0x44;
    pub const 取余: u8 = 0x45;
    pub const 幂: u8 = 0x46;
    pub const 取负: u8 = 0x47;

    // 位运算
    pub const 位与: u8 = 0x48;
    pub const 位或: u8 = 0x49;
    pub const 位非: u8 = 0x4A;
    pub const 异或: u8 = 0x4B;
    pub const 左移: u8 = 0x4C;
    pub const 右移: u8 = 0x4D;

    // 比较
    pub const 等于: u8 = 0x50;
    pub const 不等于: u8 = 0x51;
    pub const 小于: u8 = 0x52;
    pub const 大于: u8 = 0x53;
    pub const 小于等于: u8 = 0x54;
    pub const 大于等于: u8 = 0x55;
    pub const 实例相等: u8 = 0x56;
    pub const 实例不等: u8 = 0x57;

    // 逻辑
    pub const 逻辑与: u8 = 0x58;
    pub const 逻辑或: u8 = 0x59;
    pub const 逻辑非: u8 = 0x5A;
    pub const 属于: u8 = 0x5B;
    pub const 不属于: u8 = 0x5C;
    pub const 类型判断: u8 = 0x5D;  // ⚡双模式⚡ 类型判断：操作数1=值, 操作数2=类型名文本, 结果=逻辑

    // 内存操作
    pub const 加载: u8 = 0x80;
    pub const 存储: u8 = 0x81;
    pub const 取地址: u8 = 0x82;        // V3: 取局部变量地址 → 值::局部引用
    pub const 间接加载: u8 = 0x8D;      // V3: 通过引用间接加载
    pub const 间接存储: u8 = 0x8E;      // V3: 通过引用间接存储
    pub const 数组加载: u8 = 0x83;
    pub const 数组存储: u8 = 0x84;
    pub const 数组长度: u8 = 0x85;
    pub const 数组查找: u8 = 0x86;
    pub const 数组追加: u8 = 0x90;
    pub const 数组插入: u8 = 0x91;
    pub const 数组删除: u8 = 0x92;
    pub const 数组取末尾: u8 = 0x93;
    pub const 数组反转: u8 = 0x94;
    pub const 数组排序: u8 = 0x95;
    pub const 数组清空: u8 = 0x96;
    pub const 映射加载: u8 = 0x87;
    pub const 映射存储: u8 = 0x88;
    pub const 映射包含: u8 = 0x8A;
    /// 映射安全加载（.获取 方法）：缺失键返回空值，与 映射加载（索引器 []，缺失键抛异常）区分。
    /// 注意：0x8B 已被 字段加载 占用，此处用空闲值 0x8F 避免字节码分发撞车。
    pub const 映射安全加载: u8 = 0x8F;
    pub const 映射删除: u8 = 0x89;
    pub const 映射清空: u8 = 0x99;
    pub const 映射所有键: u8 = 0x97;
    pub const 映射所有值: u8 = 0x98;
    pub const 映射长度: u8 = 0x9A;
    /// 2026-08-15 统一索引分发：用户 容器[键] 语法唯一字节码入口（类型可能未知），运行时按堆实际类型分发。
    /// ⚠ 字节码值必须避开已占用常量：0x9B=文本替换 0x9C=文本分隔 0x9D=文本去除空白，故用 0x9E/0x9F。
    pub const 索引加载: u8 = 0x9E;
    pub const 索引存储: u8 = 0x9F;
    /// 2026-08-15 统一容器方法分发：编译期容器种类未知（裸标注/嵌套索引结果/任意类型/泛型擦除）时
    /// 的通用容器方法入口，运行时按堆实际类型分派数组/映射/文本语义。0xA0-0xBF 段此前全空闲。
    pub const 容器插入: u8 = 0xA0;
    pub const 容器删除: u8 = 0xA1;
    pub const 容器清空: u8 = 0xA2;
    pub const 容器是否包含: u8 = 0xA3;
    pub const 容器获取: u8 = 0xA4;
    pub const 字段加载: u8 = 0x8B;
    pub const 字段存储: u8 = 0x8C;

    // 类型转换
    pub const 整数转小数: u8 = 0xC0;
    pub const 小数转整数: u8 = 0xC1;
    pub const 整数转文本: u8 = 0xC2;
    pub const 小数转文本: u8 = 0xC3;
    pub const 逻辑转文本: u8 = 0xC4;
    pub const 文本转整数: u8 = 0xC5;
    pub const 文本转小数: u8 = 0xC6;
    pub const 任意转文本: u8 = 0xC7;

    // 运行时
    pub const 调用: u8 = 0xE0;
    pub const 间接调用: u8 = 0xE1;      // V3: 通过闭包值间接调用
    pub const 创建闭包: u8 = 0xE2;      // V3: 创建闭包值
    pub const 创建结构体: u8 = 0xE3;
    // ⚠️ 0xE4 已退役（原 `清理调用`：随 `清理` 生命周期方法 2026-09-16 整体删除）。
    //    退役空位【勿复用】——避免与既有产物/缓存中的历史字节码混淆（同 base 值标签 9 的处置先例）。
    pub const 空值检查: u8 = 0xEC;
    pub const 文本查找: u8 = 0xED;
    pub const 深度复制: u8 = 0xF5;  // 传值语义：深拷贝复合值（结构体/数组/映射/接口/任意）
    // 🆕 2026-09-16 引用计数闭环（内存管理激活）：与 base 堆操作 77/78 成对
    pub const 释放引用: u8 = 0xFA;  // −1：栈槽消亡处释放句柄（归零即回收 + 级联）
    pub const 增加引用: u8 = 0xFB;  // +1：接口引用语义路径（接口赋值/实参）
    pub const 文本截取: u8 = 0xEE;
    pub const 文本是否包含: u8 = 0xA5;  // ⚠ 原误写为 0x9E 与 索引加载 撞车，改为空闲段 0xA5（与 容器是否包含 0xA3 同族）。
    pub const 文本替换: u8 = 0x9B;
    pub const 文本分隔: u8 = 0x9C;
    pub const 文本去除空白: u8 = 0x9D;
    pub const 任务启动: u8 = 0xE9;
    pub const 任务等待: u8 = 0xEA;
    pub const 批量等待: u8 = 0xF0;
    pub const 批量等待超时: u8 = 0xF1;
    pub const 任务让出: u8 = 0xEB;
    pub const 通道创建: u8 = 0xE5;
    pub const 通道发送: u8 = 0xE6;
    pub const 通道接收: u8 = 0xE7;
    pub const 通道关闭: u8 = 0xE8;
    pub const 抛出: u8 = 0x04;
    // 🆕 方案B S4：设置异常处理(0xEF)/弹出异常处理(0xF2) 已删除（异常派发静态化，前端不再发射）。
    pub const 传播异常: u8 = 0xF3;
    // 🆕 方案C2（2026-09-04）异常状态单源 opcode（取代 全局变量(0) 伪变量通道）。
    //   查询重抛标志：count=1，发射 [dest]（写入 0/1 整数）；复位重抛标志：count=0，仅 opcode。
    pub const 查询重抛标志: u8 = 0xF6;
    pub const 复位重抛标志: u8 = 0xF7;
    pub const 置位重抛标志: u8 = 0xF8;
    pub const 外部调用: u8 = 0xF4;      // 🆕 FFI: 调用外部（C）函数
}
