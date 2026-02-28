//! RDF graph module for storing and querying actions using Oxigraph
//!
//! This module implements the RDF schema from the Actions Vocabulary v4 ontology.
//!
//! # Architecture
//!
//! Actions are loaded into an in-memory Oxigraph store as RDF triples.
//! The domain model (Plan, PlannedAct) maps to CCO-aligned classes.
//!
//! # Usage
//!
//! ```ignore
//! use clearhead_cli::graph;
//!
//! // Create store and load actions
//! let store = graph::create_store()?;
//! graph::load_actions(&store, &actions)?;
//!
//! // Query for action IDs
//! let ids = graph::query_actions(&store, "SELECT ?id WHERE { ... }")?;
//! ```

// Import core types
use clearhead_core::{ActPhase, DomainModel, Plan, PlannedAct};

// CLI-specific imports
use crate::environment_reader::Config;
use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

// Namespace constants
const ACTIONS_NS: &str = "https://clearhead.us/vocab/actions/v4#";
const CCO_NS: &str = "https://www.commoncoreontologies.org/";
const SCHEMA_NS: &str = "http://schema.org/";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";
const SKOS_NS: &str = "http://www.w3.org/2004/02/skos/core#";

// CCO class and property identifiers (opaque OBO IDs from the CCO ontology)
const CCO_PLAN: &str = "ont00000974";
const CCO_PLANNED_ACT: &str = "ont00000228";
const CCO_PRESCRIBES: &str = "ont00001942";
const CCO_STATUS_PROP: &str = "ont00001868"; // is_measured_by_nominal

/// Create an in-memory Oxigraph store
pub fn create_store() -> Result<Store, String> {
    Store::new().map_err(|e| e.to_string())
}

// Legacy alias for compatibility during refactor
pub fn create_database() -> Result<Store, String> {
    create_store()
}

fn ns(base: &str, name: &str) -> NamedNode {
    NamedNode::new(format!("{}{}", base, name)).unwrap()
}

fn actions_pred(name: &str) -> NamedNode {
    ns(ACTIONS_NS, name)
}

fn cco_node(id: &str) -> NamedNode {
    ns(CCO_NS, id)
}

fn schema_pred(name: &str) -> NamedNode {
    ns(SCHEMA_NS, name)
}

fn rdf_type() -> NamedNode {
    ns(RDF_NS, "type")
}

/// Execute a SPARQL query and return matching action IDs
///
/// The query should SELECT the `?id` variable.
pub fn query_actions(store: &Store, sparql: &str) -> Result<Vec<String>, String> {
    let results = SparqlEvaluator::new()
        .parse_query(sparql)
        .map_err(|e| e.to_string())?
        .on_store(store)
        .execute()
        .map_err(|e| e.to_string())?;

    let mut ids = Vec::new();

    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            let s = solution.map_err(|e| e.to_string())?;
            if let Some(term) = s.get("id")
                && let Term::Literal(lit) = term
            {
                ids.push(lit.value().to_string());
            }
        }
    }

    Ok(ids)
}

/// Build a SPARQL query from a WHERE clause
///
/// Wraps the where_clause in `SELECT ?id WHERE { ... }` and injects standard prefixes.
pub fn build_where_query(where_clause: &str, _select: Option<&str>, _from: Option<&str>) -> String {
    format!(
        "PREFIX actions: <{actions_ns}>\n\
         PREFIX cco: <{cco_ns}>\n\
         PREFIX schema: <{schema_ns}>\n\
         PREFIX rdf: <{rdf_ns}>\n\
         PREFIX xsd: <{xsd_ns}>\n\
         PREFIX skos: <{skos_ns}>\n\
         SELECT ?id WHERE {{    ?s <{actions_ns}id> ?id .    {{ {where_clause} }}
}}",
        actions_ns = ACTIONS_NS,
        cco_ns = CCO_NS,
        schema_ns = SCHEMA_NS,
        rdf_ns = RDF_NS,
        xsd_ns = XSD_NS,
        skos_ns = SKOS_NS,
        where_clause = where_clause
    )
}

