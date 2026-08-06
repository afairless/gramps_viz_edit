# Implementation Plan: Gramps Diff Analyzer — Step 8

Source: `docs/research/gramps-diff-plan.md`

## Status

Steps 1–7 are complete. This plan covers **Step 8 only**.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 8 | `feat: add Pass 2 extrinsic/cascading diff resolution` | Cascading module | `crates/diff/src/cascading.rs` — `resolve_extrinsic(item_diffs: Vec<ItemDiff>, handle_map: &HashMap<Handle, Handle>) -> Vec<ItemDiff>`. For each matched pair (both `handle_a` and `handle_b` present), re-evaluate every `FieldChange` whose `field_kind` is `HandleRef` or `HandleRefList` against the handle map. A handle-ref change is **extrinsic** when the B-side handle, looked up through the handle map, equals the A-side handle — meaning the referenced item is the same, only the handle value changed. Non-handle-ref changes are always intrinsic. If a matched pair has **only** extrinsic handle-ref changes (no intrinsic changes), reclassify it from `Modified` to `ExtrinsicOnly`. Items with a mix of intrinsic and extrinsic changes remain `Modified`. Items classified as `Same`, `Added`, `Removed`, or `NeedsReview` pass through unchanged. Add `mod cascading;` and `pub use cascading::resolve_extrinsic;` to `crates/diff/src/lib.rs`. | Unit (extrinsic-only case: citation with same source_handle content but different handle value → handle_map resolves it → `ExtrinsicOnly`; intrinsic + extrinsic mix: same citation but page text also changed → remains `Modified`; no remap needed: identical handles and fields → unchanged `Same`; unmatched items pass through unchanged; edge cases: B-side handle not in handle_map → treated as intrinsic change, empty handle_map → all handle-ref changes are intrinsic, empty item_diffs → empty output) |
