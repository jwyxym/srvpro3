# Kotlin 示例（Kotlin/Wasm-WASI，JDK 17+，Gradle 8.14+）

## settings.gradle.kts

```kotlin
pluginManagement {
	repositories {
		gradlePluginPortal()
		mavenCentral()
	}
}
rootProject.name = "deck-classifier"
```

## build.gradle.kts

```kotlin
plugins {
	kotlin("multiplatform") version "2.3.0"
	kotlin("plugin.serialization") version "2.3.0"
}

repositories { mavenCentral() }

kotlin {
	wasmWasi {
		binaries.executable()
		compilerOptions {
			freeCompilerArgs.add("-Xwasm-use-new-exception-proposal")
		}
	}
	sourceSets {
		wasmWasiMain.dependencies {
			implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
		}
	}
}
```

## src/wasmWasiMain/kotlin/Classifier.kt

```kotlin
@file:OptIn(
	kotlin.wasm.ExperimentalWasmInterop::class,
	kotlin.wasm.unsafe.UnsafeWasmMemoryApi::class
)

import kotlin.coroutines.*
import kotlin.wasm.WasmExport
import kotlin.wasm.unsafe.Pointer
import kotlin.wasm.unsafe.withScopedMemoryAllocator
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

@Serializable
data class Card(
	val code: Long = 0,
	val alias: Long = 0,
	val setcode: List<Int> = List(16) { 0 },
	val card_type: Long = 0,
	val level: Long = 0,
	val attribute: Long = 0,
	val race: Long = 0,
	val attack: Int = 0,
	val defense: Int = 0,
	val lscale: Long = 0,
	val rscale: Long = 0,
	val link_marker: Long = 0,
	val ot: Int = 0,
	val category: Long = 0,
	val missing: Boolean = false
)

@Serializable
data class Deck(val main: List<Card>, val extra: List<Card>, val side: List<Card>)

@Serializable
data class Classification(val id: String, val name: String)

private val json = Json { ignoreUnknownKeys = true }

private class Session(val length: Int) {
	var input: Int = 0
	var packed: Long = 0
	var next: Continuation<Unit>? = null
	var finished: Boolean = false
	var classified: Boolean = false
	var failure: Throwable? = null

	fun advance() {
		val continuation = next ?: return
		next = null
		continuation.resume(Unit)
	}
}

private var active: Session? = null

/*
 * allocator 的作用域跨两次挂起保持存活：allocate -> classify_deck -> release(input)。
 * 不能在 withScopedMemoryAllocator 返回后继续使用其中分配的指针。
 * 不使用 Component Model 内部的 realloc API。
 */
private suspend fun runSession(session: Session) {
	withScopedMemoryAllocator { allocator ->
		session.input = allocator.allocate(session.length).address.toInt()
		suspendCoroutine<Unit> { session.next = it }
		val input = ByteArray(session.length) { index ->
			Pointer(session.input.toUInt() + index.toUInt()).loadByte()
		}
		val deck = json.decodeFromString<Deck>(input.decodeToString(throwOnInvalidSequence = true))
		val count = deck.main.count { !it.missing && it.code == 89631139L }
		val result = if (count >= 3) Classification("blue-eyes", "青眼")
			else Classification("unclassified", "未分类")
		val output = json.encodeToString(result).encodeToByteArray()
		require(output.size in 1..65536)
		val address = allocator.allocate(output.size).address
		output.forEachIndexed { index, byte -> Pointer(address + index.toUInt()).storeByte(byte) }
		session.packed = (output.size.toLong() shl 32) or address.toLong()
		suspendCoroutine<Unit> { session.next = it }
	}
}

@WasmExport("allocate")
fun allocate(length: Int): Int {
	if (length !in 1..4 * 1024 * 1024 || active != null) return 0
	val session = Session(length)
	active = session
	val task: suspend () -> Unit = { runSession(session) }
	task.startCoroutine(object : Continuation<Unit> {
		override val context: CoroutineContext = EmptyCoroutineContext
		override fun resumeWith(result: Result<Unit>) {
			session.finished = true
			session.failure = result.exceptionOrNull()
		}
	})
	if (session.finished) { active = null; return 0 }
	return session.input
}

@WasmExport("classify_deck")
fun classifyDeck(pointer: Int, length: Int): Long {
	val session = active ?: return 0
	if (pointer != session.input || length != session.length || session.classified) return 0
	session.classified = true
	session.advance()
	return if (session.failure == null) session.packed else 0
}

@WasmExport("release")
fun release(pointer: Int, length: Int) {
	val session = active ?: return
	// 宿主先 release(output)，再 release(input)；最后一次释放时关闭整个作用域。
	if (pointer == session.input && length == session.length) {
		if (!session.finished) session.advance()
		active = null
	}
}

fun main() {}
```

## 构建

```sh
gradle wasmWasiProductionExecutableCompileSync
```