/// Load tag hierarchies from Config into the store
pub fn load_tag_hierarchies(store: &Store, config: &Config) -> Result<(), String> {
    for (parent, children) in &config.tag_hierarchies {
        for child in children {
            // child skos:broader parent
            // We use simple literals or URIs for tags?
            // Spec doesn't strictly define tag URIs, so we'll use a local scheme or literals.
            // But skos:broader relates Concepts (Resources).
            // Let's use `urn:tag:name`.

            let parent_uri = NamedNode::new(format!("urn:tag:{}", parent.to_lowercase())).unwrap();
            let child_uri = NamedNode::new(format!("urn:tag:{}", child.to_lowercase())).unwrap();

            store
                .insert(&Quad::new(
                    NamedOrBlankNode::NamedNode(child_uri),
                    NamedNode::new(format!("{}broader", SKOS_NS)).unwrap(),
                    Term::NamedNode(parent_uri),
                    GraphName::DefaultGraph,
                ))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Query plans by context with hierarchy expansion
pub fn query_actions_by_context(store: &Store, context: &str) -> Result<Vec<String>, String> {
    let context_tag = context.to_lowercase();

    let sparql = format!(
        "SELECT ?id WHERE {{    ?s <{actions_ns}id> ?id .    ?s <{actions_ns}hasContext> ?tagLiteral .    BIND(IRI(concat(\"urn:tag:\", lcase(?tagLiteral))) AS ?tagURI)    ?tagURI <{skos_ns}broader>* <urn:tag:{target}> .
}}",
        actions_ns = ACTIONS_NS,
        skos_ns = SKOS_NS,
        target = context_tag
    );

    query_actions(store, &sparql)
}

/// Query plans by objective (project)
pub fn query_actions_by_project(store: &Store, project: &str) -> Result<Vec<String>, String> {
    let sparql = format!(
        "SELECT ?id WHERE {{    ?s <{actions_ns}id> ?id .    ?s <{actions_ns}hasObjective> \"{project}\" .
        }}",
        actions_ns = ACTIONS_NS,
        project = project
    );

    query_actions(store, &sparql)
}

// ============================================================================
// Domain Model (CCO-aligned)
// ============================================================================

fn phase_node(phase: &ActPhase) -> NamedNode {
    let name = match phase {
        ActPhase::NotStarted => "NotStarted",
        ActPhase::InProgress => "InProgress",
        ActPhase::Completed => "Completed",
        ActPhase::Blocked => "Blocked",
        ActPhase::Cancelled => "Cancelled",
    };
    actions_pred(name)
}

/// Load a DomainModel into the store using v4 ontology
///
/// This inserts Plans and PlannedActs as separate entities with proper
/// CCO-aligned types and relationships.
pub fn load_domain_model(store: &Store, model: &DomainModel) -> Result<(), String> {
    for plan in model.all_plans() {
        insert_plan(store, plan)?;
    }
    for act in model.all_acts() {
        insert_planned_act(store, act)?;
    }
    Ok(())
}

