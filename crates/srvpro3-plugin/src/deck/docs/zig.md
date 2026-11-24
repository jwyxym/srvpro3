# Zig 示例

## classifier.zig

```zig
const std = @import("std");

var input_buffer: [4 * 1024 * 1024]u8 = undefined;
var output_buffer: [2048]u8 = undefined;
var scratch: [16 * 1024 * 1024]u8 = undefined;

const Classification = struct {
	id: []const u8,
	name: []const u8,
};

const Input = struct {
	main: []const Card,
	extra: []const Card = &.{},
	side: []const Card = &.{},
};

const Card = struct {
	code: u32,
	alias: u32 = 0,
	setcode: [16]u16 = [_]u16{0} ** 16,
	card_type: u32 = 0,
	level: u32 = 0,
	attribute: u32 = 0,
	race: u32 = 0,
	attack: i32 = 0,
	defense: i32 = 0,
	lscale: u32 = 0,
	rscale: u32 = 0,
	link_marker: u32 = 0,
	ot: u8 = 0,
	category: u32 = 0,
	// 卡库中不存在的卡只包含 code 和 missing，其余字段使用默认值。
	missing: bool = false,
};

export fn allocate(length: u32) u32 {
	if (length == 0 or length > input_buffer.len) return 0;
	return @intCast(@intFromPtr(&input_buffer));
}

export fn release(pointer: u32, length: u32) void {
	_ = pointer;
	_ = length;
	// 输入和输出均使用静态存储，不执行堆分配。
}

fn output(value: Classification) u64 {
	var writer: std.Io.Writer = .fixed(&output_buffer);
	std.json.Stringify.value(value, .{}, &writer) catch return 0;
	const bytes: []const u8 = writer.buffered();
	return (@as(u64, @intCast(bytes.len)) << 32) |
		@as(u64, @intCast(@intFromPtr(bytes.ptr)));
}

export fn classify_deck(pointer: u32, length: u32) u64 {
	if (pointer != @as(u32, @intCast(@intFromPtr(&input_buffer))) or
		length == 0 or length > input_buffer.len)
	{
		return output(.{ .id = "unclassified", .name = "未分类" });
	}

	// 解析过程使用独立的固定缓冲区，每次调用重新初始化。
	var allocator = std.heap.FixedBufferAllocator.init(&scratch);
	const parsed = std.json.parseFromSlice(
		Input,
		allocator.allocator(),
		input_buffer[0..length],
		.{ .ignore_unknown_fields = true },
	) catch return output(.{ .id = "unclassified", .name = "未分类" });
	defer parsed.deinit();

	var count: usize = 0;
	for (parsed.value.main) |card| {
		if (!card.missing and card.code == 89631139) count += 1;
	}
	return output(if (count >= 3)
		.{ .id = "blue-eyes", .name = "青眼" }
	else
		.{ .id = "unclassified", .name = "未分类" });
}
```

## 构建

```sh
zig build-exe classifier.zig -target wasm32-freestanding -O ReleaseSmall -fno-entry -rdynamic --export-memory -femit-bin=deck-classifier.wasm
```
