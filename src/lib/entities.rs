pub struct RootAction {
    core: CoreActionProperties,
    story: Option<String>,
    children: Vec<ChildAction>,
}

pub struct ChildAction {
    core: CoreActionProperties,
    grandchildren: Vec<GrandChildAction>,
}

pub struct GrandChildAction {
    core: CoreActionProperties,
    great_grandchildren: Vec<GrandChildAction>,
}

pub struct GreatGrandChildAction {
    core: CoreActionProperties,
    great_great_grandchildren: Vec<GrandChildAction>,
}

pub struct DoubleGreatGrandChildAction {
    core: CoreActionProperties,
    leaf_actions: Vec<LeafAction>,
}

pub struct LeafAction {
    core: CoreActionProperties,
}

pub struct CoreActionProperties {
    name: String,
    state: ActionState,
    description: String,
    priority: u8,
    context_list: Vec<String>,
    do_date: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    NotStarted,
    Completed,
    InProgress,
    BlockedorAwaiting,
    Cancelled,
}

pub struct ExtendedDateTime<Tz: chrono::TimeZone> {
    date: chrono::DateTime<Tz>,
    recurrance: Option<Recurrance>,
}

pub enum Recurrance {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}
