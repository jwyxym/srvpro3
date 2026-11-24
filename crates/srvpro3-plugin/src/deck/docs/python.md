# Python 示例（WASI Component）

## 安装与生成绑定（Python 3.10+）

```sh
python -m pip install componentize-py==0.19.3
componentize-py -d deck-plugin.wit -w deck-plugin bindings bindings
```

## deck-plugin.wit

```wit
package srvpro3:plugin;

interface deck {
	classify-deck: func(deck: string) -> string;
}

world deck-plugin {
	export deck;
}
```

## app.py

```python
import json
from dataclasses import asdict, dataclass, field
from wit_world import exports


@dataclass
class Card:
	code: int = 0
	alias: int = 0
	setcode: list[int] = field(default_factory=lambda: [0] * 16)
	card_type: int = 0
	level: int = 0
	attribute: int = 0
	race: int = 0
	attack: int = 0
	defense: int = 0
	lscale: int = 0
	rscale: int = 0
	link_marker: int = 0
	ot: int = 0
	category: int = 0
	missing: bool = False


@dataclass
class DeckInput:
	main: list[Card]
	extra: list[Card]
	side: list[Card]


@dataclass
class Classification:
	id: str
	name: str


class Deck(exports.Deck):
	def classify_deck(self, deck: str) -> str:
		value = json.loads(deck)
		fields = Card.__dataclass_fields__
		def section(name: str) -> list[Card]:
			return [Card(**{key: val for key, val in card.items() if key in fields})
				for card in value[name]]
		cards = DeckInput(section("main"), section("extra"), section("side"))
		count = sum(not card.missing and card.code == 89631139 for card in cards.main)
		result = Classification("blue-eyes", "青眼") if count >= 3 else Classification("unclassified", "未分类")
		return json.dumps(asdict(result), ensure_ascii=False, separators=(",", ":"))
```

## 构建（Bash）

```sh
PYTHONPATH=bindings componentize-py -d deck-plugin.wit -w deck-plugin componentize app -o deck-classifier.wasm
```

## 构建（PowerShell）

```powershell
$env:PYTHONPATH = "bindings"
componentize-py -d deck-plugin.wit -w deck-plugin componentize app -o deck-classifier.wasm
```
