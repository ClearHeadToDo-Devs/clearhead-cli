//! Tree renderer for `DomainModel` hierarchy.
//!
//! Produces a Unicode box-drawing tree that shows the full workspace shape:
//! charter hierarchy → plans → actions → child actions.
//!
//! Output is a plain `String` — callers decide whether to print it. This
//! module does not perform TTY detection; that is the caller's responsibility.
//!
//! # Example output
//!
//! ```text
//! workspace
//! ├── 📁 Platform
//! │   ├── 📋 Write graph tests  ↻ weekly
//! │   │   └── ● Write graph tests  in progress
//! │   └── ○ Review workspace structure
//! │       ├── ○ Decouple Actions for ActionsFile
//! │       └── ○ Ditto for charters
//! └── 📁 Core Work  [closed]
//!     └── ✓ Ship semantic export
//! ```

use clearhead_core::domain::{Action, ActionState, Charter, DomainModel, Plan};
use termtree::Tree;

// ─────────────────────────────────────────────────────────────────────────────
// Charter-only tree  (for `read charters`)
// ─────────────────────────────────────────────────────────────────────────────

/// Render a charter hierarchy with open action counts.
///
/// Shows only charters — no plans or actions inline. Each node displays the
/// charter title and, when non-zero, the count of open actions it contains.
///
/// ```text
/// Platform  (7 open)
/// ├── Core Work  (3 open)  [active]
/// └── Documentation
/// ```
pub fn render_charter_tree(model: &DomainModel) -> String {
    let root_charters = find_root_charters(model);

    let tree = if root_charters.len() == 1 {
        build_charter_only_tree(root_charters[0], model)
    } else {
        let mut root = Tree::new("workspace".to_string());
        for charter in root_charters {
            root.push(build_charter_only_tree(charter, model));
        }
        root
    };

    tree.to_string()
}

fn build_charter_only_tree(charter: &Charter, model: &DomainModel) -> Tree<String> {
    let mut node = Tree::new(charter_summary_label(charter));
    for child in find_charter_children(charter, model) {
        node.push(build_charter_only_tree(child, model));
    }
    node
}

fn charter_summary_label(charter: &Charter) -> String {
    let open = charter
        .actions
        .iter()
        .filter(|a| !matches!(a.state, ActionState::Completed | ActionState::Cancelled))
        .count();

    let open_tag = if open > 0 {
        format!("  ({} open)", open)
    } else {
        String::new()
    };

    let state_tag = charter
        .state
        .map(|s| format!("  [{}]", s.to_string().to_lowercase()))
        .unwrap_or_default();

    format!("📁 {}{}{}", charter.title, open_tag, state_tag)
}

/// Render a `DomainModel` as a Unicode tree string.
///
/// Root charters (those with no parent, or whose parent is not found in the
/// model) appear at the top level. When there is more than one root charter a
/// `workspace` root node is added; when there is exactly one it is the root.
pub fn render_domain_tree(model: &DomainModel) -> String {
    let root_charters = find_root_charters(model);

    let tree = if root_charters.len() == 1 {
        build_charter_tree(root_charters[0], model)
    } else {
        let mut root = Tree::new("workspace".to_string());
        for charter in root_charters {
            root.push(build_charter_tree(charter, model));
        }
        root
    };

    tree.to_string()
}

// ---------------------------------------------------------------------------
// Charter hierarchy
// ---------------------------------------------------------------------------

fn find_root_charters<'a>(model: &'a DomainModel) -> Vec<&'a Charter> {
    let known_titles: std::collections::HashSet<String> = model
        .charters
        .iter()
        .map(|c| c.title.to_lowercase())
        .collect();

    model
        .charters
        .iter()
        .filter(|c| {
            c.parent
                .as_ref()
                .map(|p| !known_titles.contains(&p.to_lowercase()))
                .unwrap_or(true)
        })
        .collect()
}

fn build_charter_tree(charter: &Charter, model: &DomainModel) -> Tree<String> {
    let mut node = Tree::new(charter_label(charter));

    // Sub-charters before plans and actions
    for sub in find_charter_children(charter, model) {
        node.push(build_charter_tree(sub, model));
    }

    // Plans with their scoped actions
    for plan in &charter.plans {
        node.push(build_plan_tree(plan, charter));
    }

    // Actions with no plan and no parent — direct charter items
    for action in charter_root_actions(charter) {
        node.push(build_action_tree(action, &charter.actions));
    }

    node
}

