---
"@gpuix/native": patch
---

Read a `style` prop without buffering it, and stop carrying it by value.

`StyleDesc` used `#[serde(flatten)]` to collect custom properties, and `Numeric`
and `FontWeightValue` used `#[serde(untagged)]`. Each of those makes serde read
the whole value into an intermediate tree before it looks at one field, and
every `setStyle` call paid for it. All three now have a hand written
`Deserialize`. A macro declares `StyleDesc` and its reader from one field list,
so the name JS writes and the name Rust reads come from the same literal.

The struct is 1,728 bytes, so the read now writes straight into a box rather
than building on the stack and copying. Measured over 200,000 parses:

| shape | before | after | no flatten, no untagged |
| --- | --- | --- | --- |
| two fields | 320 ns | 84 ns | 74 ns |
| eleven fields | 531 ns | 399 ns | 341 ns |

A retained element holds that box instead of the struct, which takes it from
2,000 bytes to 280. A tree of 10,000 elements was carrying 17 MB of mostly
empty styles. Each op in a batched mutation shrank the same way.

The wire format is unchanged.
