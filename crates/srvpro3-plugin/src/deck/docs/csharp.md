# C# 示例（.NET 10+，WASI Component）

## 创建项目

```sh
dotnet new install BytecodeAlliance.Componentize.DotNet.Templates
dotnet new componentize.wasi.lib -o DeckClassifier
cd DeckClassifier
```

## component.wit（替换模板文件）

```wit
package srvpro3:plugin;

interface deck {
	classify-deck: func(deck: string) -> string;
}

world deck-plugin {
	export deck;
}
```

## 项目文件中的 WIT 配置

```xml
<!-- 保留模板生成的 SDK、包引用、NuGet 源和其他属性。替换模板原有的 Wit 配置。 -->
<ItemGroup>
	<Wit Update="component.wit" World="deck-plugin" />
</ItemGroup>
```

## DeckImpl.cs（替换模板实现类）

```csharp
using System.Text.Json;
using System.Text.Json.Serialization;

namespace DeckPluginWorld.wit.exports.srvpro3.plugin;

public sealed class Card
{
	public uint code { get; set; }
	public uint alias { get; set; }
	public ushort[] setcode { get; set; } = new ushort[16];
	public uint card_type { get; set; }
	public uint level { get; set; }
	public uint attribute { get; set; }
	public uint race { get; set; }
	public int attack { get; set; }
	public int defense { get; set; }
	public uint lscale { get; set; }
	public uint rscale { get; set; }
	public uint link_marker { get; set; }
	public byte ot { get; set; }
	public uint category { get; set; }
	public bool missing { get; set; }
}

public sealed class DeckInput
{
	public Card[] main { get; set; } = [];
	public Card[] extra { get; set; } = [];
	public Card[] side { get; set; } = [];
}

public sealed record Classification(string id, string name);

// 使用源生成的 JSON 元数据，兼容 NativeAOT，不依赖反射序列化。
[JsonSerializable(typeof(DeckInput))]
[JsonSerializable(typeof(Classification))]
internal partial class ClassifierJsonContext : JsonSerializerContext {}

public class DeckImpl : IDeck
{
	public static string ClassifyDeck(string deck)
	{
		DeckInput cards = JsonSerializer.Deserialize(deck, ClassifierJsonContext.Default.DeckInput)
			?? throw new JsonException("卡组不能为空");
		int count = 0;
		foreach (Card card in cards.main)
		{
			if (!card.missing && card.code == 89631139u) count++;
		}
		Classification result = count >= 3
			? new("blue-eyes", "青眼")
			: new("unclassified", "未分类");
		return JsonSerializer.Serialize(result, ClassifierJsonContext.Default.Classification);
	}
}
```

## 构建

```sh
dotnet build -c Release
```