fn find_charter_children<'a>(charter: &Charter, model: &'a DomainModel) -> Vec<&'a Charter> {
    let title_lower = charter.title.to_lowercase();
    model
        .charters
        .iter()
        .filter(|c| {
            c.parent
                .as_deref()
                .map(|p| p.to_lowercase() == title_lower)
                .unwrap_or(false)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Plan subtree
// ---------------------------------------------------------------------------

fn build_plan_tree(plan: &Plan, charter: &Charter) -> Tree<String> {
    let mut node = Tree::new(plan_label(plan));

    for action in plan_root_actions(plan, charter) {
        node.push(build_action_tree(action, &charter.actions));
    }

    node
}

/// Actions that belong to this plan and have no parent action.
fn plan_root_actions<'a>(plan: &Plan, charter: &'a Charter) -> Vec<&'a Action> {
    charter
        .actions
        .iter()
        .filter(|a| a.plan_id == Some(plan.id) && a.parent_id.is_none())
        .collect()
}

// ---------------------------------------------------------------------------
// Action subtree
// ---------------------------------------------------------------------------

fn build_action_tree(action: &Action, all_actions: &[Action]) -> Tree<String> {
    let mut node = Tree::new(action_label(action));

    for child in action_children(action, all_actions) {
        node.push(build_action_tree(child, all_actions));
    }

    node
}

/// Direct children of an action within the same charter action list.
fn action_children<'a>(action: &Action, all_actions: &'a [Action]) -> Vec<&'a Action> {
    all_actions
        .iter()
        .filter(|a| a.parent_id == Some(action.id))
        .collect()
}

/// Actions in a charter with no plan and no parent — top-level charter items.
fn charter_root_actions(charter: &Charter) -> Vec<&Action> {
    charter
        .actions
        .iter()
        .filter(|a| a.plan_id.is_none() && a.parent_id.is_none())
        .collect()
}

// ---------------------------------------------------------------------------
// Node labels
// ---------------------------------------------------------------------------

fn charter_label(charter: &Charter) -> String {
    let state_tag = charter
        .state
        .map(|s| format!("  [{}]", s.to_string().to_lowercase()))
        .unwrap_or_default();
    format!("📁 {}{}", charter.title, state_tag)
}

fn plan_label(plan: &Plan) -> String {
    let recurrence_tag = plan
        .recurrence
        .as_ref()
        .map(|r| format!("  ↻ {}", r.frequency.to_lowercase()))
        .unwrap_or_default();
    format!("📋 {}{}", plan.name, recurrence_tag)
}

fn action_label(action: &Action) -> String {
    let icon = action_icon(action.state);
    let state_tag = match action.state {
        ActionState::NotStarted => String::new(),
        other => format!("  {}", state_str(other)),
    };
    format!("{} {}{}", icon, action.name, state_tag)
}

fn action_icon(state: ActionState) -> &'static str {
    match state {
        ActionState::NotStarted => "○",
        ActionState::InProgress => "●",
        ActionState::Completed => "✓",
        ActionState::BlockedOrAwaiting => "⊘",
        ActionState::Cancelled => "✗",
    }
}

