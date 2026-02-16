use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;

use crate::models::*;

const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";

/// Check if an item's iteration matches the filter.
/// Supported filter formats:
/// - `@all` - matches all iterations (no filtering)
/// - `@current` - matches the iteration that contains today's date
/// - `@previous` - matches the iteration before current
/// - `@current,@previous` - matches either current or previous
/// - `<iteration name>` - exact match on iteration title
fn matches_iteration_filter(
    iteration_title: Option<&str>,
    iteration_start: Option<&str>,
    filter: &str,
) -> bool {
    // @all means no filtering
    if filter == "@all" {
        return true;
    }

    // If filter requires an iteration but item has none, no match
    if filter.starts_with('@') && iteration_title.is_none() {
        return false;
    }

    // Parse filter parts (e.g., "@current,@previous")
    let filter_parts: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();

    for part in filter_parts {
        match part {
            "@current" => {
                if is_current_iteration(iteration_start) {
                    return true;
                }
            }
            "@previous" => {
                // We need context of all iterations to determine "previous"
                // For now, we'll use a heuristic: previous iteration ended within the last 2 weeks
                if is_recent_past_iteration(iteration_start) {
                    return true;
                }
            }
            name => {
                // Exact match on iteration title
                if iteration_title == Some(name) {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if the iteration start date indicates it's the current iteration.
/// Assumes 2-week sprints by default.
fn is_current_iteration(start_date: Option<&str>) -> bool {
    let Some(start_str) = start_date else {
        return false;
    };

    let Ok(start) = NaiveDate::parse_from_str(start_str, "%Y-%m-%d") else {
        return false;
    };

    let today = Utc::now().date_naive();
    let sprint_length = 14; // Default 2-week sprint

    // Current iteration: start <= today < start + sprint_length
    start <= today && today < start + chrono::Duration::days(sprint_length)
}

/// Check if iteration is from the recent past (likely previous iteration).
/// Uses heuristic: started between 2-4 weeks ago.
fn is_recent_past_iteration(start_date: Option<&str>) -> bool {
    let Some(start_str) = start_date else {
        return false;
    };

    let Ok(start) = NaiveDate::parse_from_str(start_str, "%Y-%m-%d") else {
        return false;
    };

    let today = Utc::now().date_naive();
    let sprint_length = 14;

    // Previous iteration: started 2-4 weeks ago
    let prev_start = today - chrono::Duration::days(sprint_length * 2);
    let prev_end = today - chrono::Duration::days(sprint_length);

    start >= prev_start && start < prev_end
}

#[derive(Debug, Default)]
pub struct FetchStats {
    pub total_items: usize,
    pub archived: usize,
    pub wrong_column: usize,
    pub not_issue: usize,
    pub filtered_by_time: usize,
    pub filtered_by_iteration: usize,
    pub columns_seen: HashSet<String>,
    pub iterations_seen: HashSet<String>,
}

pub struct GitHubClient {
    client: Client,
    token: String,
}

impl GitHubClient {
    pub fn new(token: &str) -> Self {
        Self {
            client: Client::new(),
            token: token.to_string(),
        }
    }

    /// Resolve a project identifier to a GraphQL node ID
    /// Supports:
    /// - Direct node ID (starts with "PVT_")
    /// - Owner/number format (e.g., "myorg/5" or "myuser/3")
    pub async fn resolve_project_id(&self, project_id: &str) -> Result<String> {
        // If it looks like a node ID, return as-is
        if project_id.starts_with("PVT_") {
            return Ok(project_id.to_string());
        }

        // Parse owner/number format
        let parts: Vec<&str> = project_id.split('/').collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid project ID format. Use 'owner/number' (e.g., 'myorg/5') or a GraphQL node ID (starting with 'PVT_')"
            ));
        }

        let owner = parts[0];
        let number: u32 = parts[1]
            .parse()
            .context("Project number must be a positive integer")?;

        self.lookup_project_id(owner, number).await
    }

    /// Look up a project's node ID by owner and project number
    async fn lookup_project_id(&self, owner: &str, number: u32) -> Result<String> {
        // Try organization first, then user
        if let Ok(id) = self.lookup_org_project(owner, number).await {
            return Ok(id);
        }

        self.lookup_user_project(owner, number).await
    }

    async fn lookup_org_project(&self, org: &str, number: u32) -> Result<String> {
        let query = r#"
            query($org: String!, $number: Int!) {
                organization(login: $org) {
                    projectV2(number: $number) {
                        id
                    }
                }
            }
        "#;

        let variables = json!({
            "org": org,
            "number": number
        });

        let response = self.execute_query(query, &variables).await?;

        #[derive(Deserialize)]
        struct OrgData {
            organization: Option<OrgNode>,
        }

        #[derive(Deserialize)]
        struct OrgNode {
            #[serde(rename = "projectV2")]
            project_v2: Option<ProjectId>,
        }

        #[derive(Deserialize)]
        struct ProjectId {
            id: String,
        }

        let parsed: GraphQLResponse<OrgData> =
            serde_json::from_str(&response).context("Failed to parse GitHub response")?;

        // Check for GraphQL errors
        if let Some(errors) = &parsed.errors {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            return Err(anyhow!("GitHub API error: {}", messages.join(", ")));
        }

        if let Some(data) = parsed.data {
            if let Some(org_data) = data.organization {
                if let Some(project) = org_data.project_v2 {
                    return Ok(project.id);
                }
                return Err(anyhow!(
                    "Project #{} not found in organization '{}'. Check the project number and your token permissions (needs 'read:project' scope).",
                    number, org
                ));
            }
            return Err(anyhow!(
                "Organization '{}' not found or not accessible. Check the org name and your token permissions.",
                org
            ));
        }

        Err(anyhow!("Organization project not found"))
    }

    async fn lookup_user_project(&self, user: &str, number: u32) -> Result<String> {
        let query = r#"
            query($user: String!, $number: Int!) {
                user(login: $user) {
                    projectV2(number: $number) {
                        id
                    }
                }
            }
        "#;

        let variables = json!({
            "user": user,
            "number": number
        });

        let response = self.execute_query(query, &variables).await?;

        #[derive(Deserialize)]
        struct UserData {
            user: Option<UserNode>,
        }

        #[derive(Deserialize)]
        struct UserNode {
            #[serde(rename = "projectV2")]
            project_v2: Option<ProjectId>,
        }

        #[derive(Deserialize)]
        struct ProjectId {
            id: String,
        }

        let parsed: GraphQLResponse<UserData> =
            serde_json::from_str(&response).context("Failed to parse GitHub response")?;

        if let Some(data) = parsed.data {
            if let Some(user) = data.user {
                if let Some(project) = user.project_v2 {
                    return Ok(project.id);
                }
            }
        }

        Err(anyhow!(
            "Project not found. Check that the owner and project number are correct."
        ))
    }

    async fn execute_query(
        &self,
        query: &str,
        variables: &serde_json::Value,
    ) -> Result<String> {
        let response = self
            .client
            .post(GITHUB_GRAPHQL_URL)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "doner-cli")
            .json(&json!({
                "query": query,
                "variables": variables
            }))
            .send()
            .await
            .context("Failed to send request to GitHub API")?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(anyhow!("GitHub API error ({}): {}", status, body));
        }

        Ok(body)
    }

    pub async fn fetch_project_issues(
        &self,
        project_node_id: &str,
        column_name: &str,
        since: Option<DateTime<Utc>>,
        iteration_filter: Option<&str>,
        collect_stats: bool,
    ) -> Result<(Vec<Issue>, FetchStats)> {
        let mut all_issues = Vec::new();
        let mut cursor: Option<String> = None;
        let mut stats = FetchStats::default();

        loop {
            let (issues, page_info, page_stats) = self
                .fetch_project_items_page(project_node_id, column_name, iteration_filter, cursor.as_deref(), collect_stats)
                .await?;

            stats.total_items += page_stats.total_items;
            stats.archived += page_stats.archived;
            stats.wrong_column += page_stats.wrong_column;
            stats.not_issue += page_stats.not_issue;
            stats.filtered_by_iteration += page_stats.filtered_by_iteration;
            stats.columns_seen.extend(page_stats.columns_seen);
            stats.iterations_seen.extend(page_stats.iterations_seen);

            for issue in issues {
                // Filter by time if specified
                if let Some(since_time) = since {
                    if let Some(closed_at) = issue.closed_at {
                        if closed_at < since_time {
                            stats.filtered_by_time += 1;
                            continue;
                        }
                    } else {
                        // If no closed_at and we have a time filter, skip
                        stats.filtered_by_time += 1;
                        continue;
                    }
                }
                all_issues.push(issue);
            }

            if !page_info.has_next_page {
                break;
            }
            cursor = page_info.end_cursor;
        }

        Ok((all_issues, stats))
    }

    async fn fetch_project_items_page(
        &self,
        project_node_id: &str,
        column_name: &str,
        iteration_filter: Option<&str>,
        cursor: Option<&str>,
        collect_stats: bool,
    ) -> Result<(Vec<Issue>, PageInfo, FetchStats)> {
        let query = r#"
            query($projectId: ID!, $cursor: String, $statusField: String!, $iterationField: String!) {
                node(id: $projectId) {
                    ... on ProjectV2 {
                        items(first: 100, after: $cursor) {
                            pageInfo {
                                hasNextPage
                                endCursor
                            }
                            nodes {
                                id
                                isArchived
                                fieldValueByName(name: $statusField) {
                                    ... on ProjectV2ItemFieldSingleSelectValue {
                                        __typename
                                        name
                                    }
                                }
                                iteration: fieldValueByName(name: $iterationField) {
                                    ... on ProjectV2ItemFieldIterationValue {
                                        __typename
                                        title
                                        startDate
                                    }
                                }
                                content {
                                    __typename
                                    ... on Issue {
                                        number
                                        title
                                        url
                                        closedAt
                                        repository {
                                            nameWithOwner
                                        }
                                        parent {
                                            number
                                            title
                                            url
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        "#;

        // Allow overriding field names via environment variables
        let status_field = std::env::var("DONER_STATUS_FIELD").unwrap_or_else(|_| "Status".to_string());
        let iteration_field = std::env::var("DONER_ITERATION_FIELD").unwrap_or_else(|_| "Iteration".to_string());

        let variables = json!({
            "projectId": project_node_id,
            "cursor": cursor,
            "statusField": status_field,
            "iterationField": iteration_field
        });

        let response = self.execute_query(query, &variables).await?;

        let parsed: GraphQLResponse<ProjectData> =
            serde_json::from_str(&response).context("Failed to parse GitHub response")?;

        if let Some(errors) = parsed.errors {
            let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
            return Err(anyhow!("GraphQL errors: {}", messages.join(", ")));
        }

        let project = parsed
            .data
            .and_then(|d| d.node)
            .ok_or_else(|| anyhow!("Project not found. Make sure the project ID is correct."))?;

        let mut issues = Vec::new();
        let mut stats = FetchStats::default();
        stats.total_items = project.items.nodes.len();

        for item in project.items.nodes {
            // Skip archived items (hidden in GitHub UI)
            if item.is_archived {
                stats.archived += 1;
                continue;
            }

            // Check if item is in the specified column
            let item_column = item
                .field_value_by_name
                .as_ref()
                .and_then(|fv| fv.name());

            // Collect column names for debug output
            if collect_stats {
                if let Some(col) = item_column {
                    stats.columns_seen.insert(col.to_string());
                } else {
                    stats.columns_seen.insert("<no status>".to_string());
                }
            }

            if item_column != Some(column_name) {
                stats.wrong_column += 1;
                continue;
            }

            // Get iteration info
            let item_iteration = item.iteration.as_ref().and_then(|iv| iv.title());
            let item_iteration_start = item.iteration.as_ref().and_then(|iv| iv.start_date());

            // Collect iteration names for debug output
            if collect_stats {
                if let Some(iter) = item_iteration {
                    stats.iterations_seen.insert(iter.to_string());
                } else {
                    stats.iterations_seen.insert("<no iteration>".to_string());
                }
            }

            // Filter by iteration if specified
            if let Some(filter) = iteration_filter {
                if !matches_iteration_filter(item_iteration, item_iteration_start, filter) {
                    stats.filtered_by_iteration += 1;
                    continue;
                }
            }

            // Extract issue content
            match item.content {
                Some(ItemContent::Issue(content)) => {
                    let parent = content.parent.map(|p| crate::models::ParentIssue {
                        number: p.number,
                        title: p.title,
                        url: p.url,
                    });

                    issues.push(Issue {
                        number: content.number,
                        title: content.title,
                        url: content.url,
                        closed_at: content.closed_at,
                        repository: content.repository.name_with_owner,
                        parent,
                    });
                }
                _ => {
                    stats.not_issue += 1;
                }
            }
        }

        Ok((issues, project.items.page_info, stats))
    }
}
