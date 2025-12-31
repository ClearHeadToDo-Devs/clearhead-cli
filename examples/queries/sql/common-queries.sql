-- Common SQL Queries for .actions Database
-- These queries assume you're using the canonical schema from schema/actions.sql

-- =============================================================================
-- Basic Filters
-- =============================================================================

-- All priority 1 actions
SELECT * FROM actions WHERE priority = 1;

-- All completed actions
SELECT * FROM actions WHERE state = 'completed';

-- All in-progress actions
SELECT * FROM actions WHERE state = 'in_progress';

-- All blocked actions
SELECT * FROM actions WHERE state = 'blocked';

-- All not-started actions
SELECT * FROM actions WHERE state = 'not_started';

-- =============================================================================
-- Context-Based Queries
-- =============================================================================

-- Actions in 'work' context
SELECT a.* FROM actions a
JOIN action_contexts c ON a.id = c.action_id
WHERE c.context = 'work';

-- Actions in 'work' OR 'urgent' context
SELECT DISTINCT a.* FROM actions a
JOIN action_contexts c ON a.id = c.action_id
WHERE c.context IN ('work', 'urgent');

-- Actions with multiple contexts
SELECT a.*, COUNT(c.context) as context_count
FROM actions a
JOIN action_contexts c ON a.id = c.action_id
GROUP BY a.id
HAVING COUNT(c.context) > 1;

-- =============================================================================
-- Date-Based Queries
-- =============================================================================

-- Actions due today
SELECT * FROM actions
WHERE date(do_datetime) = date('now');

-- Actions due this week
SELECT * FROM actions
WHERE do_datetime >= date('now', 'weekday 0', '-7 days')
  AND do_datetime < date('now', 'weekday 0');

-- Actions due in the next 7 days
SELECT * FROM actions
WHERE do_datetime >= datetime('now')
  AND do_datetime < datetime('now', '+7 days');

-- Overdue not-started actions
SELECT * FROM actions
WHERE state = 'not_started'
  AND do_datetime < datetime('now');

-- Actions completed today
SELECT * FROM actions
WHERE date(completed_datetime) = date('now');

-- Actions completed this week
SELECT * FROM actions
WHERE completed_datetime >= date('now', 'weekday 0', '-7 days')
  AND completed_datetime < date('now', 'weekday 0');

-- =============================================================================
-- Story/Project Queries
-- =============================================================================

-- Actions in a specific project
SELECT * FROM actions
WHERE story = 'Weekly Errands';

-- All actions grouped by project
SELECT story, COUNT(*) as action_count
FROM actions
WHERE story IS NOT NULL
GROUP BY story
ORDER BY action_count DESC;

-- =============================================================================
-- Hierarchical Queries
-- =============================================================================

-- Root actions only
SELECT * FROM actions WHERE depth = 0;
-- Or equivalently:
SELECT * FROM actions WHERE parent_id IS NULL;

-- Direct children of a specific action
SELECT * FROM actions
WHERE parent_id = 'some-action-id';

-- All actions with children
SELECT DISTINCT p.*
FROM actions p
JOIN actions c ON c.parent_id = p.id;

-- All descendants of an action (recursive)
WITH RECURSIVE descendants AS (
  SELECT * FROM actions WHERE id = 'some-action-id'
  UNION ALL
  SELECT a.* FROM actions a
  JOIN descendants d ON a.parent_id = d.id
)
SELECT * FROM descendants WHERE id != 'some-action-id';

-- All ancestors of an action (recursive)
WITH RECURSIVE ancestors AS (
  SELECT * FROM actions WHERE id = 'some-action-id'
  UNION ALL
  SELECT a.* FROM actions a
  JOIN ancestors d ON d.parent_id = a.id
)
SELECT * FROM ancestors WHERE id != 'some-action-id';

-- Leaf actions (actions with no children)
SELECT a.*
FROM actions a
LEFT JOIN actions c ON c.parent_id = a.id
WHERE c.id IS NULL;

-- =============================================================================
-- Aggregate Queries
-- =============================================================================

