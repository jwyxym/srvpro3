# Rust 示例

## Cargo.toml

```toml
[package]
name = "deck-classifier-rust"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
panic = "abort"
strip = true
```

## src/lib.rs

```rust
use std::{mem, ptr, slice};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Input {
	main: Vec<Card>,
	extra: Vec<Card>,
	side: Vec<Card>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Card {
	code: u32,
	alias: u32,
	setcode: [u16; 16],
	card_type: u32,
	level: u32,
	attribute: u32,
	race: u32,
	attack: i32,
	defense: i32,
	lscale: u32,
	rscale: u32,
	link_marker: u32,
	ot: u8,
	category: u32,
	// 卡库中不存在的卡只包含 code 和 missing，其余字段使用默认值。
	missing: bool,
}

#[derive(Serialize)]
struct Classification<'a> {
	id: &'a str,
	name: &'a str,
}

fn output(id: &str, name: &str) -> u64 {
	let result: Classification<'_> = Classification { id, name };
	let mut buffer: Box<[u8]> = serde_json::to_vec(&result)
		.expect("序列化分类结果失败")
		.into_boxed_slice();
	let length: u64 = buffer.len() as u64;
	let pointer: u64 = buffer.as_mut_ptr() as u32 as u64;
	mem::forget(buffer);
	(length << 32) | pointer
}

#[unsafe(no_mangle)]
pub extern "C" fn allocate(length: u32) -> *mut u8 {
	if length == 0 || length > 4 * 1024 * 1024 { return ptr::null_mut(); }
	let mut buffer: Box<[u8]> = vec![0; length as usize].into_boxed_slice();
	let pointer: *mut u8 = buffer.as_mut_ptr();
	mem::forget(buffer);
	pointer
}

// pointer 和 length 必须来自本模块分配的同一缓冲区，且只能释放一次。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn release(pointer: *mut u8, length: u32) {
	if pointer.is_null() { return; }
	unsafe {
		drop(Box::from_raw(ptr::slice_from_raw_parts_mut(pointer, length as usize)));
	}
}

// 输入缓冲区由宿主持有；此函数不能释放它。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn classify_deck(pointer: *const u8, length: u32) -> u64 {
	if pointer.is_null() || length == 0 || length > 4 * 1024 * 1024 { return output("unclassified", "未分类"); }
	let bytes: &[u8] = unsafe { slice::from_raw_parts(pointer, length as usize) };
	let input: Input = match serde_json::from_slice(bytes) {
		Ok(input) => input,
		Err(_) => return output("unclassified", "未分类"),
	};
	// 也可以通过 card.race、card.attribute 和 card.setcode 判断系列。
	let count: usize = input.main.iter().filter(|card: &&Card| !card.missing && card.code == 89631139).count();
	if count >= 3 { output("blue-eyes", "青眼") }
	else { output("unclassified", "未分类") }
}
```

## 构建

```sh
cargo build --release --target wasm32-unknown-unknown
```
