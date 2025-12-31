-- Import JSON to SQL Schema
-- This script shows how to load JSON data into the canonical SQL schema
-- Assumes SQLite with JSON support

-- =============================================================================
-- Setup: Create a temporary table to hold JSON data
-- =============================================================================

CREATE TEMP TABLE json_import (
    json_data TEXT
);

-- Load your JSON file (this is database-specific)
-- For SQLite CLI:
-- .mode line
-- INSERT INTO json_import VALUES(readfile('examples/sample.json'));

-- =============================================================================
-- Import Root Actions
-- =============================================================================

INSERT INTO actions (
    id,
    parent_id,
    depth,
    state,
    name,
    description,
    priority,
    story,
    do_datetime,
    do_duration,
    completed_datetime
)
SELECT
    COALESCE(
        json_extract(value, '$.id'),
        lower(hex(randomblob(16)))  -- Generate UUID if not present
    ) as id,
    NULL as parent_id,  -- Root actions have no parent
    0 as depth,         -- Root actions are depth 0
    json_extract(value, '$.state'),
    json_extract(value, '$.name'),
    json_extract(value, '$.description'),
    CAST(json_extract(value, '$.priority') AS INTEGER),
    json_extract(value, '$.story'),
    json_extract(value, '$.doDate.datetime'),
    CAST(json_extract(value, '$.doDate.duration') AS INTEGER),
    json_extract(value, '$.completedDate')
FROM json_import,
     json_each(json_import.json_data, '$.actions');

-- =============================================================================
-- Import Contexts
-- =============================================================================

INSERT INTO action_contexts (action_id, context)
SELECT
    COALESCE(
        json_extract(action.value, '$.id'),
        lower(hex(randomblob(16)))
    ) as action_id,
    context.value as context
FROM json_import,
     json_each(json_import.json_data, '$.actions') as action,
     json_each(json_extract(action.value, '$.contexts')) as context
WHERE json_extract(action.value, '$.contexts') IS NOT NULL;

-- =============================================================================
-- Import Recurrence Rules
-- =============================================================================

INSERT INTO action_recurrence (
    action_id,
    frequency,
    interval,
    count,
    until_date,
    by_minute,
    by_hour,
    by_day,
    by_month_day,
    by_month
)
SELECT
    COALESCE(
        json_extract(action.value, '$.id'),
        lower(hex(randomblob(16)))
    ) as action_id,
    json_extract(action.value, '$.doDate.recurrence.frequency'),
    CAST(json_extract(action.value, '$.doDate.recurrence.interval') AS INTEGER),
    CAST(json_extract(action.value, '$.doDate.recurrence.count') AS INTEGER),
    json_extract(action.value, '$.doDate.recurrence.until'),
    json_extract(action.value, '$.doDate.recurrence.byMinute'),
    json_extract(action.value, '$.doDate.recurrence.byHour'),
    json_extract(action.value, '$.doDate.recurrence.byDay'),
    json_extract(action.value, '$.doDate.recurrence.byMonthDay'),
    json_extract(action.value, '$.doDate.recurrence.byMonth')
FROM json_import,
     json_each(json_import.json_data, '$.actions') as action
WHERE json_extract(action.value, '$.doDate.recurrence') IS NOT NULL;

-- =============================================================================
-- Import Child Actions (Depth 1)
-- =============================================================================

INSERT INTO actions (
    id,
    parent_id,
    depth,
    state,
    name,
    description,
    priority,
    do_datetime,
    do_duration,
    completed_datetime
)
SELECT
    COALESCE(
        json_extract(child.value, '$.id'),
        lower(hex(randomblob(16)))
    ) as id,
    COALESCE(
        json_extract(parent.value, '$.id'),
        lower(hex(randomblob(16)))
    ) as parent_id,
    1 as depth,
    json_extract(child.value, '$.state'),
    json_extract(child.value, '$.name'),
    json_extract(child.value, '$.description'),
    CAST(json_extract(child.value, '$.priority') AS INTEGER),
    json_extract(child.value, '$.doDate.datetime'),
    CAST(json_extract(child.value, '$.doDate.duration') AS INTEGER),
    json_extract(child.value, '$.completedDate')
FROM json_import,
     json_each(json_import.json_data, '$.actions') as parent,
     json_each(json_extract(parent.value, '$.children')) as child
WHERE json_extract(parent.value, '$.children') IS NOT NULL;

-- Import contexts for child actions
INSERT INTO action_contexts (action_id, context)
SELECT
    COALESCE(
        json_extract(child.value, '$.id'),
        lower(hex(randomblob(16)))
    ) as action_id,
    context.value as context
FROM json_import,
     json_each(json_import.json_data, '$.actions') as parent,
     json_each(json_extract(parent.value, '$.children')) as child,
     json_each(json_extract(child.value, '$.contexts')) as context
WHERE json_extract(child.value, '$.contexts') IS NOT NULL;

-- NOTE: For deeper nesting levels (depth 2-5), repeat the pattern above
-- with additional levels of json_each() nesting

-- =============================================================================
-- Validation
-- =============================================================================

-- Verify import counts
SELECT 'Root actions imported:' as description, COUNT(*) as count FROM actions WHERE depth = 0
UNION ALL
SELECT 'Child actions imported:', COUNT(*) FROM actions WHERE depth > 0
UNION ALL
SELECT 'Total contexts:', COUNT(*) FROM action_contexts
UNION ALL
SELECT 'Recurring actions:', COUNT(*) FROM action_recurrence;

-- Check for orphaned children (should be 0)
SELECT 'Orphaned children (should be 0):' as description, COUNT(*) as count
FROM actions a
WHERE a.parent_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM actions p WHERE p.id = a.parent_id);

-- =============================================================================
-- Cleanup
-- =============================================================================

DROP TABLE json_import;
