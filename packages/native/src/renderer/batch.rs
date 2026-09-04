//! Turning one batch of React mutations into tree operations.
//!
//! Every op is decoded and every style is resolved before any of them touches
//! the tree, so a malformed batch changes nothing rather than applying half of
//! itself.

use std::sync::Arc;

use super::raw_element_id;
use crate::retained_tree::{RetainedTree, StyleTable};
use crate::style::StyleDesc;

/// Parsed batch operation — typed enum for atomic validation.
/// All ops are parsed and validated BEFORE any tree mutation occurs.
/// This prevents partial application on malformed batches.
enum BatchOp<'a> {
    CreateElement {
        id: u64,
        element_type: String,
    },
    DestroyElement {
        id: u64,
    },
    AppendChild {
        parent_id: u64,
        child_id: u64,
    },
    InsertBefore {
        parent_id: u64,
        child_id: u64,
        before_id: u64,
    },
    /// The payload stays as raw JSON until apply time.
    ///
    /// Two reasons. A parsed `StyleDesc` is ~1.4 KB, and a `Vec<BatchOp>` is as
    /// wide as its widest variant, so inlining one made a 220k-op mount reserve
    /// over 300 MB before it parsed a single op. And the tree hash-conses
    /// styles by content, so it needs the bytes: hashing ~110 bytes is far
    /// cheaper than building 80 `Option` fields and throwing 99.8% of them away.
    SetStyle {
        id: u64,
        style: &'a serde_json::value::RawValue,
    },
    SetText {
        id: u64,
        content: String,
    },
    SetEventListener {
        id: u64,
        event_type: String,
        has_handler: bool,
    },
    SetRoot {
        id: u64,
    },
    SetCustomProp {
        id: u64,
        key: String,
        value: serde_json::Value,
    },
}

/// A batch failure. The message names the op index, so it survives the trip
/// back to JS as a plain `Error`.
pub type BatchResult<T> = std::result::Result<T, String>;

/// Decode the batch straight from its JSON bytes into `Vec<BatchOp>`.
///
/// There is deliberately no `Vec<serde_json::Value>` in between. That tree cost
/// a `String` per key and per value, every payload was then deep-cloned out of
/// it, and `from_value` parsed the clone a second time, so one style was
/// allocated three times. A 220k-op mount made 1.5M allocations that way.
///
/// Everything the `Value` version guaranteed still holds, and each one is
/// load-bearing:
///
/// * an unknown opcode is a hard error, not a skipped op. Silently ignoring one
///   would let a JS/Rust version skew desync the tree instead of throwing
/// * ids go through `raw_element_id`, so non-finite, negative, fractional and
///   out-of-safe-range values are still rejected
/// * `hasHandler` is accepted as a bool or a number
/// * errors still name the op index. `serde_json` reports a byte offset, which
///   is useless when you are chasing a desync
fn parse_batch_ops(bytes: &[u8]) -> BatchResult<Vec<BatchOp<'_>>> {
    serde_json::from_slice::<BatchOps>(bytes)
        .map(|batch| batch.0)
        .map_err(|error| format!("Failed to parse batch: {error}"))
}

struct BatchOps<'a>(Vec<BatchOp<'a>>);

impl<'de> serde::Deserialize<'de> for BatchOps<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct OpsVisitor;

        impl<'de> serde::de::Visitor<'de> for OpsVisitor {
            type Value = BatchOps<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an array of mutation tuples")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<BatchOps<'de>, A::Error> {
                let mut ops = Vec::with_capacity(seq.size_hint().unwrap_or(64));
                loop {
                    // The index is attached here because this is the only place
                    // that knows it.
                    let index = ops.len();
                    match seq.next_element::<BatchOp<'de>>() {
                        Ok(Some(op)) => ops.push(op),
                        Ok(None) => break,
                        Err(error) => {
                            return Err(serde::de::Error::custom(format!(
                                "Batch op {index}: {error}"
                            )))
                        }
                    }
                }
                Ok(BatchOps(ops))
            }
        }

        deserializer.deserialize_seq(OpsVisitor)
    }
}