fn state_str(state: ActionState) -> &'static str {
    match state {
        ActionState::NotStarted => "not started",
        ActionState::InProgress => "in progress",
        ActionState::Completed => "done",
        ActionState::BlockedOrAwaiting => "blocked",
        ActionState::Cancelled => "cancelled",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_core::domain::{Action, ActionState, Charter, DomainModel, Plan};
    use uuid::Uuid;

    fn make_action(name: &str, state: ActionState) -> Action {
        Action {
            id: Uuid::now_v7(),
            name: name.to_string(),
            state,
            ..Default::default()
        }
    }

    fn make_plan(name: &str) -> Plan {
        Plan {
            id: Uuid::now_v7(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn single_root_charter_renders_without_workspace_wrapper() {
        let model = DomainModel {
            objectives: vec![],
            charters: vec![Charter {
                id: Uuid::now_v7(),
                title: "Platform".to_string(),
                ..Default::default()
            }],
        };
        let out = render_domain_tree(&model);
        assert!(out.starts_with("📁 Platform"));
        assert!(!out.contains("workspace"));
    }

    #[test]
    fn multiple_root_charters_get_workspace_wrapper() {
        let model = DomainModel {
            objectives: vec![],
            charters: vec![
                Charter { id: Uuid::now_v7(), title: "Alpha".to_string(), ..Default::default() },
                Charter { id: Uuid::now_v7(), title: "Beta".to_string(), ..Default::default() },
            ],
        };
        let out = render_domain_tree(&model);
        assert!(out.starts_with("workspace"));
        assert!(out.contains("📁 Alpha"));
        assert!(out.contains("📁 Beta"));
    }

    #[test]
    fn sub_charter_appears_under_parent() {
        let model = DomainModel {
            objectives: vec![],
            charters: vec![
                Charter { id: Uuid::now_v7(), title: "Root".to_string(), ..Default::default() },
                Charter {
                    id: Uuid::now_v7(),
                    title: "Child".to_string(),
                    parent: Some("Root".to_string()),
                    ..Default::default()
                },
            ],
        };
        let out = render_domain_tree(&model);
        let root_pos = out.find("📁 Root").unwrap();
        let child_pos = out.find("📁 Child").unwrap();
        assert!(child_pos > root_pos, "child should appear after root");
        // Child should be indented (has a tree connector before it)
        let child_line = out.lines().find(|l| l.contains("📁 Child")).unwrap();
        assert!(child_line.contains('└') || child_line.contains('├'));
    }

    #[test]
    fn plan_appears_under_charter() {
        let plan = make_plan("Write tests");
        let model = DomainModel {
            objectives: vec![],
            charters: vec![Charter {
                id: Uuid::now_v7(),
                title: "Platform".to_string(),
                plans: vec![plan],
                ..Default::default()
            }],
        };
        let out = render_domain_tree(&model);
        assert!(out.contains("📋 Write tests"));
        let charter_pos = out.find("📁 Platform").unwrap();
        let plan_pos = out.find("📋 Write tests").unwrap();
        assert!(plan_pos > charter_pos);
    }

    #[test]
    fn action_icons_reflect_state() {
        let plan_id = Uuid::now_v7();
        let mut act = make_action("Do work", ActionState::InProgress);
        act.plan_id = Some(plan_id);

        let plan = Plan { id: plan_id, name: "Some plan".to_string(), ..Default::default() };
        let model = DomainModel {
            objectives: vec![],
            charters: vec![Charter {
                id: Uuid::now_v7(),
                title: "Work".to_string(),
                plans: vec![plan],
                actions: vec![act],
                ..Default::default()
            }],
        };
        let out = render_domain_tree(&model);
        assert!(out.contains("● Do work  in progress"));
    }

    #[test]
    fn child_actions_nest_under_parent() {
        let parent_id = Uuid::now_v7();
        let parent = Action { id: parent_id, name: "Parent".to_string(), ..Default::default() };
        let child = Action {
            id: Uuid::now_v7(),
            name: "Child".to_string(),
            parent_id: Some(parent_id),
            ..Default::default()
        };
        let model = DomainModel {
            objectives: vec![],
            charters: vec![Charter {
                id: Uuid::now_v7(),
                title: "Work".to_string(),
                actions: vec![parent, child],
                ..Default::default()
            }],
        };
        let out = render_domain_tree(&model);
        let parent_pos = out.find("○ Parent").unwrap();
        let child_pos = out.find("○ Child").unwrap();
        assert!(child_pos > parent_pos);
        let child_line = out.lines().find(|l| l.contains("○ Child")).unwrap();
        assert!(child_line.contains('└') || child_line.contains('├'));
    }

    #[test]
    fn recurring_plan_shows_frequency_tag() {
        use clearhead_core::domain::Recurrence;
        let plan = Plan {
            id: Uuid::now_v7(),
            name: "Weekly review".to_string(),
            recurrence: Some(Recurrence {
                frequency: "weekly".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let model = DomainModel {
            objectives: vec![],
            charters: vec![Charter {
                id: Uuid::now_v7(),
                title: "Ops".to_string(),
                plans: vec![plan],
                ..Default::default()
            }],
        };
        let out = render_domain_tree(&model);
        assert!(out.contains("📋 Weekly review  ↻ weekly"));
    }
}

#[cfg(test)]
mod visual_tests {
    use super::*;
    use clearhead_core::domain::*;
    use uuid::Uuid;

    /// Run with `cargo test print_sample_tree -- --nocapture --ignored` to see live output.
    #[test]
    #[ignore]
    fn print_sample_tree() {
        let plan_id = Uuid::now_v7();
        let act2_id = Uuid::now_v7();
        let model = DomainModel {
            objectives: vec![],
            charters: vec![
                Charter {
                    id: Uuid::now_v7(),
                    title: "Platform".to_string(),
                    state: Some(CharterState::Active),
                    plans: vec![Plan {
                        id: plan_id,
                        name: "Write graph tests".to_string(),
                        recurrence: Some(Recurrence { frequency: "weekly".to_string(), ..Default::default() }),
                        ..Default::default()
                    }],
                    actions: vec![
                        Action { id: Uuid::now_v7(), name: "Write graph tests".to_string(), state: ActionState::InProgress, plan_id: Some(plan_id), ..Default::default() },
                        Action { id: act2_id, name: "Review workspace structure".to_string(), ..Default::default() },
                        Action { id: Uuid::now_v7(), name: "Decouple Actions".to_string(), parent_id: Some(act2_id), ..Default::default() },
                        Action { id: Uuid::now_v7(), name: "Ditto for charters".to_string(), parent_id: Some(act2_id), ..Default::default() },
                    ],
                    ..Default::default()
                },
                Charter {
                    id: Uuid::now_v7(),
                    title: "Core Work".to_string(),
                    parent: Some("Platform".to_string()),
                    actions: vec![Action { id: Uuid::now_v7(), name: "Ship semantic export".to_string(), state: ActionState::Completed, ..Default::default() }],
                    ..Default::default()
                },
            ],
        };
        println!("\n{}", render_domain_tree(&model));
    }
}
