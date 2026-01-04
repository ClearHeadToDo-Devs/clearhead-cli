# Linting Test Examples

This directory contains canonical linting test cases for `.actions` files. These examples demonstrate the linting errors and their correct fixes as defined in the linting specification.

## Purpose

These test cases serve as:
1. **Specification artifacts** - Executable demonstrations of linting rules defined in [linting_specification.md](../../linting_specification.md)
2. **Implementation tests** - Used by linter implementations (CLI, LSP server) to verify correctness via snapshot testing
3. **Documentation** - Show developers what each linting rule detects and how to fix violations

## Structure

Each test case is a directory containing two files:
- `error.actions` - Valid syntax but contains linting violations
- `fixed.actions` - The same actions corrected to pass the linting rule

```
linting/
  E001_completed_without_date/
    error.actions           # [x] actions without %
    fixed.actions           # [x] actions with valid %
  E002_date_without_completed/
    error.actions           # [ ] actions with %
    fixed.actions           # [x] actions with %
  ...
```

## Rule Categories

### Error-Level Rules (E001-E016)

These rules detect semantic correctness issues that likely indicate mistakes:

| Rule | Name | Description |
|------|------|-------------|
| E001 | Completed State Requires Completed Date | Actions with `[x]` must have `%` |
| E002 | Completed Date Requires Completed State | Actions with `%` must have `[x]` |
| E003 | Invalid Priority Level | Priority must be 1-5 |
| E004 | Invalid UUID Format | UUIDs must follow standard format |
| E005 | Duplicate ID | Action IDs must be unique |
| E006 | Duration Without Do-Date | `D` requires `@` |
| E007 | Recurrence Without Do-Date | `R:` requires `@` |
| E008 | Empty Context Tag | Context tags cannot be empty |
| E009 | Hierarchy Depth Exceeded | Maximum depth is 5 levels |
| E010 | Orphaned Child Marker | Children must have proper parents |
| E011 | Skipped Hierarchy Level | Cannot skip depth levels |
| E012 | Completed Parent with Uncompleted Children | `[x]` parent cannot have `[ ]` children |
| E013 | Uncompleted Parent with All Children Completed | Parent should be `[x]` if all children are |
| E014 | Missing Creation Date | Actions should have `^` or UUID v7 |
| E015 | Creation Date in Future | `^` cannot be in the future |
| E016 | Completion Before Creation | `%` cannot be before `^` |

### Warning-Level Rules (W001-W004)

These rules detect suspicious patterns that may need review:

| Rule | Name | Description |
|------|------|-------------|
| W001 | Past Do-Date on Not-Started Action | `[ ]` actions with very old `@` dates |
| W002 | Completed Before Scheduled | `%` significantly before `@` |
| W003 | Excessive Duration | Very long `D` values may be errors |
| W004 | Very Old Action | Not-started actions with ancient `@` dates |

### Info-Level Rules (I001-I010)

These rules enforce style conventions and best practices:

| Rule | Name | Description |
|------|------|-------------|
| I001 | Metadata Order | Metadata should follow canonical order |
| I002 | High Priority Without Do-Date | Priority 1-2 actions may need `@` |
| I003 | Blocked Without Description | `[=]` actions should explain why |
| I004 | Action Name Too Long | Long names should use `$` instead |
| I005 | Missing Context | Actions may benefit from `+` tags |
| I006 | Missing Story Assignment | Root actions may benefit from `*` |
| I007 | Child Higher Priority Than Parent | May indicate organizational issues |
| I008 | TODO/FIXME Markers | May indicate incomplete planning |
| I009 | Question Marks in Name | May indicate uncertainty |
| I010 | All Caps Name | Use priority instead of shouting |

## Important Notes

### All Examples Use Valid Syntax

The `error.actions` files contain **valid** `.actions` syntax that will parse successfully. They demonstrate **semantic** or **stylistic** issues, not syntax errors.

For example:
- `#123` is valid syntax (hex characters) but an invalid UUID format (E004)
- `!6` is valid syntax (number) but an invalid priority level (E003)

This is intentional - linting happens *after* parsing, so linting test cases must use parseable input.

### Multiple Violations Per File

Each `error.actions` file contains 2-4 instances of the same violation to provide more comprehensive test coverage. All violations in a file are related to the same rule - we don't mix different rule violations.

### Configuration-Dependent Rules

Some rules have configurable thresholds (noted in comments):
- W001: `past_do_date_threshold_days` (default: 7)
- W002: `early_completion_threshold_minutes` (default: 60)
- W003: `max_duration_minutes` (default: 480)
- W004: `old_action_threshold_days` (default: 365)
- I004: `max_name_length` (default: 50)

The examples use default thresholds unless otherwise noted.

## Usage

### For Linter Implementors

Use these test cases to verify your linter implementation:

```bash
# Example: Testing a linter
for test in E*/; do
  linter check "$test/error.actions" > output.txt
  # Verify expected diagnostics are present
  # Verify no diagnostics for fixed.actions
  linter check "$test/fixed.actions" || echo "Fixed file should not have errors"
done
```

### For Snapshot Testing

These examples are ideal for snapshot testing:

```javascript
// Example: Jest snapshot test
describe('Linter', () => {
  const testDirs = fs.readdirSync('specifications/examples/linting');

  testDirs.forEach(dir => {
    test(`${dir} detects errors`, () => {
      const result = linter.check(`${dir}/error.actions`);
      expect(result.diagnostics).toMatchSnapshot();
    });

    test(`${dir} fixed version passes`, () => {
      const result = linter.check(`${dir}/fixed.actions`);
      expect(result.diagnostics).toHaveLength(0);
    });
  });
});
```

### For the CLI

The actions-cli can use these for testing and documentation:

```bash
cd actions-cli
npm run test:linting
```

## Adding New Test Cases

When adding linting test cases:

1. **Create a descriptive directory name** using the rule code and name (e.g., `E017_new_rule`)
2. **Add both files**:
   - `error.actions` - Valid syntax with 2-4 violations of the rule
   - `fixed.actions` - Same actions corrected
3. **Test one rule** - Each directory tests a single linting rule
4. **Use realistic examples** - Prefer real-world scenarios over contrived cases
5. **Add comments** - Include rule description at top of each file
6. **Verify parsing** - Ensure `error.actions` parses successfully

## Linting Rules Reference

For the complete linting specification, see:
- [linting_specification.md](../../linting_specification.md) - Canonical linting rules
- [action_specification.md](../../action_specification.md) - File format syntax
- [formatting_specification.md](../../formatting_specification.md) - Formatting rules

## Version History

- **2026-01-04** - Initial test suite with E, W, and I level rules (29 rules total)