/// A string argument, borrowed from the input when the JSON has no escapes.
///
/// The owned copy happens exactly once, on the way into the `BatchOp`. The
/// `Value` path allocated twice: into `Value::String`, then into the op.
struct StrArg<'a>(std::borrow::Cow<'a, str>);

impl<'de> serde::Deserialize<'de> for StrArg<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        use std::borrow::Cow;
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = StrArg<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                v: &'de str,
            ) -> std::result::Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Borrowed(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Owned(v)))
            }
        }
        deserializer.deserialize_str(V)
    }
}

fn next_arg<'de, A, T>(seq: &mut A, what: &str) -> std::result::Result<T, A::Error>
where
    A: serde::de::SeqAccess<'de>,
    T: serde::Deserialize<'de>,
{
    seq.next_element()?
        .ok_or_else(|| serde::de::Error::custom(format!("missing {what}")))
}

/// Read an element id. Ids cross napi as JS numbers, so they are read as `f64`
/// and validated exactly as `batch_id` did.
fn next_id<'de, A: serde::de::SeqAccess<'de>>(
    seq: &mut A,
    what: &str,
) -> std::result::Result<u64, A::Error> {
    let raw: f64 = next_arg(seq, what)?;
    raw_element_id(raw).map_err(serde::de::Error::custom)
}

impl<'de> serde::Deserialize<'de> for BatchOp<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct V;

        impl<'de> serde::de::Visitor<'de> for V {
            type Value = BatchOp<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a [opcode, ...args] mutation tuple")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<BatchOp<'de>, A::Error> {
                let name: StrArg<'de> = next_arg(&mut seq, "op name")?;
                let op = match name.0.as_ref() {
                    "createElement" => BatchOp::CreateElement {
                        id: next_id(&mut seq, "id")?,
                        element_type: next_arg::<A, StrArg>(&mut seq, "element type")?
                            .0
                            .into_owned(),
                    },
                    "destroyElement" => BatchOp::DestroyElement {
                        id: next_id(&mut seq, "id")?,
                    },
                    "appendChild" => BatchOp::AppendChild {
                        parent_id: next_id(&mut seq, "parent id")?,
                        child_id: next_id(&mut seq, "child id")?,
                    },
                    "insertBefore" => BatchOp::InsertBefore {
                        parent_id: next_id(&mut seq, "parent id")?,
                        child_id: next_id(&mut seq, "child id")?,
                        before_id: next_id(&mut seq, "before id")?,
                    },
                    "setStyle" => BatchOp::SetStyle {
                        id: next_id(&mut seq, "id")?,
                        style: next_arg(&mut seq, "style")?,
                    },
                    "setText" => BatchOp::SetText {
                        id: next_id(&mut seq, "id")?,
                        content: next_arg::<A, StrArg>(&mut seq, "text")?.0.into_owned(),
                    },
                    "setEventListener" => BatchOp::SetEventListener {
                        id: next_id(&mut seq, "id")?,
                        event_type: next_arg::<A, StrArg>(&mut seq, "event type")?.0.into_owned(),
                        has_handler: next_arg(&mut seq, "hasHandler")?,
                    },
                    "setRoot" => BatchOp::SetRoot {
                        id: next_id(&mut seq, "id")?,
                    },
                    "setCustomProp" => BatchOp::SetCustomProp {
                        id: next_id(&mut seq, "id")?,
                        key: next_arg::<A, StrArg>(&mut seq, "prop key")?.0.into_owned(),
                        value: next_arg(&mut seq, "custom prop value")?,
                    },
                    other => {
                        return Err(serde::de::Error::custom(format!(
                            "unknown operation: {other:?}"
                        )))
                    }
                };
                // Trailing arguments are tolerated, as they were when the op was
                // an indexed array.
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(op)
            }
        }

        deserializer.deserialize_seq(V)
    }
}

