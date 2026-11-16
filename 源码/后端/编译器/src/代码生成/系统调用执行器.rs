// 文件: 后端/编译器/src/代码生成/系统调用执行器.rs
// 职责: 底层表示 → Cranelift IR → 目标文件，完整支持结构体/数组/函数调用（含参数传递）和DWARF调试信息

use cranelift_codegen::ir::{types, AbiParam, InstBuilder, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{Linkage, Module, FuncId};
use cranelift_object::ObjectModule;
use target_lexicon::Triple;

use frontend::底层表示::指令集::三地址码;

/// 统一值表示：每个局部 = (标签, 载荷) 双槽 i64，与运行时 运行时值 / 智语言_堆操作 / 智语言_系统调用 同一套语言。
use crate::代码生成::编译器;

impl 编译器 {
    pub(crate) fn 取系统调用函数(
        &mut self,
        模块: &mut ObjectModule,
    ) -> Result<FuncId, String> {
        if let Some(fid) = self.系统调用函数标识 {
            return Ok(fid);
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64));
        签名.params.push(AbiParam::new(types::I64));
        签名.params.push(AbiParam::new(types::I64));
        签名.params.push(AbiParam::new(types::I64));
        let fid = 模块
            .declare_function("智语言_系统调用", Linkage::Import, &签名)
            .map_err(|e| format!("声明系统调用函数失败: {}", e))?;
        self.系统调用函数标识 = Some(fid);
        Ok(fid)
    }

    pub(crate) fn 取异常函数(
        &mut self,
        模块: &mut ObjectModule,
        名称: &str,
    ) -> Result<FuncId, String> {
        if let Some(&fid) = self.异常函数缓存.get(名称) {
            return Ok(fid);
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        if 名称 == "智语言_异常待处理" {
            签名.returns.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_异常待重抛" {
            签名.returns.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_抛出异常" {
            // C-ABI: 智语言_抛出异常(类型:*const u8, 类型长度:i64, 描述:*const u8, 描述长度:i64)
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_打印当前异常" {
            // void：无参数、无返回值（默认空签名）
        } else if 名称 == "智语言_重新抛出" {
            // void：无参数、无返回值（裸【抛出】重新抛出，置位 待处理）
        // 🆕 方案C2：压入/弹出/取顶层/派发异常处理 死分支（已停用动态栈）——异常派发静态单源化后，
        // 编译器不再请求这些 base C-ABI 符号（契约测试保留于 base 派发状态机契约 模块），故移除其签名声明。
        } else if 名称 == "智语言_压入调用帧" {
            // ★ 2026-09-28（**F8-①**）：
            //   智语言_压入调用帧(函数名指针:*const u8, 函数名长度:i64, 位置指针:*const u8, 位置长度:i64) -> void
            //   ⚠ 必须在此登记签名：本表是唯一签名真相，未登记的符号会被声明为**空签名（0 参）** ✗，
            //     而调用点按 4 参发射 ⇒ Cranelift 校验失败「mismatched argument count: got 4, expected 0」
            //     （实测：编译函数【内层】失败，location inst25）✗。
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_弹出调用帧" {
            // void：无参数、无返回值（默认空签名）
        } else if 名称 == "智语言_设置异常位置" {
            // 设置异常统一位置：智语言_设置异常位置(路径指针:*const u8, 路径长度:i64) -> void
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_设置调用深度上限" {
            // 设置失控递归调用深度上限：(上限:i64) -> void
            签名.params.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_压入编译帧" {
            // 🆕 统一帧契约（UFC）：(影子块地址:i64, 槽数:i64, 拥有型槽清单指针:i64, 清单长度:i64) -> void
            //   （编译帧链压栈 + 首次自注册根提供者；清单供弹帧时**帧级批量释放**）
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
            签名.params.push(AbiParam::new(types::I64));
        } else if 名称 == "智语言_弹出编译帧" {
            // 🆕 统一帧契约（UFC）：无参无返回（编译帧链弹栈；弹空自动注销根提供者）
        }
        let fid = 模块
            .declare_function(名称, Linkage::Import, &签名)
            .map_err(|e| format!("声明异常函数 {} 失败: {}", 名称, e))?;
        self.异常函数缓存.insert(名称.to_string(), fid);
        Ok(fid)
    }

    /// ⚡FFI⚡: 声明 C-ABI 导入 智语言_注册外部函数（编译模式 prologue 注册外部函数用）。
    /// 签名：17 个 I64 参数（前 10 同 FFI 项元数据，接着 是否静态/原生约定，
    /// 再 3 个为 🆕 P3 精确 C 类型：形参C类型数组指针/长度、返回C类型，
    /// 末尾 2 个为 🆕 回调：函数指针形状数组指针/长度），无返回值。
    /// ★须与 base `注册外部函数_导出` 形参表严格逐一对齐——Cranelift 会校验实参个数，
    ///   漏声明将在 定义函数【启动程序】 阶段报 mismatched argument count。
    pub(crate) fn 取注册外部函数(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.注册外部函数标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 键
        签名.params.push(AbiParam::new(types::I64)); // c符号指针
        签名.params.push(AbiParam::new(types::I64)); // c符号长度
        签名.params.push(AbiParam::new(types::I64)); // 库标识指针
        签名.params.push(AbiParam::new(types::I64)); // 库标识长度
        签名.params.push(AbiParam::new(types::I64)); // 返回标签
        签名.params.push(AbiParam::new(types::I64)); // 形参标签数组指针
        签名.params.push(AbiParam::new(types::I64)); // 形参标签长度
        签名.params.push(AbiParam::new(types::I64)); // 输出位图数组指针
        签名.params.push(AbiParam::new(types::I64)); // 输出位图长度
        签名.params.push(AbiParam::new(types::I64)); // 是否静态 (0/1)
        签名.params.push(AbiParam::new(types::I64)); // 原生约定 (0/1)
        签名.params.push(AbiParam::new(types::I64)); // 🆕 P3: 形参C类型数组指针
        签名.params.push(AbiParam::new(types::I64)); // 🆕 P3: 形参C类型长度
        签名.params.push(AbiParam::new(types::I64)); // 🆕 P3: 返回C类型
        签名.params.push(AbiParam::new(types::I64)); // 🆕 回调: 函数指针形状数组指针
        签名.params.push(AbiParam::new(types::I64)); // 🆕 回调: 函数指针形状长度
        let id = 模块
            .declare_function("智语言_注册外部函数", Linkage::Import, &签名)
            .expect("声明 智语言_注册外部函数 失败");
        self.注册外部函数标识 = Some(id);
        id
    }

    /// 🆕 G1：声明 C ABI 导入 智语言_注册外部结构体布局。
    /// 走**旁路通道**而非扩充 取注册外部函数 的 17 参签名——跨层签名漂移是 FFI 最隐蔽的
    /// bug 源（历史上漏参直接导致编译模式整体 mismatched argument count）。
    /// 签名：名称指针/名称长度/字段类型码数组指针/字段类型码长度。
    pub(crate) fn 取注册外部结构体布局(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.注册外部结构体布局标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 名称指针
        签名.params.push(AbiParam::new(types::I64)); // 名称长度
        签名.params.push(AbiParam::new(types::I64)); // 字段类型码数组指针
        签名.params.push(AbiParam::new(types::I64)); // 字段类型码长度
        let id = 模块
            .declare_function("智语言_注册外部结构体布局", Linkage::Import, &签名)
            .expect("声明 智语言_注册外部结构体布局 失败");
        self.注册外部结构体布局标识 = Some(id);
        id
    }

    /// 🆕 G1：声明 C ABI 导入 智语言_设置FFI结构体名（为已注册外部函数补结构体名）。
    /// 签名：键/形参名打包指针/打包长度/返回名指针/返回名长度。
    /// 形参名以 `\u{1}` 分隔打包成单串，避免引入二维指针数组。
    pub(crate) fn 取设置FFI结构体名(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.设置FFI结构体名标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 键
        签名.params.push(AbiParam::new(types::I64)); // 形参结构体名打包指针
        签名.params.push(AbiParam::new(types::I64)); // 形参结构体名打包长度
        签名.params.push(AbiParam::new(types::I64)); // 返回结构体名指针
        签名.params.push(AbiParam::new(types::I64)); // 返回结构体名长度
        let id = 模块
            .declare_function("智语言_设置FFI结构体名", Linkage::Import, &签名)
            .expect("声明 智语言_设置FFI结构体名 失败");
        self.设置FFI结构体名标识 = Some(id);
        id
    }

    /// 🆕 2026-08-29 FFI 数组：声明 C ABI 导入 智语言_设置FFI数组元数据
    /// （为已注册外部函数补数组元素类型码与长度参数索引）。
    /// 签名：键/元素码数组指针/元素码长度/长度索引数组指针/长度索引长度。
    /// 长度索引数组按 u64（64 位平台 usize）逐元素传递，避免二维数组复杂度。
    pub(crate) fn 取设置FFI数组元数据(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.设置FFI数组元数据标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 键
        签名.params.push(AbiParam::new(types::I64)); // 元素码数组指针
        签名.params.push(AbiParam::new(types::I64)); // 元素码数组长度
        签名.params.push(AbiParam::new(types::I64)); // 长度索引数组指针
        签名.params.push(AbiParam::new(types::I64)); // 长度索引数组长度
        let id = 模块
            .declare_function("智语言_设置FFI数组元数据", Linkage::Import, &签名)
            .expect("声明 智语言_设置FFI数组元数据 失败");
        self.设置FFI数组元数据标识 = Some(id);
        id
    }

    /// 声明 C ABI 导入 智语言_注册包装调度函数（编译模式注册 shim 分发函数指针）
    pub(crate) fn 取注册包装调度函数(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.注册包装调度函数标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 调度函数地址
        let id = 模块
            .declare_function("智语言_注册包装调度函数", Linkage::Import, &签名)
            .expect("声明 智语言_注册包装调度函数 失败");
        self.注册包装调度函数标识 = Some(id);
        id
    }

    /// 🆕 FFI（§6.5.2 第5项）：声明 C ABI 导入 智语言_设置FFI全局变量标记。
    /// 签名：键（单 I64 参数），无返回值。编译模式 prologue 在注册外部函数后，
    /// 对全局变量型外部函数补发一次，使 shim 走 `static` 读取而非 `fn` 调用。
    pub(crate) fn 取设置FFI全局变量标记(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.设置FFI全局变量标记标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 键
        let id = 模块
            .declare_function("智语言_设置FFI全局变量标记", Linkage::Import, &签名)
            .expect("声明 智语言_设置FFI全局变量标记 失败");
        self.设置FFI全局变量标记标识 = Some(id);
        id
    }

    /// 🆕 声明 C ABI 导入 智语言_注册回调提供者（编译模式注册 shim 回调 trampoline 提供者）
    pub(crate) fn 取注册回调提供者(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.注册回调提供者标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 注册器函数地址
        let id = 模块
            .declare_function("智语言_注册回调提供者", Linkage::Import, &签名)
            .expect("声明 智语言_注册回调提供者 失败");
        self.注册回调提供者标识 = Some(id);
        id
    }

    /// 🆕 声明 C ABI 导入 智语言_设置回调调度（编译模式设置 shim 回调系统调用）
    pub(crate) fn 取设置回调调度(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.设置回调调度标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 设置函数地址
        let id = 模块
            .declare_function("智语言_设置回调调度", Linkage::Import, &签名)
            .expect("声明 智语言_设置回调调度 失败");
        self.设置回调调度标识 = Some(id);
        id
    }

    /// 🆕 声明 C ABI 导入 智语言_注册用户函数调用器（编译模式登记「按索引调用」thunk）
    pub(crate) fn 取注册用户函数调用器(&mut self, 模块: &mut ObjectModule) -> FuncId {
        if let Some(id) = self.注册用户函数调用器标识 {
            return id;
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // thunk 函数地址
        let id = 模块
            .declare_function("智语言_注册用户函数调用器", Linkage::Import, &签名)
            .expect("声明 智语言_注册用户函数调用器 失败");
        self.注册用户函数调用器标识 = Some(id);
        id
    }

    pub(crate) fn 注入异常检查(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
    ) -> Result<(), String> {
        // 🆕 方案B S3：静态异常派发——按「当前基本块」的 异常目标 直跳（前端 S1 标注单源，
        // 与解释器 派发静态异常 同一套语义）。不再调用 base 智语言_派发异常 状态机。
        //   捕获(块)：在途 → 跳 catch/stub（其入口 复位重抛标志 吞 / 置位重抛标志 传播，保留载荷）；
        //   收尾(块)：在途 → 置位重抛标志（智语言_重新抛出，置 base 待处理=1 保载荷）+ 跳始终块（两模同一 base 函数）；
        //              （保载荷；清在途防 finally 体内调用残留误派发——X2A finally 截断根因）+ 跳始终块；
        //   无：在途 → 跳 异常返回块（早返回；调用方注入点按调用点所在块的静态目标接管 = 跨函数）。
        let 异常返回块 = self.异常返回块.ok_or_else(|| "异常返回块未创建".to_string())?;
        let 函数 = self.取异常函数(模块, "智语言_异常待处理")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 结果 = 构建器.ins().call(调用, &[]);
        let 标志 = 构建器.func.dfg.first_result(结果);
        let 零 = 构建器.ins().iconst(types::I64, 0);
        let 置位 = 构建器.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, 标志, 零);
        let 合并块 = 构建器.create_block();
        let 派发块 = 构建器.create_block();
        构建器.ins().brif(置位, 派发块, &[], 合并块, &[]);
        构建器.switch_to_block(派发块);
        match self.当前异常目标 {
            frontend::底层表示::模块结构::异常目标::捕获(目标块) => {
                let 目标 = *self.基本块映射.get(&目标块).unwrap_or(&异常返回块);
                构建器.ins().jump(目标, &[]);
            }
            frontend::底层表示::模块结构::异常目标::收尾(目标块) => {
                //
                //
                // 🆕 方案C2.1：进入 finally 前 = 置「待重抛」(智语言_设置待重抛) + 清「待处理」——
                //   清待处理 防 finally 体内调用注入检查误拦（C2 曾把重抛并入 base 待处理致 X2 编译截断）。
                let 重抛函数 = self.取异常函数(模块, "智语言_设置待重抛")?;
                let 重抛调用 = 模块.declare_func_in_func(重抛函数, &mut 构建器.func);
                构建器.ins().call(重抛调用, &[]);
                let 清除函数 = self.取异常函数(模块, "智语言_清除待处理")?;
                let 清除调用 = 模块.declare_func_in_func(清除函数, &mut 构建器.func);
                构建器.ins().call(清除调用, &[]);
                let 目标 = *self.基本块映射.get(&目标块).unwrap_or(&异常返回块);
                构建器.ins().jump(目标, &[]);
            }
            frontend::底层表示::模块结构::异常目标::无 => {
                构建器.ins().jump(异常返回块, &[]);
            }
        }
        // 合并块：正常（无异常）路径继续
        构建器.switch_to_block(合并块);
        Ok(())
    }

    /// 编译模式“抛出”辅助：调用 C ABI 智语言_抛出异常 设置可捕获异常，并立即注入异常检查
    /// （跳转到当前 catch 块或 异常返回块），与解释器 抛出 opcode 语义一致。
    pub(crate) fn 编译抛出(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        类型: &str,
        描述: &str,
    ) -> Result<(), String> {
        let 函数 = self.取异常函数(模块, "智语言_抛出异常")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 类型值 = 构建器.ins().iconst(types::I64, 类型.as_ptr() as i64);
        let 类型长度 = 构建器.ins().iconst(types::I64, 类型.len() as i64);
        let 描述值 = 构建器.ins().iconst(types::I64, 描述.as_ptr() as i64);
        let 描述长度 = 构建器.ins().iconst(types::I64, 描述.len() as i64);
        构建器.ins().call(调用, &[类型值, 类型长度, 描述值, 描述长度]);
        self.注入异常检查(构建器, 模块)?;
        Ok(())
    }

    /// 编译模式失控递归防护：用户函数 prologue 调用 智语言_递增调用深度，
    /// 超上限时该函数内部抛「栈溢出」异常，故立即注入异常检查（与 编译抛出 同语义）。
    pub(crate) fn 编译递增调用深度(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
    ) -> Result<(), String> {
        let 函数 = self.取异常函数(模块, "智语言_递增调用深度")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        构建器.ins().call(调用, &[]);
        self.注入异常检查(构建器, 模块)?;
        Ok(())
    }

    /// 编译模式失控递归防护：入口函数 prologue 调用 智语言_设置调用深度上限，
    /// 把上限覆盖为「编译器侧同源值」——base 静态库恒以 release 构建，其 cfg 默认值
    /// 与解释模式（debug）不一致，必须显式对齐才能保证双模 parity。
    pub(crate) fn 编译设置调用深度上限(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
    ) -> Result<(), String> {
        let 函数 = self.取异常函数(模块, "智语言_设置调用深度上限")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 上限 = base::异常运行时::默认调用深度上限() as i64;
        let 上限值 = 构建器.ins().iconst(types::I64, 上限);
        构建器.ins().call(调用, &[上限值]);
        Ok(())
    }

    /// 编译模式失控递归防护：用户函数 epilogue（每个 return 前）调用 智语言_递减调用深度。
    /// void 函数无返回值，无需注入异常检查。
    pub(crate) fn 编译递减调用深度(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
    ) -> Result<(), String> {
        let 函数 = self.取异常函数(模块, "智语言_递减调用深度")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        构建器.ins().call(调用, &[]);
        Ok(())
    }

    /// ★ 2026-09-28（**F8-① 修复**）编译模式**调用栈压入**：prologue 调用 `智语言_压入调用帧`。
    ///
    /// **背景**：运行时早已提供 `智语言_压入调用帧/弹出调用帧`（`异常运行时.rs` L245-255）并在
    ///   `智语言_抛出异常` / `智语言_抛出异常_值` 中经 `构造调用栈文本()` 用它构造异常 **`堆栈` 字段**，
    ///   但**全仓库无任何调用者** ✗ ⇒ 编译产物该栈恒为空 ⇒ 实测编译模式 `异常错误().堆栈` 恒为**空串** ✗
    ///   （解释模式为 `#0 调用点偏移 …` ✓）⇒ 这正是双模最后一项真分歧 F8-① 的根因。
    /// **配对纪律**：本函数与 `编译弹出调用帧` 必须与 `编译递增/递减调用深度` **同点发射**
    ///   （prologue 1 处 + 全部 return 出口 4 处）——深度计数器的配对已被验证正确 ✓，照抄其位置即可保证平衡 ✓。
    /// **参数**：函数名（便于定位）+ 位置（调用点，可为空串 —— 编译模式无解释器式的代码偏移，
    ///   故 `堆栈` 文本形态与解释模式**天然不同**，双方都非空即达成本项修复目标 ✓）。
    pub(crate) fn 编译压入调用帧(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        函数名: &str,
    ) -> Result<(), String> {
        let 函数 = self.取异常函数(模块, "智语言_压入调用帧")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        // ⚠ 必须用 `嵌入字节` 把函数名放进**数据段**再取运行时地址：
        //   直接用 `函数名.as_ptr() as i64` 对**运行时构造的 String** 取址 ⇒ 地址编译期不可知
        //   ⇒ Cranelift 校验失败（实测 VerifierError: location inst25，编译函数【内层】失败）✗。
        //   （`编译抛出` 能直接 iconst 指针，是因为它传的是 `&'static str` 字面量 ✓。）
        let 数据名 = format!("调用帧名_{}_{}", 函数名, self.异常位置计数);
        self.异常位置计数 += 1;
        let 名指针 = self.嵌入字节(模块, 构建器, 函数名.as_bytes(), &数据名)?;
        let 名长度 = 构建器.ins().iconst(types::I64, 函数名.len() as i64);
        // 位置：编译模式无解释器式代码偏移 ⇒ 传空（运行时 `读文本` 对 长度<=0 返回空串 ✓）。
        let 空指针 = 构建器.ins().iconst(types::I64, 0);
        let 空长度 = 构建器.ins().iconst(types::I64, 0);
        构建器.ins().call(调用, &[名指针, 名长度, 空指针, 空长度]);
        Ok(())
    }

    /// ★ 2026-09-28（F8-① 修复）编译模式**调用栈弹出**：每个 return 前与 `编译递减调用深度` 同点发射。
    pub(crate) fn 编译弹出调用帧(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
    ) -> Result<(), String> {
        let 函数 = self.取异常函数(模块, "智语言_弹出调用帧")?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        构建器.ins().call(调用, &[]);
        Ok(())
    }

    pub(crate) fn 编译系统调用(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        指令: &三地址码,
        索引: usize,
    ) -> Result<(), String> {
        let argc = self.当前实参列表.len() / 2;
        let 索引值 = 构建器.ins().iconst(types::I64, 索引 as i64);
        let argc值 = 构建器.ins().iconst(types::I64, argc as i64);

        // 构建 运行时值 数组：每参数 32 字节（与运行时 运行时值 结构同构：标签@0, 整数@8, 小数@16, 句柄@24）
        let 数组字节 = (argc as i64) * 32;
        let 数组指针 = self.分配缓冲(构建器, 模块, 数组字节)?;
        for i in 0..argc {
            let 标签 = self.当前实参列表[i * 2];
            let 载荷 = self.当前实参列表[i * 2 + 1];
            let 基址 = 构建器.ins().iadd_imm(数组指针, (i as i64) * 32);
            // 标签 @ offset 0；载荷按标签落入对应字段（运行时 运行时值 布局唯一真相：句柄@24/小数@16/整数@8）
            self.写运行时值(构建器, 标签, 载荷, 基址);
        }

        // 输出缓冲需容纳完整 运行时值（32 字节），旧代码只分配 16 字节会越界写入小数@16/句柄@24。
        let 输出缓冲 = self.分配缓冲(构建器, 模块, 32)?;

        // 声明并调用 智语言_系统调用(索引:u64, 参数:*const 运行时值, 参数数量:u64, 输出:*mut 运行时值)
        let 系统调用函数 = self.取系统调用函数(模块)?;
        let 调用 = 模块.declare_func_in_func(系统调用函数, &mut 构建器.func);
        构建器.ins().call(调用, &[索引值, 数组指针, argc值, 输出缓冲]);

        self.当前实参列表.clear();

        // 结果载荷：按标签从对应字段读取（句柄@24 / 小数@16 / 整数@8），与入参编组对称。
        let (结果标签, 结果载荷) = self.读运行时值(构建器, 输出缓冲);
        self.存储值对(构建器, &指令.结果, 结果标签, 结果载荷);

        Ok(())
    }

}