/// Insert a Plan into the store
///
/// Maps to cco:Plan - information content that defines a task.
fn insert_plan(store: &Store, plan: &Plan) -> Result<(), String> {
    let subject =
        NamedOrBlankNode::NamedNode(NamedNode::new(format!("urn:uuid:{}", plan.id)).unwrap());
    let graph = GraphName::DefaultGraph;

    let add = |pred: NamedNode, term: Term| {
        store
            .insert(&Quad::new(subject.clone(), pred, term, graph.clone()))
            .map_err(|e| e.to_string())
    };

    // rdf:type cco:Plan (ont00000974)
    add(rdf_type(), Term::NamedNode(cco_node(CCO_PLAN)))?;

    // actions:id
    add(
        actions_pred("id"),
        Term::Literal(Literal::new_simple_literal(plan.id.to_string())),
    )?;

    // schema:name
    add(
        schema_pred("name"),
        Term::Literal(Literal::new_simple_literal(&plan.name)),
    )?;

    // schema:description
    if let Some(desc) = &plan.description {
        add(
            schema_pred("description"),
            Term::Literal(Literal::new_simple_literal(desc)),
        )?;
    }

    // actions:hasPriority
    if let Some(priority) = plan.priority {
        add(
            actions_pred("hasPriority"),
            Term::Literal(Literal::new_typed_literal(
                priority.to_string(),
                ns(XSD_NS, "integer"),
            )),
        )?;
    }

    // actions:hasContext (multiple)
    if let Some(contexts) = &plan.contexts {
        for context in contexts {
            add(
                actions_pred("hasContext"),
                Term::Literal(Literal::new_simple_literal(context)),
            )?;
        }
    }

    // actions:hasObjective (story/project)
    if let Some(objective) = &plan.objective {
        add(
            actions_pred("hasObjective"),
            Term::Literal(Literal::new_simple_literal(objective)),
        )?;
    }

    // actions:partOf (parent plan)
    if let Some(parent_id) = plan.parent {
        let parent_uri = NamedNode::new(format!("urn:uuid:{}", parent_id)).unwrap();
        add(actions_pred("partOf"), Term::NamedNode(parent_uri))?;
    }

    // actions:dependsOn (predecessor plans)
    if let Some(deps) = &plan.depends_on {
        for dep_id in deps {
            let dep_uri = NamedNode::new(format!("urn:uuid:{}", dep_id)).unwrap();
            add(actions_pred("dependsOn"), Term::NamedNode(dep_uri))?;
        }
    }

    // actions:alias
    if let Some(alias) = &plan.alias {
        add(
            actions_pred("alias"),
            Term::Literal(Literal::new_simple_literal(alias)),
        )?;
    }

    // actions:isSequential
    if let Some(true) = plan.is_sequential {
        add(
            actions_pred("isSequential"),
            Term::Literal(Literal::new_typed_literal("true", ns(XSD_NS, "boolean"))),
        )?;
    }

    // Recurrence (as blank node)
    if let Some(recurrence) = &plan.recurrence {
        let bnode = BlankNode::default();
        add(actions_pred("hasRecurrence"), Term::BlankNode(bnode.clone()))?;

        let r_subj = NamedOrBlankNode::BlankNode(bnode);
        let add_r = |pred: NamedNode, term: Term| {
            store
                .insert(&Quad::new(r_subj.clone(), pred, term, graph.clone()))
                .map_err(|e| e.to_string())
        };

        add_r(
            actions_pred("frequency"),
            Term::Literal(Literal::new_simple_literal(&recurrence.frequency)),
        )?;

        if let Some(interval) = recurrence.interval {
            add_r(
                actions_pred("interval"),
                Term::Literal(Literal::new_typed_literal(
                    interval.to_string(),
                    ns(XSD_NS, "integer"),
                )),
            )?;
        }

        if let Some(by_day) = &recurrence.by_day {
            for day in by_day {
                add_r(
                    actions_pred("byDay"),
                    Term::Literal(Literal::new_simple_literal(day)),
                )?;
            }
        }
    }

    // cco:prescribes (ont00001942) — forward link from Plan to each PlannedAct
    for act in &plan.acts {
        let act_uri = NamedNode::new(format!("urn:uuid:{}", act.id)).unwrap();
        add(cco_node(CCO_PRESCRIBES), Term::NamedNode(act_uri))?;
    }

    Ok(())
}

