# Action Structs Design

**Purpose:** Define Rust data structures that serve as the hub between DSL, CRDT, and RDF representations
**Philosophy:** Structs are **semantic representation**, not just storage - they encode meaning per ontology

---

## Core Design Principles

### 1. Lossless Conversion
Every transformation must be perfectly reversible:
- **CRDT → Structs → CRDT** = identical state
- **Structs → RDF → Structs** = no information loss
- **DSL → Structs → DSL** = preserves formatting where it matters

### 2. Type-Safety Through Semantics
Use Rust's type system to enforce invariants that ontology defines:
- ActionPlans can't have process states
- ActionProcesses must have a prescribing ActionPlan
- Hierarchy depth is constrained to 0-5

### 3. Pragmatic Defaults, Rigorous Core
Balance usability with ontological correctness:
- **Contexts**: Start as `Vec<String>` for editor workflow
- **Future**: Path to typed entities when LSP needs it
- **Separation**: Keep plan vs process data distinct

---

## Struct Definitions

### ActionPlan (BFO Continuant)

```rust
/// An ActionPlan is information that persists across time and prescribes executions
/// BFO: Generically Dependent Continuant → Directive Information Content Entity
/// Ontology: ActionPlan (subClass of cco:ont00000965)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPlan {
    /// UUIDv7 identifier - unique across workspace
    /// Ontology: Not a property, used for referencing
    pub id: UuidV7,

    /// Action name/title - required, primary key for humans
    /// Ontology: schema:name
    pub name: String,

    /// Optional long-form description
    /// Ontology: schema:description
    pub description: Option<String>,

    /// Hierarchical depth (0-5), maps to Root/Child/Leaf classes
    /// Ontology: actions:hasDepth
    pub depth: u8,  // 0 = RootActionPlan, 1-4 = ChildActionPlan, 5 = LeafActionPlan

    /// Parent action ID (None for root actions)
    /// Ontology: actions:parentAction (subProperty of bfo:0000050 part_of)
    pub parent_id: Option<UuidV7>,

    /// Project/story assignment (root actions only)
    /// Ontology: actions:hasProject
    pub project: Option<String>,

    /// Priority level 1-4 (Eisenhower Matrix)
    /// Ontology: actions:hasPriority
    pub priority: Option<Priority>,  // 1=Do(Urgent&Important), 2=Schedule, 3=Delegate, 4=Delete

    /// Contexts required for execution
    /// CURRENT: String-based for pragmatic compatibility
    /// FUTURE: May become Vec<ContextRef> for typed entities
    /// Ontology: actions:requiresContext (→ ActionContext classes)
    pub contexts: Vec<String>,

    /// Planned start date/time
    /// Ontology: actions:hasDoDateTime
    pub do_datetime: Option<DateTime<Utc>>,

    /// Planned duration in minutes
    /// Ontology: actions:hasDurationMinutes
    pub duration_minutes: Option<u32>,

    /// Recurrence rule (RFC 5545 RRULE)
    /// Ontology: actions:hasRecurrenceFrequency, actions:hasRecurrenceInterval, etc.
    pub recurrence: Option<RecurrenceRule>,

    /// Logical predecessors (must complete before this can start)
    /// Ontology: actions:dependsOn (transitive property)
    pub predecessors: Vec<UuidV7>,

    /// Sequential children marker (children implicitly depend on previous sibling)
    /// DSL-only: Not stored in ontology, but tracked for DSL projection
    pub sequential_children: bool,

    /// Stable human-readable reference
    /// Ontology: Not directly in ontology, but maps to assignedToAgent patterns
    pub alias: Option<String>,

    /// Creation timestamp (derived from UUIDv7 if not set)
    /// Ontology: Not a standard property, but useful for analytics
    pub created_at: DateTime<Utc>,
}

/// Hierarchy classes - enforced by depth field
impl ActionPlan {
    pub fn is_root(&self) -> bool { self.depth == 0 }
    pub fn is_child(&self) -> bool { (1..=4).contains(&self.depth) }
    pub fn is_leaf(&self) -> bool { self.depth == 5 }

    pub fn can_have_project(&self) -> bool {
        self.is_root()  // Only root actions have projects
    }

    pub fn can_have_children(&self) -> bool {
        !self.is_leaf()  // Only root and child actions can have children
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Priority(pub u8);
impl Priority {
    pub const DO: Self = Self(1);      // Urgent & Important
    pub const SCHEDULE: Self = Self(2); // Important (not urgent)
    pub const DELEGATE: Self = Self(3); // Urgent (not important)
    pub const DELETE: Self = Self(4);    // Neither urgent nor important

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1..=4 => Some(Self(value)),
            _ => None,
        }
    }
}
}
```

