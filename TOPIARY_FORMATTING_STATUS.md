# Topiary Formatting Integration - Status Report

## Summary

We've integrated Topiary as a formatter for `.actions` files, implementing a "cargo fmt" equivalent with two formatting modes (compact and list). The integration is ~90% complete.

## What Was Accomplished

### 1. Architecture Setup ✅
- Created `specifications/formatting_specification.md` with plain-English formatting rules
- Created `tree-sitter-actions/queries/actions/topiary.scm` query file
- Exported query via `TOPIARY_QUERY` constant in grammar bindings
- Added Topiary dependencies to CLI

### 2. FormatConfig Enhancement ✅
Added `include_id: bool` field to `FormatConfig` to control UUID output.

**Key Insight**: The parser auto-generates UUIDs as a side-effect (via `get_action_list_struct`), which is useful for the normal pipeline but pollutes formatting tests. By adding `include_id: false` to the formatter config, we can:
- Test formatter output without UUID variance
- Avoid needing regex redaction in snapshots
- Simplify the RON snapshot tests similarly in the future

This is cleaner than adding UUID-disabling config to the parser since valid `.actions` files don't require UUIDs from the grammar perspective.

### 3. Test Infrastructure ✅
- Created `tests/formatting.rs` with snapshot tests for both modes
- Tests use `include_id: false` for stable snapshots
- All 20 formatting snapshots generated and accepted
- Tests compile and run successfully

### 4. CLI Integration ✅
- Added `--style` and `--indent-width` flags to normalize command
- CLI defaults to `include_id: true` (normal behavior)
- Proper conversion between argparser types and lib types

## What's Not Working

### Anonymous Node Preservation ❌

**Problem**: Topiary is stripping regex-matched anonymous nodes from the parse tree.

**Symptoms**:
- Priority outputs as `!` instead of `!1` (number lost)
- Story outputs as `*` instead of `* Story Name` (name lost)
- Context works fine: `+office,computer` (because tags are named nodes marked `@leaf`)

**Root Cause**:
- Grammar defines priority as: `seq(field('icon', '!'), field('level', /[1-5]/))`
- The level is an anonymous regex-matched node
- Topiary strips anonymous nodes unless explicitly marked as `@leaf`
- We can't use `(priority (_) @leaf)` because tree-sitter says "Impossible pattern" when a node only has anonymous children

**What We Tried**:
1. `(priority (_) @leaf)` → "Impossible pattern" error
2. `(priority level: (_) @leaf)` → Parse error
3. `(_) @leaf` globally → Works but removes all spacing (too broad)

## What Needs to Be Done

### Fix the Anonymous Node Issue

**Option 1: Adjust Grammar (Cleaner)**
Change the grammar to make priority level and story name into named nodes instead of anonymous regex matches:

```javascript
// Current (anonymous):
priority: $ => seq(
  field('icon', '!'),
  field('level', /[1-5]/)
),

// Proposed (named):
priority: $ => seq(
  field('icon', '!'),
  field('level', $.priority_level)
),
priority_level: $ => /[1-5]/,
```

Then mark in topiary.scm:
```scheme
(priority_level) @leaf
(story_name) @leaf
```

**Option 2: Fix Topiary Query (Harder)**
Research how to properly mark anonymous regex nodes in Topiary queries. The Topiary documentation or examples from other grammars might show the pattern.

**Option 3: Post-Process (Hacky)**
Don't use Topiary for these fields - handle them specially in the formatter. Not recommended.

### Verify Roundtrip Tests

Once anonymous nodes are preserved:
```bash
cargo test roundtrip_action_with_everything
```

This test currently fails at line 272 because priority is lost during format→parse roundtrip.

### Test Both Formatting Modes

The current Topiary query only implements basic spacing (compact mode). List mode needs indentation scoping added:

```scheme
;; List mode: indent metadata under actions
(priority) @prepend_indent_start
(priority) @append_indent_end
```

But get the basic anonymous node issue fixed first.

## How to Test

### Quick Test
```bash
# Should output: [x] buy groceries !1 #uuid
cargo run -- normalize examples/with_priority.actions

# Should output: [x] story test * Test Story #uuid
cargo run -- normalize examples/with_story.actions
```

### Full Test Suite
```bash
cargo test

# Accept new snapshots if formatter output changes
cargo insta review
# or
cargo insta test --accept
```

### Check Specific Snapshots
```bash
cat tests/snapshots/formatting__compact_with_priority.snap
cat tests/snapshots/formatting__compact_with_story.snap
```

## Files Modified

### tree-sitter-actions/
- `queries/actions/topiary.scm` - Formatting query (needs anonymous node fix)
- `bindings/rust/lib.rs` - Added TOPIARY_QUERY export

### clearhead-cli/
- `src/format.rs` - Added FormatStyle, FormatConfig.include_id, Topiary integration
- `src/argparser.rs` - Added Style enum, normalize command flags
- `src/main.rs` - Wire up style flags (include_id: true for CLI)
- `tests/formatting.rs` - Snapshot tests (include_id: false for stability)
- `Cargo.toml` - Added topiary-core and topiary-tree-sitter-facade deps

### specifications/
- `formatting_specification.md` - Canonical formatting rules

## Key Learnings

1. **Side-effect separation**: Parser side-effects (UUID generation) should be separable from core parsing for testing purposes
2. **Config over workarounds**: Adding `include_id: false` is cleaner than regex redaction or fixture files with stable UUIDs
3. **Anonymous nodes**: Tree-sitter grammars using regex patterns create anonymous nodes that formatters can't easily preserve
4. **Named nodes are easier**: Context tags work perfectly because `tag` is a named node type

## Next Steps for Helper

1. Fix anonymous node preservation (try grammar changes first - see Option 1 above)
2. Rebuild grammar: `cd tree-sitter-actions && tree-sitter generate && cargo build`
3. Test roundtrip: `cd clearhead-cli && cargo test roundtrip_action_with_everything`
4. Accept new snapshots: `cargo insta review`
5. Implement list mode indentation in topiary.scm
6. Add LSP integration later (separate task)

## Questions?

Check:
- Topiary tutorial: https://www.tweag.io/blog/2025-01-30-topiary-tutorial-part-1/
- Tree-sitter query docs: https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries
- Our grammar: `tree-sitter-actions/grammar.js`
- Test with: `tree-sitter parse examples/with_priority.actions`