/// Turn one raw `setStyle` payload into a shared style.
///
/// The reconciler sends an object. Anything else, `null` included, is handed
/// to `StyleDesc` and rejected there. Interning the raw bytes here keeps them
/// available for the content hash.
fn intern_style_payload(
    styles: &mut StyleTable,
    payload: &serde_json::value::RawValue,
) -> BatchResult<Arc<StyleDesc>> {
    styles.intern(payload.get().trim().as_bytes())
}

/// Resolve every `setStyle` payload in the batch, in op order.
///
/// This is the last fallible step, so it runs before the apply loop and borrows
/// only the style table. The borrow checker then proves no element was touched
/// when it returns `Err`, which is what makes a batch atomic. An earlier
/// version interned inside the apply loop, so a malformed style at the end of a
/// batch left everything before it applied and then threw.
fn resolve_styles(styles: &mut StyleTable, ops: &[BatchOp<'_>]) -> BatchResult<Vec<Arc<StyleDesc>>> {
    let mut resolved = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        if let BatchOp::SetStyle { style, .. } = op {
            let shared = intern_style_payload(styles, style)
                .map_err(|error| format!("Batch op {index} setStyle parse error: {error}"))?;
            resolved.push(shared);
        }
    }
    Ok(resolved)
}

/// Apply a batch of mutation tuples to a RetainedTree.
/// Shared between GpuixRenderer::apply_batch and TestGpuixRenderer::apply_batch.
/// Returns accumulated destroyed IDs (as f64) from all destroyElement ops.
///
/// ATOMIC: the batch is decoded and every style is resolved before a single
/// element is touched. If any op is malformed the tree is left unchanged and an
/// error is returned. Nothing after that point can fail, so JS and Rust cannot
/// desync when a batch is retried.
///
/// Batch format: JSON array of tuples [opcode, ...args].
/// See GpuixRenderer::apply_batch for opcode documentation.
///
/// Public so `examples/bench_serde.rs` times this exact function. A replica in
/// the bench would drift, and the numbers would then describe code nobody runs.
pub fn apply_batch_to_tree(tree: &mut RetainedTree, bytes: &[u8]) -> BatchResult<Vec<f64>> {
    // Phase 1: decode. No mutation.
    let parsed = parse_batch_ops(bytes)?;

    // Phase 2: resolve styles. Touches the style table only; a failure here
    // sweeps back out whatever this call interned.
    let styles = resolve_styles(&mut tree.styles, &parsed)
        .inspect_err(|_| tree.styles.sweep())?;
    let mut styles = styles.into_iter();

    // Phase 3: apply. Cannot fail.
    let mut destroyed_ids: Vec<f64> = Vec::new();
    for batch_op in parsed {
        match batch_op {
            BatchOp::CreateElement { id, element_type } => {
                tree.create_element(id, element_type);
            }
            BatchOp::DestroyElement { id } => {
                let destroyed = tree.destroy_element(id);
                destroyed_ids.extend(destroyed.iter().map(|&id| id as f64));
            }
            BatchOp::AppendChild {
                parent_id,
                child_id,
            } => {
                tree.append_child(parent_id, child_id);
            }
            BatchOp::InsertBefore {
                parent_id,
                child_id,
                before_id,
            } => {
                tree.insert_before(parent_id, child_id, before_id);
            }
            BatchOp::SetStyle { id, .. } => {
                let shared = styles.next().expect("one resolved style per setStyle op");
                tree.set_style(id, shared);
            }
            BatchOp::SetText { id, content } => {
                tree.set_text(id, content);
            }
            BatchOp::SetEventListener {
                id,
                event_type,
                has_handler,
            } => {
                tree.set_event_listener(id, event_type, has_handler);
            }
            BatchOp::SetRoot { id } => {
                tree.root_id = Some(id);
            }
            BatchOp::SetCustomProp { id, key, value } => {
                tree.set_custom_prop(id, key, value);
            }
        }
    }

    // Release styles nothing references any more. Without this a dragged
    // element, which produces a distinct style every frame, would grow the
    // table for as long as the app runs. The element count is what catches the
    // opposite case, a batch that destroyed most of the tree.
    let live_elements = tree.elements.len();
    tree.styles.maybe_sweep(live_elements);

    Ok(destroyed_ids)
}
