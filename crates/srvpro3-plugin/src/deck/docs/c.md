# C 示例

## cJSON

```sh
git clone --depth 1 --branch v1.7.19 https://github.com/DaveGamble/cJSON.git
```

## classifier.c

```c
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

#define INPUT_LIMIT (4u * 1024u * 1024u)

typedef struct {
	uint32_t code;
	uint32_t alias;
	uint16_t setcode[16];
	uint32_t card_type;
	uint32_t level;
	uint32_t attribute;
	uint32_t race;
	int32_t attack;
	int32_t defense;
	uint32_t lscale;
	uint32_t rscale;
	uint32_t link_marker;
	uint8_t ot;
	uint32_t category;
	int missing;
} Card;

typedef struct {
	const char *id;
	const char *name;
} Classification;

uint32_t allocate(uint32_t length) {
	if (length == 0 || length > INPUT_LIMIT) return 0;
	return (uint32_t)(uintptr_t)malloc(length);
}

void release(uint32_t pointer, uint32_t length) {
	(void)length;
	free((void *)(uintptr_t)pointer);
}

static double read_number(const cJSON *value, double minimum, double maximum) {
	return cJSON_IsNumber(value) && value->valuedouble >= minimum && value->valuedouble <= maximum
		? value->valuedouble : 0;
}

static int read_card(const cJSON *object, Card *card) {
	if (!cJSON_IsObject(object)) return 0;
	*card = (Card){0};
	card->code = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "code"), 0, UINT32_MAX);
	card->alias = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "alias"), 0, UINT32_MAX);
	card->card_type = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "card_type"), 0, UINT32_MAX);
	card->level = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "level"), 0, UINT32_MAX);
	card->attribute = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "attribute"), 0, UINT32_MAX);
	card->race = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "race"), 0, UINT32_MAX);
	card->attack = (int32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "attack"), INT32_MIN, INT32_MAX);
	card->defense = (int32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "defense"), INT32_MIN, INT32_MAX);
	card->lscale = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "lscale"), 0, UINT32_MAX);
	card->rscale = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "rscale"), 0, UINT32_MAX);
	card->link_marker = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "link_marker"), 0, UINT32_MAX);
	card->ot = (uint8_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "ot"), 0, UINT8_MAX);
	card->category = (uint32_t)read_number(cJSON_GetObjectItemCaseSensitive(object, "category"), 0, UINT32_MAX);
	card->missing = cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(object, "missing"));
	const cJSON *setcode = cJSON_GetObjectItemCaseSensitive(object, "setcode");
	for (int i = 0; i < 16; ++i) {
		card->setcode[i] = (uint16_t)read_number(cJSON_GetArrayItem(setcode, i), 0, UINT16_MAX);
	}
	return card->code != 0;
}

static uint32_t count_main(const cJSON *deck) {
	const cJSON *main = cJSON_GetObjectItemCaseSensitive(deck, "main");
	if (!cJSON_IsArray(main)) return 0;
	uint32_t count = 0;
	const cJSON *object = NULL;
	cJSON_ArrayForEach(object, main) {
		Card card;
		if (!read_card(object, &card) || card.missing) continue;
		/* 所有卡片字段已解析，可直接通过 card.race、card.attribute 等进行分类。 */
		if (card.code == 89631139u) ++count;
	}
	return count;
}

/* 将 Classification 结构体序列化为 JSON，不手写输出字节。 */
static uint64_t output(const Classification *value) {
	cJSON *object = cJSON_CreateObject();
	if (object == NULL) return 0;
	if (cJSON_AddStringToObject(object, "id", value->id) == NULL ||
		cJSON_AddStringToObject(object, "name", value->name) == NULL) {
		cJSON_Delete(object);
		return 0;
	}
	char *json = cJSON_PrintUnformatted(object);
	cJSON_Delete(object);
	if (json == NULL) return 0;
	uint32_t length = (uint32_t)strlen(json);
	/* cJSON 默认使用 malloc/free，宿主通过 release 释放输出。 */
	return ((uint64_t)length << 32) | (uint32_t)(uintptr_t)json;
}

uint64_t classify_deck(uint32_t pointer, uint32_t length) {
	Classification result = { .id = "unclassified", .name = "未分类" };
	if (pointer == 0 || length == 0 || length > INPUT_LIMIT) return output(&result);
	cJSON *deck = cJSON_ParseWithLength((const char *)(uintptr_t)pointer, length);
	if (deck != NULL && count_main(deck) >= 3) {
		result.id = "blue-eyes";
		result.name = "青眼";
	}
	cJSON_Delete(deck);
	return output(&result);
}
```

## 构建（WASI SDK）

```sh
"$WASI_SDK_PATH/bin/clang" --sysroot="$WASI_SDK_PATH/share/wasi-sysroot" -mexec-model=reactor -O2 classifier.c cJSON/cJSON.c -IcJSON -lm -Wl,--export-memory -Wl,--export=allocate -Wl,--export=classify_deck -Wl,--export=release -Wl,--max-memory=67108864 -o deck-classifier.wasm
```