### ActionProcess (BFO Occurrent)

```rust
/// An ActionProcess is the actual execution of an ActionPlan
/// BFO: Occurrent (Process) → Planned Act
/// Ontology: ActionProcess (subClass of cco:ont00000228)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionProcess {
    /// UUIDv7 identifier - unique across workspace
    pub id: UuidV7,

    /// The ActionPlan that prescribes this execution
    /// Ontology: cco:prescribed_by (inverse of actions:prescribes)
    pub plan_id: UuidV7,

    /// Action name (inherited or overridden from plan)
    pub name: String,

    /// Current execution state
    /// Ontology: actions:hasState (BFO quality that inheres in process)
    pub state: ProcessState,

    /// When execution was actually completed
    /// Ontology: actions:hasCompletedDateTime
    pub completed_at: Option<DateTime<Utc>>,

    /// Notes about execution (not intention)
    /// Ontology: Not in ontology, but useful for human notes
    pub notes: Option<String>,
}

/// Execution states - BFO qualities of processes
/// Ontology: ActionState (subClass of bfo:Quality)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessState {
    NotStarted,
    InProgress,
    Completed,
    Blocked,
    Cancelled,
}
```

### RecurrenceRule (iCalendar RFC 5545)

```rust
/// Recurrence defines pattern for generating process instances
/// Ontology: Maps to actions:hasRecurrenceFrequency, hasRecurrenceInterval, etc.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceRule {
    /// Frequency: DAILY, WEEKLY, MONTHLY, YEARLY
    pub frequency: Frequency,

    /// Repeat every N intervals (default 1)
    pub interval: u32,

    /// Maximum number of occurrences (mutually exclusive with until)
    pub count: Option<u32>,

    /// End date/time for recurrence (mutually exclusive with count)
    pub until: Option<DateTime<Utc>>,

    /// Specific days of week: MO, TU, WE, TH, FR, SA, SU
    pub by_day: Option<Vec<Weekday>>,

    /// Days of month: 1-31 or -1 to -31
    pub by_month_day: Option<Vec<i32>>,

    /// Hours: 0-23
    pub by_hour: Option<Vec<u8>>,

    /// Minutes: 0-59
    pub by_minute: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
```

---

## The Union: Action

We work with a unified representation that can be either a plan or process:

```rust
/// Represents an Action entity that can be either a plan or a process
/// CRDT stores both; struct unifies for in-memory handling

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    Plan(ActionPlan),
    Process(ActionProcess),
}

impl Action {
    /// Get UUID regardless of variant
    pub fn id(&self) -> UuidV7 {
        match self {
            Action::Plan(p) => p.id,
            Action::Process(p) => p.id,
        }
    }

    /// Check if this is a plan (has intention properties)
    pub fn is_plan(&self) -> bool {
        matches!(self, Action::Plan(_))
    }

    /// Check if this is a process (has execution properties)
    pub fn is_process(&self) -> bool {
        matches!(self, Action::Process(_))
    }
}
```

---

## Conversion Contracts

### CRDT ↔ Structs

**CRDT Schema** (Automerge):
```typescript
{
  "plans": Map<UuidV7, ActionPlan>,
  "processes": Map<UuidV7, ActionProcess>
}
```

**Conversion Rules:**

