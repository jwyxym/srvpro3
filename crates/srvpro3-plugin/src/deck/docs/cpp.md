# C++ 示例（WASI SDK）

## 依赖

```sh
curl -L https://github.com/nlohmann/json/releases/download/v3.12.0/json.hpp -o json.hpp
```

## classifier.cpp

```cpp
#define JSON_NOEXCEPTION 1
#include "json.hpp"
#include <array>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

using nlohmann::json;

struct Card {
	uint32_t code = 0;
	uint32_t alias = 0;
	std::array<uint16_t, 16> setcode{};
	uint32_t card_type = 0;
	uint32_t level = 0;
	uint32_t attribute = 0;
	uint32_t race = 0;
	int32_t attack = 0;
	int32_t defense = 0;
	uint32_t lscale = 0;
	uint32_t rscale = 0;
	uint32_t link_marker = 0;
	uint8_t ot = 0;
	uint32_t category = 0;
	bool missing = false;
};

NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(Card, code, alias, setcode,
	card_type, level, attribute, race, attack, defense, lscale, rscale,
	link_marker, ot, category, missing)

struct Deck {
	std::vector<Card> main;
	std::vector<Card> extra;
	std::vector<Card> side;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE_WITH_DEFAULT(Deck, main, extra, side)

struct Classification {
	std::string id;
	std::string name;
};
NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE(Classification, id, name)

extern "C" uint32_t allocate(uint32_t length) {
	if (length == 0 || length > 4 * 1024 * 1024) return 0;
	return static_cast<uint32_t>(reinterpret_cast<uintptr_t>(std::malloc(length)));
}

extern "C" void release(uint32_t pointer, uint32_t length) {
	(void)length;
	std::free(reinterpret_cast<void *>(static_cast<uintptr_t>(pointer)));
}

extern "C" uint64_t classify_deck(uint32_t pointer, uint32_t length) {
	if (pointer == 0 || length == 0 || length > 4 * 1024 * 1024) return 0;
	const char *input = reinterpret_cast<const char *>(static_cast<uintptr_t>(pointer));
	json parsed = json::parse(input, input + length, nullptr, false);
	if (parsed.is_discarded() || !parsed.is_object()) return 0;
	Deck deck = parsed.get<Deck>();
	uint32_t count = 0;
	for (const Card &card : deck.main) {
		if (!card.missing && card.code == 89631139u) ++count;
	}
	Classification result = count >= 3
		? Classification{"blue-eyes", "青眼"}
		: Classification{"unclassified", "未分类"};
	std::string output = json(result).dump();
	uint32_t address = allocate(static_cast<uint32_t>(output.size()));
	if (address == 0) return 0;
	std::memcpy(reinterpret_cast<void *>(static_cast<uintptr_t>(address)), output.data(), output.size());
	return (static_cast<uint64_t>(output.size()) << 32) | address;
}
```

## 构建（Bash）

```sh
"$WASI_SDK_PATH/bin/clang++" --target=wasm32-wasip1 --sysroot="$WASI_SDK_PATH/share/wasi-sysroot" -std=c++17 -O2 -fno-exceptions -mexec-model=reactor classifier.cpp -Wl,--export-memory -Wl,--export=allocate -Wl,--export=classify_deck -Wl,--export=release -Wl,--max-memory=67108864 -o deck-classifier.wasm
```

## 构建（Visual Studio 开发者 PowerShell）

```powershell
# 可以使用 Visual Studio 编辑；构建使用 WASI SDK 的 Clang，不使用 MSVC cl.exe/link.exe。
& "$env:WASI_SDK_PATH/bin/clang++.exe" --target=wasm32-wasip1 "--sysroot=$env:WASI_SDK_PATH/share/wasi-sysroot" -std=c++17 -O2 -fno-exceptions -mexec-model=reactor classifier.cpp "-Wl,--export-memory" "-Wl,--export=allocate" "-Wl,--export=classify_deck" "-Wl,--export=release" "-Wl,--max-memory=67108864" -o deck-classifier.wasm
```