-- Completion rate by project
SELECT
  story,
  COUNT(*) as total,
  SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) as completed,
  SUM(CASE WHEN state = 'in_progress' THEN 1 ELSE 0 END) as in_progress,
  SUM(CASE WHEN state = 'not_started' THEN 1 ELSE 0 END) as not_started,
  SUM(CASE WHEN state = 'blocked' THEN 1 ELSE 0 END) as blocked,
  ROUND(100.0 * SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) / COUNT(*), 2) as completion_rate
FROM actions
WHERE story IS NOT NULL AND depth = 0
GROUP BY story
ORDER BY completion_rate DESC;

-- Action count by state
SELECT state, COUNT(*) as count
FROM actions
GROUP BY state
ORDER BY count DESC;

-- Priority distribution
SELECT
  COALESCE(priority, 999) as priority,
  COUNT(*) as count
FROM actions
GROUP BY priority
ORDER BY priority;

-- Actions by context with counts
SELECT
  context,
  COUNT(*) as action_count
FROM action_contexts
GROUP BY context
ORDER BY action_count DESC;

-- Average time to completion
SELECT
  AVG(
    CAST(
      (julianday(completed_datetime) - julianday(do_datetime)) * 24 * 60
      AS INTEGER
    )
  ) as avg_minutes_to_complete
FROM actions
WHERE completed_datetime IS NOT NULL
  AND do_datetime IS NOT NULL;

-- =============================================================================
-- Complex Combined Queries
-- =============================================================================

-- P1 actions in 'work' context due this week
SELECT a.* FROM actions a
JOIN action_contexts c ON a.id = c.action_id
WHERE a.priority = 1
  AND c.context = 'work'
  AND a.do_datetime >= date('now', 'weekday 0', '-7 days')
  AND a.do_datetime < date('now', 'weekday 0');

-- Not-started P1 actions without children (actionable items)
SELECT a.*
FROM actions a
LEFT JOIN actions c ON c.parent_id = a.id
WHERE a.state = 'not_started'
  AND a.priority = 1
  AND c.id IS NULL;

-- Blocked actions with no blocking reason in description
SELECT * FROM actions
WHERE state = 'blocked'
  AND (description IS NULL OR description NOT LIKE '%block%');

-- Actions overdue by more than 7 days
SELECT
  *,
  CAST(
    (julianday('now') - julianday(do_datetime)) AS INTEGER
  ) as days_overdue
FROM actions
WHERE state != 'completed'
  AND do_datetime < datetime('now', '-7 days')
ORDER BY days_overdue DESC;

-- =============================================================================
-- Recurring Actions
-- =============================================================================

-- All recurring actions
SELECT a.*, r.frequency
FROM actions a
JOIN action_recurrence r ON a.id = r.action_id;

-- Daily recurring actions
SELECT a.*
FROM actions a
JOIN action_recurrence r ON a.id = r.action_id
WHERE r.frequency = 'daily';

-- Recurring actions that repeat on specific days
SELECT a.*, r.by_day
FROM actions a
JOIN action_recurrence r ON a.id = r.action_id
WHERE r.by_day IS NOT NULL;

-- =============================================================================
-- Useful Reporting Queries
-- =============================================================================

-- Dashboard summary
SELECT
  (SELECT COUNT(*) FROM actions WHERE state = 'not_started') as not_started,
  (SELECT COUNT(*) FROM actions WHERE state = 'in_progress') as in_progress,
  (SELECT COUNT(*) FROM actions WHERE state = 'completed') as completed,
  (SELECT COUNT(*) FROM actions WHERE state = 'blocked') as blocked,
  (SELECT COUNT(*) FROM actions WHERE priority = 1) as p1_count,
  (SELECT COUNT(*) FROM actions WHERE do_datetime < datetime('now') AND state != 'completed') as overdue;

-- Top 10 most used contexts
SELECT context, COUNT(*) as usage_count
FROM action_contexts
GROUP BY context
ORDER BY usage_count DESC
LIMIT 10;

-- Most productive projects (by completion count)
SELECT
  story,
  COUNT(*) as total_actions,
  SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) as completed_count
FROM actions
WHERE story IS NOT NULL AND depth = 0
GROUP BY story
ORDER BY completed_count DESC
LIMIT 10;

-- Actions per depth level
SELECT
  depth,
  COUNT(*) as action_count
FROM actions
GROUP BY depth
ORDER BY depth;