1. **CRDT → Structs** (Materialization):
```rust
fn materialize_from_crdt(crdt: &AutomergeDoc) -> Result<Workspace> {
    let plans: Map<UuidV7, ActionPlan> = crdt.get("plans")?;
    let processes: Map<UuidV7, ActionProcess> = crdt.get("processes")?;

    // Validate referential integrity
    for process in processes.values() {
        if !plans.contains_key(&process.plan_id) {
            return Err(Error::ProcessWithoutPlan(process.id));
        }
    }

    Ok(Workspace { plans, processes })
}
```

2. **Structs → CRDT** (Update):
```rust
fn apply_to_crdt(workspace: &Workspace, crdt: &mut AutomergeDoc) -> Result<()> {
    // Update plans
    crdt.set("plans", &workspace.plans)?;

    // Update processes
    crdt.set("processes", &workspace.processes)?;

    // Automerge handles merge conflicts automatically
    Ok(())
}
```

**Key invariants:**
- No orphaned processes (must reference valid plan)
- Depth consistency enforced by parent_id + depth
- UUIDs are always UuidV7

---

### DSL ↔ Structs (via tree-sitter)

**Input:** tree-sitter AST

**Conversion Rules:**

1. **Parse → Structs**:
```rust
fn from_ast(node: Node) -> Result<Action> {
    // Extract state marker [x] [-] [=] [_]
    let state = extract_state(node)?;

    // Extract metadata (!, *, +, @, D, R:, etc.)
    let plan = ActionPlan {
        id: extract_or_generate_uuid(node)?,
        name: extract_name(node)?,
        state,  // Only for processes
        depth: calculate_depth(node)?,
        parent_id: extract_parent_uuid(node)?,
        // ... extract other fields
    };

    Ok(Action::Plan(plan))
}
```

2. **Structs → DSL**:
```rust
fn to_dsl(actions: &[Action]) -> String {
    actions.iter()
        .map(|action| action.to_dsl_string())
        .collect::<Vec<_>>()
        .join("\n")  // One action per line
}

impl Action {
    fn to_dsl_string(&self) -> String {
        match self {
            Action::Plan(plan) => plan.to_dsl_string(),
            Action::Process(proc) => proc.to_dsl_string(),
        }
    }
}
```

**Formatting:**
- Preserve user's horizontal spacing (formatting spec: whitespace-insensitive)
- Add depth markers (`>`, `>>`, etc.) based on depth
- Order metadata canonically (linting I006 rule)

---

### Structs ↔ RDF (for Oxigraph)

**Ontology Mapping:**

| Struct Field | Ontology Property | RDF Value Type | Notes |
|-------------|------------------|----------------|------|
| id | - | - | Not stored as property, used for subject |
| name | schema:name | xsd:string | Shared between Plan and Process |
| depth | actions:hasDepth | xsd:integer | Only for plans |
| parent_id | actions:parentAction | xsd:string | Only for plans, references plan UUID |
| project | actions:hasProject | xsd:string | Only for root plans |
| priority | actions:hasPriority | xsd:integer | 1-4 |
| contexts | actions:requiresContext | xsd:string | CURRENT: strings, FUTURE: typed entities |
| do_datetime | actions:hasDoDateTime | xsd:dateTime | ISO 8601 |
| duration_minutes | actions:hasDurationMinutes | xsd:positiveInteger | |
| recurrence | actions:hasRecurrenceFrequency | - | Expanded to multiple properties |
| predecessors | actions:dependsOn | xsd:string | Plan UUID references |
| sequential_children | - | - | DSL-only, not in ontology |
| alias | - | - | Maps to assignedToAgent pattern |
| plan_id (Process) | cco:prescribed_by | xsd:string | Only for processes |
| state (Process) | actions:hasState | actions:ActionState | BFO quality |
| completed_at (Process) | actions:hasCompletedDateTime | xsd:dateTime | Only for processes |
| notes (Process) | - | - | Not in ontology |

**Materialization Strategy:**

