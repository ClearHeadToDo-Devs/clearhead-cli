# Ontology & Query Integration Strategy

**Purpose:** Define how CLI integrates with ontology, SPARQL queries, and validation at STRATEGY level

---

## Repository Responsibilities

### Ontology Repository (Canonical Source)

**Location:** `/ontology/` (in platform repo)

**Responsibilities:**
- **Canonical semantic definitions** - Actions Vocabulary (BFO/CCO-aligned)
- **SHACL validation rules** - Constraint definitions
- **Published vocabulary** - Stable URIs for external reference
- **Versioning** - Semantic versioning, schema evolution

**Key Files:**
- `ontology/site/vocab/actions/v3/actions-vocabulary.ttl` - The vocabulary
- `ontology/site/vocab/actions/v3/shapes.ttl` - SHACL validation rules

**Why central here:**
- Multiple implementations reference same source
- Avoids duplicate definitions and version drift
- Single source of truth for semantic meaning
- Stable URIs (`https://clearhead.us/vocab/actions/v3#`)

### CLI Repository (Implementation)

**Location:** `/clearhead-cli/` (in platform repo)

**Responsibilities:**
- **Application logic** - How to use data for user workflows
- **Query definitions** - SPARQL queries for features
- **Struct definitions** - Rust types (aligned with ontology)
- **CRDT integration** - Automerge storage and sync
- **DSL parsing** - tree-sitter integration
- **Validation integration** - SHACL validation (via pyshacl or Rust)

**Key Files:**
- `src/models/` - Struct definitions (derived from spec)
- `src/rdf/queries/` - SPARQL query files (app-specific)
- `src/crdt/` - CRDT storage layer

**Why separate:**
- Queries evolve independently of ontology
- Implementation details differ per tool
- Keeps ontology repo pure and canonical
- Allows multiple query strategies without conflicting

---

## Integration Strategy

### 1. Ontology Access

**Approach:** URL Reference (Recommended)

```rust
const ACTIONS_VOCABULARY: &str = "https://clearhead.us/vocab/actions/v3#";
const ACTIONS_SHAPES: &str = "https://clearhead.us/vocab/actions/v3/shapes.ttl";
```

**Benefits:**
- Always uses latest published version
- No file sync issues
- Works for anyone with internet access
- Smaller binary size

**Alternative:** Vendored Copy

**Benefits:**
- Works offline
- Faster (no network I/O)
- Version consistency (ships with CLI)
- Larger binary size

**Decision:** URL reference for development, vendored for production releases

### 2. SHACL Validation

**Architecture:**
```
Structs → RDF Triples → pyshacl Validation → LSP Diagnostics
```

**Why pyshacl initially:**
- Battle-tested implementation
- Python integration (easy to call from Rust)
- Rich error messages

**Future: Native Rust SHACL**
- Faster performance
- No Python dependency
- But requires development effort

**Implementation:**
```rust
use pyo3::prelude::*;

pub fn validate_with_shacl(actions: &[Action]) -> Vec<Diagnostic> {
    let rdf_graph = actions_to_rdf_triples(actions)?;
    
    pyo3::run_bound(
        &["python", "-c", &format!("from pyshacl import validate; validate('{}', rdf_to_string(rdf_graph))],
        |stdout_handler|pyo3::Text::new().map(|output| {
            output
                .split('\n')
                .filter_map(|line| line_to_diagnostic(line))
                .filter(|diag| !diag.is_empty())
                .collect()
    )
    ).map_err(|e| format!("SHACL validation failed: {}", e))?;
    
    Ok(diagnostics)
}
```

### 3. SPARQL Query Integration

**Query File Location:** `src/rdf/queries/`

**Example structure:**
```sparql
# agenda.sparql
PREFIX actions: <https://clearhead.us/vocab/actions/v3#>
PREFIX cco: <https://www.commoncoreontologies.org/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?plan ?name ?priority
WHERE {
  ?plan a actions:RootActionPlan ;
        actions:hasDepth 0 ;
        schema:name ?name ;
        actions:hasPriority ?priority .
  
  FILTER NOT EXISTS {
    ?plan actions:dependsOn ?depPlan .
    ?depPlan cco:prescribes ?depProcess .
    FILTER (?depProcess actions:hasState != actions:Completed)
  }
}
ORDER BY ?priority
LIMIT 20
```

**Loading queries:**
```rust
use std::fs;

pub fn load_query(name: &str) -> Result<String> {
    let query_path = format!("src/rdf/queries/{}.sparql", name);
    fs::read_to_string(query_path)
        .map_err(|e| format!("Failed to load query: {}", e))
}
```

**Executing queries:**
```rust
use oxigraph::store::Store;

pub fn execute_agenda_query(store: &Store) -> Result<Vec<AgendaItem>> {
    let query = load_query("agenda")?;
    
    let results = store.query(&query)?;
    
    Ok(results.into_iter()
        .map(|binding| AgendaItem {
            id: binding.get("plan")?.unwrap(),
            name: binding.get("name")?.unwrap(),
            priority: binding.get("priority")?.unwrap(),
        })
        .collect())
}
```

### 4. Context Hierarchy Expansion

**Strategy:** Expand string contexts to typed entities when LSP needs it

```rust
pub fn expand_contexts(
    contexts: &[String],
    tag_hierarchies: &TagHierarchies,
    context_defs: &[ContextDefinition],
) -> Vec<ContextRef> {
    let mut expanded = Vec::new();
    
    for ctx in contexts {
        // Add context itself
        expanded.push(ContextRef {
            id: ctx.clone(),
            name: ctx.clone(),
            context_type: ContextType::Tag,  // Initially as tag
        });
        
        // Add parent contexts from hierarchy
        if let Some(parents) = tag_hierarchies.get(ctx) {
            for parent in parents {
                expanded.push(ContextRef {
                    id: format!("ctx-{}-{}", hash_string(&parent)),
                    name: parent.clone(),
                    context_type: ContextType::Location, // Example
                });
            }
        }
    }
    
    expanded
}
```

