# Lua 示例（Lua 5.4，WASI SDK 26+）

## 依赖（Bash）

```sh
curl -L https://www.lua.org/ftp/lua-5.4.8.tar.gz -o lua.tar.gz
tar -xf lua.tar.gz
curl -L https://raw.githubusercontent.com/rxi/json.lua/master/json.lua -o json.lua
```

## 依赖（Windows CMD）

```bat
curl.exe -L https://www.lua.org/ftp/lua-5.4.8.tar.gz -o lua.tar.gz
tar -xf lua.tar.gz
curl.exe -L https://raw.githubusercontent.com/rxi/json.lua/master/json.lua -o json.lua
```

## classifier.lua

```lua
local Card = {}
Card.__index = Card

function Card.new(value)
	local card = {
		code = value.code or 0,
		alias = value.alias or 0,
		setcode = value.setcode or {},
		card_type = value.card_type or 0,
		level = value.level or 0,
		attribute = value.attribute or 0,
		race = value.race or 0,
		attack = value.attack or 0,
		defense = value.defense or 0,
		lscale = value.lscale or 0,
		rscale = value.rscale or 0,
		link_marker = value.link_marker or 0,
		ot = value.ot or 0,
		category = value.category or 0,
		missing = value.missing == true,
	}
	for i = 1, 16 do
		card.setcode[i] = card.setcode[i] or 0
	end
	return setmetatable(card, Card)
end

local function section(values)
	local cards = {}
	for i, value in ipairs(values) do cards[i] = Card.new(value) end
	return cards
end

function classify_json(input)
	local value = json.decode(input)
	local deck = {
		main = section(value.main),
		extra = section(value.extra),
		side = section(value.side),
	}
	local count = 0
	for _, card in ipairs(deck.main) do
		if not card.missing and card.code == 89631139 then count = count + 1 end
	end
	local result = { id = "unclassified", name = "未分类" }
	if count >= 3 then result = { id = "blue-eyes", name = "青眼" } end
	return json.encode(result)
end
```

## embed.py

```python
from pathlib import Path

with Path("embedded.h").open("w", encoding="utf-8") as output:
	for name, path in (("json_source", "json.lua"), ("classifier_source", "classifier.lua")):
		data = Path(path).read_bytes()
		output.write(f"static const unsigned char {name}[] = {{")
		output.write(",".join(str(byte) for byte in data))
		output.write(",0};\n")
```

## bridge.c

```c
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
#include "embedded.h"

uint32_t allocate(uint32_t length) {
	if (length == 0 || length > 4 * 1024 * 1024) return 0;
	return (uint32_t)(uintptr_t)malloc(length);
}

void release(uint32_t pointer, uint32_t length) {
	(void)length;
	free((void *)(uintptr_t)pointer);
}

/* 每次调用创建独立解释器；脚本和 JSON 库均内嵌，不从服务器读取文件。 */
uint64_t classify_deck(uint32_t pointer, uint32_t length) {
	if (pointer == 0 || length == 0 || length > 4 * 1024 * 1024) return 0;
	lua_State *state = luaL_newstate();
	if (state == NULL) return 0;
	luaL_requiref(state, "_G", luaopen_base, 1); lua_pop(state, 1);
	luaL_requiref(state, "table", luaopen_table, 1); lua_pop(state, 1);
	luaL_requiref(state, "string", luaopen_string, 1); lua_pop(state, 1);
	luaL_requiref(state, "math", luaopen_math, 1); lua_pop(state, 1);
	uint64_t packed = 0;
	if (luaL_loadbuffer(state, (const char *)json_source, sizeof(json_source) - 1, "json") != LUA_OK ||
		lua_pcall(state, 0, 1, 0) != LUA_OK) goto done;
	lua_setglobal(state, "json");
	if (luaL_loadbuffer(state, (const char *)classifier_source, sizeof(classifier_source) - 1, "classifier") != LUA_OK ||
		lua_pcall(state, 0, 0, 0) != LUA_OK) goto done;
	lua_getglobal(state, "classify_json");
	lua_pushlstring(state, (const char *)(uintptr_t)pointer, length);
	if (lua_pcall(state, 1, 1, 0) != LUA_OK || lua_type(state, -1) != LUA_TSTRING) goto done;
	size_t output_length = 0;
	const char *output = lua_tolstring(state, -1, &output_length);
	if (output_length == 0 || output_length > 65536) goto done;
	uint32_t address = allocate((uint32_t)output_length);
	if (address == 0) goto done;
	memcpy((void *)(uintptr_t)address, output, output_length);
	packed = ((uint64_t)output_length << 32) | address;
done:
	lua_close(state);
	return packed;
}
```

## 构建（Bash）

```sh
python embed.py
mkdir -p build
for source in lua-5.4.8/src/*.c; do
	case "$source" in */lua.c|*/luac.c) continue ;; esac
	"$WASI_SDK_PATH/bin/clang" --target=wasm32-wasip1 -O2 -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false -Ilua-5.4.8/src -c "$source" -o "build/$(basename "$source" .c).o"
done
"$WASI_SDK_PATH/bin/clang" --target=wasm32-wasip1 -O2 -mexec-model=reactor -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false -Ilua-5.4.8/src bridge.c build/*.o -lsetjmp -lm -Wl,--export-memory -Wl,--export=allocate -Wl,--export=classify_deck -Wl,--export=release -Wl,--max-memory=67108864 -o deck-classifier.wasm
```

## build.cmd（Windows CMD，保存为文件后运行）

```bat
@echo off
setlocal

rem WASI_SDK_PATH 指向 Windows 版 WASI SDK 26+ 的根目录。
if not defined WASI_SDK_PATH (
	echo Please set WASI_SDK_PATH before running this script.
	exit /b 1
)

python embed.py
if errorlevel 1 exit /b 1
if not exist build mkdir build
if errorlevel 1 exit /b 1

rem 使用响应文件传递对象文件列表，不依赖 CMD 展开 build\*.o。
type nul > build\lua-objects.rsp
for %%F in (lua-5.4.8\src\*.c) do (
	if /I not "%%~nxF"=="lua.c" if /I not "%%~nxF"=="luac.c" (
		"%WASI_SDK_PATH%\bin\clang.exe" --target=wasm32-wasip1 -O2 -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false -Ilua-5.4.8\src -c "%%F" -o "build\%%~nF.o"
		if errorlevel 1 exit /b 1
		>> build\lua-objects.rsp echo "build/%%~nF.o"
	)
)

"%WASI_SDK_PATH%\bin\clang.exe" --target=wasm32-wasip1 -O2 -mexec-model=reactor -mllvm -wasm-enable-sjlj -mllvm -wasm-use-legacy-eh=false -Ilua-5.4.8\src bridge.c @build\lua-objects.rsp -lsetjmp -lm -Wl,--export-memory -Wl,--export=allocate -Wl,--export=classify_deck -Wl,--export=release -Wl,--max-memory=67108864 -o deck-classifier.wasm
if errorlevel 1 exit /b 1

echo Built deck-classifier.wasm
endlocal
```

## 构建（Windows CMD）

```bat
set "WASI_SDK_PATH=C:\wasi-sdk"
build.cmd
```