```rust
fn to_rdf(actions: &[Action]) -> Vec<Triple> {
    let mut triples = Vec::new();

    for action in actions {
        match action {
            Action::Plan(plan) => {
                // Create Plan subject
                let subject = format!("urn:action:plan:{}", plan.id);

                // Add type triple
                triples.push Triple {
                    subject: subject.clone(),
                    predicate: "rdf:type".into(),
                    object: object_from_depth(plan.depth),  // Root/Child/Leaf
                });

                // Add name
                triples.push(Triple::literal(
                    subject.clone(),
                    "schema:name",
                    plan.name.clone(),
                ));

                // Add properties...

                // Add contexts (PHASE 1: as strings)
                for ctx in &plan.contexts {
                    triples.push(Triple::literal(
                        subject.clone(),
                        "actions:requiresContext",
                        ctx.clone(),  // String literal
                    ));
                }
            }
            }
            Action::Process(proc) => {
                // Create Process subject
                let subject = format!("urn:action:process:{}", proc.id);

                // Add type triple
                triples.push(Triple {
                    subject: subject.clone(),
                    predicate: "rdf:type".into(),
                    object: "actions:ActionProcess".into(),
                });

                // Add prescribed_by link
                triples.push(Triple::iri(
                    subject.clone(),
                    "cco:prescribed_by",
                    format!("urn:action:plan:{}", proc.plan_id),
                ));

                // Add state
                triples.push(Triple::iri(
                    subject.clone(),
                    "actions:hasState",
                    format!("actions:{}", proc.state.to_rdf_string()),
                ));

                // Add properties...
            }
        }
    }

    triples
}

fn object_from_depth(depth: u8) -> Object {
    match depth {
        0 => "actions:RootActionPlan".into(),
        1..=4 => "actions:ChildActionPlan".into(),
        5 => "actions:LeafActionPlan".into(),
        _ => unreachable!(),
    }
}

impl ProcessState {
    fn to_rdf_string(&self) -> &'static str {
        match self {
            ProcessState::NotStarted => "actions:NotStarted",
            ProcessState::InProgress => "actions:InProgress",
            ProcessState::Completed => "actions:Completed",
            ProcessState::Blocked => "actions:Blocked",
            ProcessState::Cancelled => "actions:Cancelled",
        }
    }
}
```

**Phase 2 Enhancement (LSP integration):**

When LSP adds SHACL validation, convert string contexts to typed entities:

```rust
fn contexts_to_typed(
    contexts: &[String],
    tag_hierarchies: &TagHierarchies,
) -> Vec<ContextRef> {
    // Expand hierarchical tags
    // "+neovim" → ["+neovim", "+terminal", "+computer"]
    // Resolve to ontology classes
    // "+office" → actions:LocationContext → specific facility
}
```

---

## Query Requirements (Agenda View MVP)

### The Killer Query: "What can I do right now?"

This query must answer in one efficient SPARQL call:

```sparql
PREFIX actions: <https://clearhead.us/vocab/actions/v3#>
PREFIX cco: <https://www.commoncoreontologies.org/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?plan ?name ?priority
WHERE {
  # Must be a root action plan (depth = 0)
  ?plan a actions:RootActionPlan ;
        actions:hasDepth 0 ;
        schema:name ?name ;
        actions:hasPriority ?priority .

  # Optional: In a project (filter by project)
  # ?plan actions:hasProject ?project .

  # Dependencies: All predecessors must be completed
  FILTER NOT EXISTS {
    ?plan actions:dependsOn ?depPlan .
    ?depPlan cco:prescribes ?depProcess .
    FILTER (?depProcess actions:hasState != actions:Completed &&
            ?depProcess actions:hasState != actions:Cancelled)
  }

  # Parents: All ancestor plans must be completed
  FILTER NOT EXISTS {
    ?plan actions:parentAction+ ?ancestorPlan .
    ?ancestorPlan cco:prescribes ?ancestorProcess .
    FILTER (?ancestorProcess actions:hasState != actions:Completed &&
            ?ancestorProcess actions:hasState != actions:Cancelled)
  }

  # Contexts: Must have at least one matching context
  # CURRENT: String matching with tag_hierarchies expansion
  # FUTURE: Typed entity matching (actions:requiresContext ?ctx)
  # ?plan actions:requiresContext ?ctx .
  # FILTER (?ctx = "@computer" || ?ctx = "@low_energy")

  # Time: Do date not overdue (or no do date)
  FILTER (NOT EXISTS { ?plan actions:hasDoDateTime ?doDate } ||
          ?doDate >= NOW)
}
ORDER BY ?priority
LIMIT 20
```

