# WASM 导出接口

| 导出名称 | 参数类型 | 返回值类型 |
| --- | --- | --- |
| `memory` | 无 | WASM 32 位线性内存导出 |
| `allocate` | `length: i32` | `pointer: i32`，失败返回 `0` |
| `classify_deck` | `pointer: i32, length: i32` | `i64`，高 32 位为输出长度，低 32 位为输出指针 |
| `release` | `pointer: i32, length: i32` | 无（`void`） |
| `_initialize`（可选） | 无 | 无（`void`），每个实例创建后、首次分类前调用一次 |

上述导出用于无导入的核心 WASM 或导入 `wasi_snapshot_preview1` 的 WASI Preview 1 reactor。不会调用命令入口 `_start`。支持 WasmGC 和标准异常处理；不支持 Emscripten JavaScript 导入。

## WASI Component 导出接口

```wit
package srvpro3:plugin;

interface deck {
	classify-deck: func(deck: string) -> string;
}

world deck-plugin {
	export deck;
}
```

Component 必须导出 `srvpro3:plugin/deck` 接口及其 `classify-deck` 函数，接受一个包含下述输入 JSON 的 UTF-8 字符串，返回包含下述输出 JSON 的 UTF-8 字符串。由 Component 绑定管理内存，无须手动导出 `memory`、`allocate`、`release`。允许导入 WASI 0.2 标准接口。

WASI 实例不继承服务器的环境变量、命令行参数、标准输入输出或目录，不授予文件目录和网络访问权限。标准输入为 EOF，标准输出和标准错误丢弃；可以使用 WASI 时钟和随机数接口。

## 参数

`pointer` 和 `length` 按无符号 32 位整数解释。长度单位为字节。

`allocate` 分配至少 `length` 字节的缓冲区。

`classify_deck` 的输入缓冲区是 UTF-8 JSON，不包含末尾 NUL：

```typescript
interface Input {
	main : (Card | MissingCard)[];
	extra : (Card | MissingCard)[];
	side : (Card | MissingCard)[];
}

interface Card {
	code : number;       // u32 卡号
	alias : number;      // u32 同名卡号
	setcode : number[];  // 固定 16 项 u16 系列代码，0 为未使用
	card_type : number;  // u32 类型位掩码
	level : number;      // u32 等级、阶级或 Link 值
	attribute : number;  // u32 属性位掩码
	race : number;       // u32 种族位掩码
	attack : number;     // i32
	defense : number;    // i32
	lscale : number;     // u32 左灵摆刻度
	rscale : number;     // u32 右灵摆刻度
	link_marker : number; // u32 Link 箭头位掩码
	ot : number;        // u8 卡片适用范围标记
	category : number;  // u32 效果分类位掩码
}

interface MissingCard {
	code : number;
	missing : true;
}
```

每张实体卡对应一个对象，重复卡保留重复项及原顺序。卡片字段来自本次查询加载的服务器卡片数据快照，包括扩展资源覆盖后的数据；历史卡号在当前卡库中不存在时返回 `MissingCard`，不删除该卡。输入最多 4 MiB。

`release` 接收本模块分配的缓冲区指针及其原始字节长度。

## 返回值

`classify_deck` 的输出缓冲区是 UTF-8 JSON，不包含末尾 NUL：

```typescript
interface Output {
	id : string;
	name : string;
}
```

`id` 非空且最多 128 个 UTF-8 字节，`name` 非空且最多 100 个 Unicode 字符。同一 `id` 对应相同的 `name`。

返回值的打包格式：

```text
(u64(output_length) << 32) | u32(output_pointer)
```

输出长度为 `1..=65536` 字节，输出位于 `memory` 内，且不得与输入缓冲区重叠。