/// Insert a PlannedAct into the store
///
/// Maps to cco:PlannedAct - an occurrence that realizes a Plan.
fn insert_planned_act(store: &Store, act: &PlannedAct) -> Result<(), String> {
    let subject =
        NamedOrBlankNode::NamedNode(NamedNode::new(format!("urn:uuid:{}", act.id)).unwrap());
    let graph = GraphName::DefaultGraph;

    let add = |pred: NamedNode, term: Term| {
        store
            .insert(&Quad::new(subject.clone(), pred, term, graph.clone()))
            .map_err(|e| e.to_string())
    };

    // rdf:type cco:PlannedAct (ont00000228)
    add(rdf_type(), Term::NamedNode(cco_node(CCO_PLANNED_ACT)))?;

    // actions:id
    add(
        actions_pred("id"),
        Term::Literal(Literal::new_simple_literal(act.id.to_string())),
    )?;

    // actions:prescribedBy (convenience inverse of cco:prescribes for efficient lookup)
    let plan_uri = NamedNode::new(format!("urn:uuid:{}", act.plan_id)).unwrap();
    add(actions_pred("prescribedBy"), Term::NamedNode(plan_uri))?;

    // cco:is_measured_by_nominal (ont00001868) — status as Event Status Nominal ICE
    add(cco_node(CCO_STATUS_PROP), Term::NamedNode(phase_node(&act.phase)))?;

    // actions:scheduledAt
    if let Some(dt) = &act.scheduled_at {
        add(
            actions_pred("scheduledAt"),
            Term::Literal(Literal::new_typed_literal(
                dt.to_rfc3339(),
                ns(XSD_NS, "dateTime"),
            )),
        )?;
    }

    // actions:duration
    if let Some(duration) = act.duration {
        add(
            actions_pred("duration"),
            Term::Literal(Literal::new_typed_literal(
                duration.to_string(),
                ns(XSD_NS, "integer"),
            )),
        )?;
    }

    // actions:completedAt
    if let Some(dt) = &act.completed_at {
        add(
            actions_pred("completedAt"),
            Term::Literal(Literal::new_typed_literal(
                dt.to_rfc3339(),
                ns(XSD_NS, "dateTime"),
            )),
        )?;
    }

    // actions:createdAt
    if let Some(dt) = &act.created_at {
        add(
            actions_pred("createdAt"),
            Term::Literal(Literal::new_typed_literal(
                dt.to_rfc3339(),
                ns(XSD_NS, "dateTime"),
            )),
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_core::Action;
    use clearhead_core::workspace::actions::convert;

    #[test]
    fn test_load_domain_model() {
        let store = create_store().unwrap();

        let actions = vec![Action::new("Test task")];
        let model = convert::from_actions(&actions);

        load_domain_model(&store, &model).unwrap();

        // Verify Plan was inserted with correct CCO class URI (ont00000974)
        let plan_query = format!(
            "SELECT ?name WHERE {{ ?s a <{}{}> . ?s <{}name> ?name }}",
            CCO_NS, CCO_PLAN, SCHEMA_NS
        );
        let results = SparqlEvaluator::new()
            .parse_query(&plan_query)
            .unwrap()
            .on_store(&store)
            .execute()
            .unwrap();

        if let QueryResults::Solutions(solutions) = results {
            let names: Vec<_> = solutions
                .filter_map(|s| s.ok())
                .filter_map(|s| s.get("name").cloned())
                .collect();
            assert_eq!(names.len(), 1);
        }
    }

    #[test]
    fn test_plan_and_act_linked() {
        let store = create_store().unwrap();

        let actions = vec![Action::new("Linked task")];
        let model = convert::from_actions(&actions);
        let plan_id = model.all_plans()[0].id;

        load_domain_model(&store, &model).unwrap();

        // Query for PlannedAct using the convenience prescribedBy inverse
        let query = format!(
            "SELECT ?act WHERE {{ ?act <{}prescribedBy> <urn:uuid:{}> }}",
            ACTIONS_NS, plan_id
        );
        let results = SparqlEvaluator::new()
            .parse_query(&query)
            .unwrap()
            .on_store(&store)
            .execute()
            .unwrap();

        if let QueryResults::Solutions(solutions) = results {
            let acts: Vec<_> = solutions.filter_map(|s| s.ok()).collect();
            assert_eq!(acts.len(), 1);
        }
    }
}