**When to use:**
- LSP validation needs typed entities (for SHACL validation)
- Complex analytics queries benefit from ontology reasoning
- Keep string contexts for editor workflow

---

## Directory Structure

### Ontology Repo (Canonical)
```
ontology/
├── site/vocab/actions/v3/
│   ├── actions-vocabulary.ttl     ← Published vocabulary (stable URI)
│   └── shapes.ttl                 ← SHACL validation rules
└── schemas/
    └── action.schema.json            ← JSON Schema (for JSON-LD)
```

### CLI Repo (Implementation)
```
clearhead-cli/
├── src/
│   ├── models/                      ← Struct definitions (from spec)
│   ├── crdt/                         ← CRDT storage
│   ├── rdf/                          ← RDF integration
│   │   ├── mod.rs
│   │   ├── materialize.rs       ← Structs → RDF
│   │   ├── queries.rs           ← SPARQL files
│   │   └── vendored/          ← Optional: ontology copy
│   └── linter/                      ← SHACL validation
│       ├── shacl_validator.rs   ← Via pyshacl
│       └── diagnostics.rs
└── Cargo.toml
```

---

## Integration Flows

### Flow 1: LSP Edit → Validation → Diagnostics

```
User edits DSL in Neovim
    ↓
tree-sitter parses to AST
    ↓
AST converted to Structs (following spec)
    ↓
Structs converted to RDF triples
    ↓
pyshacl validates against SHACL shapes
    ↓
Diagnostics sent to LSP client
```

### Flow 2: CLI Query → Oxigraph → Results

```
User runs: clearhead agenda
    ↓
CLI loads CRDT document
    ↓
CRDT materialized to Structs
    ↓
Structs converted to RDF triples
    ↓
RDF loaded into Oxigraph
    ↓
SPARQL query executed (agenda.sparql)
    ↓
Results formatted and displayed
```

### Flow 3: Sync Update → Materialization

```
clearhead-sync receives CRDT changes
    ↓
Updates local CRDT document
    ↓
Materializes to Structs (detects changes)
    ↓
Incrementally updates Oxigraph (rebuilds affected triples)
    ↓
Projects updated DSL files (if visible)
```

---

## Configuration

### RDF Settings

```json
{
  "rdf": {
    "enabled": true,
    "store_path": "~/.local/state/clearhead/oxigraph/",
    "query_timeout_seconds": 5,
    "ontology_url": "https://clearhead.us/vocab/actions/v3#",
    "shapes_url": "https://clearhead.us/vocab/actions/v3/shapes.ttl"
  }
}
```

### Validation Settings

```json
{
  "linter": {
    "shacl_enabled": true,
    "severity_overrides": {
      "I013": "info"  // Downgrade warnings
    }
  }
}
```

---

## Testing Strategy

### Semantic Tests

Use test data from ontology repository:
```bash
cd ontology/examples/v3
cargo test --test semantic_validity
```

### Conversion Tests

Test roundtrip transformations:
```rust
#[test]
fn test_structs_crdt_roundtrip() {
    let original = load_test_crdt();
    let structs = materialize_from_crdt(&original)?;
    let crdt_after = structs_to_crdt(&structs)?;
    
    assert_eq!(original, crdt_after);
}
```

### Query Tests

Test SPARQL queries against known data:
```rust
#[test]
fn test_agenda_query() {
    let store = setup_test_oxigraph(&test_actions());
    let results = execute_agenda_query(&store)?;
    
    assert_eq!(results.len(), 5);  // Should find 5 doable actions
}
```

---

## Implementation Phases

### Phase 0: Specification (Do Now)
- [ ] Review and finalize [struct_design.md](../specifications/struct_design.md)
- [ ] Review existing ontology mappings
- [ ] Document integration strategy

### Phase 1: Foundation (Foundation)
- [ ] Define core structs (ActionPlan, ActionProcess)
- [ ] Implement CRDT conversion layer
- [ ] Basic RDF materialization

### Phase 2: Query Layer (MVP)
- [ ] Integrate Oxigraph
- [ ] Implement agenda query
- [ ] Add query file loading

### Phase 3: Validation (Correctness)
- [ ] Integrate pyshacl for SHACL validation
- [ ] Convert SHACL violations to LSP diagnostics
- [ ] Add severity override support

### Phase 4: Advanced Features
- [ ] Typed context support (string → entity expansion)
- [ ] Recurrence expansion in queries
- [ ] Critical path analysis queries
- [ ] Completion analytics queries

---

## Open Questions

1. **Native Rust SHACL:**
   - When should we implement Rust SHACL instead of pyshacl?
   - Trade-off: Development effort vs. runtime performance

2. **Oxigraph versioning:**
   - How to handle schema migrations?
   - Approach: Rebuild entire graph on version change

3. **Offline support:**
   - Should we cache ontology files locally?
   - Strategy: Download on first run, vendored for production

4. **Query caching:**
   - Should we cache query results?
   - Consider: Only for expensive analytics, not agenda view

---

## See Also

- [Struct Design](../specifications/struct_design.md) - Data model concepts
- [Ontology](../ontology/) - Semantic definitions and validation
- [Linting Specification](../specifications/linting.md) - Validation rules
- [Action File Format](../specifications/action_file_format.md) - DSL syntax
- [DECISIONS.md](../DECISIONS.md) - Architectural decisions

---

**Version:** 1.0.0
**Created:** 2026-01-19
**Status:** Strategy Document
