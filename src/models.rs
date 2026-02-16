use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub closed_at: Option<DateTime<Utc>>,
    pub parent: Option<ParentIssue>,
    pub repository: String,
}

#[derive(Debug, Clone)]
pub struct ParentIssue {
    #[allow(dead_code)]
    pub number: u64,
    pub title: String,
    pub url: String,
}

// GraphQL response structures

#[derive(Debug, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
pub struct GraphQLError {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectData {
    pub node: Option<ProjectNode>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectNode {
    pub items: ItemConnection,
}

#[derive(Debug, Deserialize)]
pub struct ItemConnection {
    pub nodes: Vec<ProjectItem>,
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectItem {
    #[allow(dead_code)]
    pub id: String,
    #[serde(rename = "isArchived")]
    pub is_archived: bool,
    #[serde(rename = "fieldValueByName")]
    pub field_value_by_name: Option<FieldValue>,
    pub iteration: Option<IterationValue>,
    pub content: Option<ItemContent>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum FieldValue {
    ProjectV2ItemFieldSingleSelectValue { name: Option<String> },
    #[serde(other)]
    Other,
}

impl FieldValue {
    pub fn name(&self) -> Option<&str> {
        match self {
            FieldValue::ProjectV2ItemFieldSingleSelectValue { name } => name.as_deref(),
            FieldValue::Other => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum IterationValue {
    ProjectV2ItemFieldIterationValue {
        title: Option<String>,
        #[serde(rename = "startDate")]
        start_date: Option<String>,
    },
    #[serde(other)]
    Other,
}

impl IterationValue {
    pub fn title(&self) -> Option<&str> {
        match self {
            IterationValue::ProjectV2ItemFieldIterationValue { title, .. } => title.as_deref(),
            IterationValue::Other => None,
        }
    }

    pub fn start_date(&self) -> Option<&str> {
        match self {
            IterationValue::ProjectV2ItemFieldIterationValue { start_date, .. } => start_date.as_deref(),
            IterationValue::Other => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum ItemContent {
    Issue(IssueContent),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct IssueContent {
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(rename = "closedAt")]
    pub closed_at: Option<DateTime<Utc>>,
    pub repository: RepositoryInfo,
    pub parent: Option<ParentIssueContent>,
}

#[derive(Debug, Deserialize)]
pub struct RepositoryInfo {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
}

#[derive(Debug, Deserialize)]
pub struct ParentIssueContent {
    pub number: u64,
    pub title: String,
    pub url: String,
}