### Required RDF Structure

For this query to work, Oxigraph must contain:

```turtle
# Example: Simple plan with contexts
@prefix actions: <https://clearhead.us/vocab/actions/v3#>
@prefix schema: <http://schema.org/>
@prefix xsd: <http://www.w3.org/2001/XMLSchema#>

:plan-001 a actions:RootActionPlan ;
    schema:name "Review quarterly reports" ;
    actions:hasDepth 0 ;
    actions:hasPriority 2 ;
    actions:requiresContext "@computer", "@office" .  # CURRENT: strings
    # actions:requiresContext :ctx-computer, :ctx-office .  # FUTURE: typed entities
```

---

## Implementation Phases

### Phase 1: Core Structs (Do Now)
- [ ] Define ActionPlan, ActionProcess, RecurrenceRule
- [ ] Define Action union enum
- [ ] Add derive macros for serialization
- [ ] Add validation invariants (depth constraints, etc.)

### Phase 2: CRDT Conversion
- [ ] Implement CRDT → Structs materialization
- [ ] Implement Structs → CRDT updates
- [ ] Add referential integrity checks
- [ ] Write roundtrip tests

### Phase 3: DSL Integration
- [ ] Implement DSL → Structs via tree-sitter AST
- [ ] Implement Structs → DSL projection
- [ ] Add depth marker generation
- [ ] Test preservation of user formatting

### Phase 4: RDF Materialization (MVP)
- [ ] Implement Structs → RDF conversion
- [ ] Map fields to ontology properties correctly
- [ ] Use string contexts initially
- [ ] Test basic SPARQL queries

### Phase 5: Advanced Features (Later)
- [ ] Add typed context support (resolve tag_hierarchies)
- [ ] Implement recurrence expansion
- [ ] Add more complex SPARQL queries
- [ ] Integrate SHACL validation

---

## Open Questions

1. **Context representation**: Should we start with strings or immediately add typed context model?
   - **Recommendation**: Strings for now, typed entities as Phase 5 (LSP integration)

2. **State handling**: When user marks `[x]` in DSL, do we:
   - Update plan to "retired" + create completed process (non-recurring)
   - Create completed process only (recurring)
   - **Recommendation**: Track plan state separately from process state

3. **Dependency representation**: Should predecessors store UUID references or full objects?
   - **Recommendation**: UUID references (simpler, matches CRDT model)

4. **Performance**: Should we cache RDF materialization or rebuild on every query?
   - **Recommendation**: Rebuild on CRDT change (materialize flag)

---

## Testing Requirements

1. **Roundtrip Tests**: CRDT → Structs → CRDT = identical
2. **Lossless Conversion**: Structs → RDF → Structs = no information loss
3. **Invariant Enforcement**: Depth 0-5 enforced, no orphans, etc.
4. **Query Correctness**: Agenda query returns expected results for known test data

---

## References

- [Ontology V3](https://clearhead.us/vocab/actions/v3) - Formal definitions
- [BFO/CCO Alignment](../ontology/BFO_CCO_ALIGNMENT.md) - Philosophical foundations
- [Action File Format](../specifications/action_file_format.md) - DSL specification
- [Configuration](../specifications/configuration.md) - tag_hierarchies, etc.
- [Linting Specification](../specifications/linting.md) - Validation rules
- [DECISIONS.md](../DECISIONS.md) - Architectural decisions

---

**Version:** 1.0.0
**Created:** 2026-01-19
**Status:** Design Document - Not Implemented
