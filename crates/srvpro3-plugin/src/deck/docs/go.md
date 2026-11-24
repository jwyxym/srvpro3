# Go 示例（Go 1.24 或更高版本）

## go.mod

```go
module example.com/deck-classifier

go 1.24.0
```

## main.go

```go
package main

import (
	"encoding/json"
	"unsafe"
)

type Card struct {
	Code uint32 `json:"code"`
	Alias uint32 `json:"alias"`
	Setcode [16]uint16 `json:"setcode"`
	CardType uint32 `json:"card_type"`
	Level uint32 `json:"level"`
	Attribute uint32 `json:"attribute"`
	Race uint32 `json:"race"`
	Attack int32 `json:"attack"`
	Defense int32 `json:"defense"`
	Lscale uint32 `json:"lscale"`
	Rscale uint32 `json:"rscale"`
	LinkMarker uint32 `json:"link_marker"`
	Ot uint8 `json:"ot"`
	Category uint32 `json:"category"`
	Missing bool `json:"missing"`
}

type Deck struct {
	Main []Card `json:"main"`
	Extra []Card `json:"extra"`
	Side []Card `json:"side"`
}

type Classification struct {
	ID string `json:"id"`
	Name string `json:"name"`
}

// 保留切片引用，保证宿主使用指针期间不会被 GC 回收。
var buffers = map[uint32][]byte{}

func retain(buffer []byte) uint32 {
	if len(buffer) == 0 { return 0 }
	pointer := uint32(uintptr(unsafe.Pointer(&buffer[0])))
	buffers[pointer] = buffer
	return pointer
}

//go:wasmexport allocate
func allocate(length uint32) uint32 {
	if length == 0 || length > 4 * 1024 * 1024 { return 0 }
	return retain(make([]byte, int(length)))
}

//go:wasmexport release
func release(pointer uint32, length uint32) {
	if buffer, ok := buffers[pointer]; ok && uint32(len(buffer)) == length {
		delete(buffers, pointer)
	}
}

//go:wasmexport classify_deck
func classifyDeck(pointer uint32, length uint32) uint64 {
	input, ok := buffers[pointer]
	if !ok || length == 0 || uint32(len(input)) != length { return 0 }
	var deck Deck
	if json.Unmarshal(input, &deck) != nil { return 0 }
	count := 0
	for _, card := range deck.Main {
		if !card.Missing && card.Code == 89631139 { count++ }
	}
	result := Classification{ID: "unclassified", Name: "未分类"}
	if count >= 3 { result = Classification{ID: "blue-eyes", Name: "青眼"} }
	output, err := json.Marshal(result)
	if err != nil { return 0 }
	address := retain(output)
	return uint64(len(output)) << 32 | uint64(address)
}

func main() {}
```

## 构建（Bash）

```sh
GOOS=wasip1 GOARCH=wasm go build -buildmode=c-shared -o deck-classifier.wasm .
```

## 构建（PowerShell）

```powershell
$env:GOOS = "wasip1"
$env:GOARCH = "wasm"
go build -buildmode=c-shared -o deck-classifier.wasm .
```
