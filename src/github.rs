use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::models::*;

const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";

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
    ) -> Result<Vec<Issue>> {
        let mut all_issues = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let (issues, page_info) = self
                .fetch_project_items_page(project_node_id, column_name, cursor.as_deref())
                .await?;

            for issue in issues {
                // Filter by time if specified
                if let Some(since_time) = since {
                    if let Some(closed_at) = issue.closed_at {
                        if closed_at < since_time {
                            continue;
                        }
                    } else {
                        // If no closed_at and we have a time filter, skip
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

        Ok(all_issues)
    }

    async fn fetch_project_items_page(
        &self,
        project_node_id: &str,
        column_name: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<Issue>, PageInfo)> {
        let query = r#"
            query($projectId: ID!, $cursor: String, $statusField: String!) {
                node(id: $projectId) {
                    ... on ProjectV2 {
                        items(first: 100, after: $cursor) {
                            pageInfo {
                                hasNextPage
                                endCursor
                            }
                            nodes {
                                id
                                fieldValueByName(name: $statusField) {
                                    ... on ProjectV2ItemFieldSingleSelectValue {
                                        __typename
                                        name
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

        let variables = json!({
            "projectId": project_node_id,
            "cursor": cursor,
            "statusField": "Status"
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

        for item in project.items.nodes {
            // Check if item is in the specified column
            let item_column = item
                .field_value_by_name
                .as_ref()
                .and_then(|fv| fv.name());

            if item_column != Some(column_name) {
                continue;
            }

            // Extract issue content
            if let Some(ItemContent::Issue(content)) = item.content {
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
        }

        Ok((issues, project.items.page_info))
    }
}
