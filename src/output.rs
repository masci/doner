use std::collections::HashMap;

use crate::models::Issue;
use crate::OutputFormat;

/// Format issues as a simple list
pub fn format_list(issues: &[Issue], format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => format_list_text(issues),
        OutputFormat::Markdown => format_list_markdown(issues),
    }
}

/// Format issues grouped by parent
pub fn format_grouped(issues: &[Issue], format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => format_grouped_text(issues),
        OutputFormat::Markdown => format_grouped_markdown(issues),
    }
}

fn format_list_text(issues: &[Issue]) -> String {
    let mut output = String::new();

    output.push_str(&format!("Found {} issue(s):\n\n", issues.len()));

    for issue in issues {
        output.push_str(&format!(
            "• [{}#{}] {}\n",
            issue.repository, issue.number, issue.title
        ));
        output.push_str(&format!("  {}\n", issue.url));

        if let Some(parent) = &issue.parent {
            output.push_str(&format!("  Parent: {} ({})\n", parent.title, parent.url));
        }

        if let Some(closed_at) = issue.closed_at {
            output.push_str(&format!("  Closed: {}\n", closed_at.format("%Y-%m-%d %H:%M")));
        }

        output.push('\n');
    }

    output.trim_end().to_string()
}

fn format_list_markdown(issues: &[Issue]) -> String {
    let mut output = String::new();

    output.push_str(&format!("## Summary ({} issues)\n\n", issues.len()));

    for issue in issues {
        output.push_str(&format!(
            "- **[{}#{}]({})**: {}\n",
            issue.repository, issue.number, issue.url, issue.title
        ));

        if let Some(parent) = &issue.parent {
            output.push_str(&format!("  - Parent: [{}]({})\n", parent.title, parent.url));
        }

        if let Some(closed_at) = issue.closed_at {
            output.push_str(&format!(
                "  - Closed: {}\n",
                closed_at.format("%Y-%m-%d %H:%M")
            ));
        }
    }

    output.trim_end().to_string()
}

fn format_grouped_text(issues: &[Issue]) -> String {
    let grouped = group_by_parent(issues);
    let mut output = String::new();

    output.push_str(&format!("Found {} issue(s):\n\n", issues.len()));

    // First, output issues with parents
    for (parent_title, (parent_info, children)) in grouped.with_parent.iter() {
        output.push_str(&format!("▶ {}\n", parent_title));
        if let Some(info) = parent_info {
            output.push_str(&format!("  {}\n", info.url));
        }
        output.push_str("  Completed:\n");

        for issue in children {
            output.push_str(&format!(
                "    • [{}#{}] {}\n",
                issue.repository, issue.number, issue.title
            ));
        }
        output.push('\n');
    }

    // Then, output orphan issues (no parent)
    if !grouped.orphans.is_empty() {
        output.push_str("▶ Standalone Issues\n");
        for issue in &grouped.orphans {
            output.push_str(&format!(
                "  • [{}#{}] {}\n",
                issue.repository, issue.number, issue.title
            ));
            output.push_str(&format!("    {}\n", issue.url));
        }
    }

    output.trim_end().to_string()
}

fn format_grouped_markdown(issues: &[Issue]) -> String {
    let grouped = group_by_parent(issues);
    let mut output = String::new();

    output.push_str(&format!("## Summary ({} issues)\n\n", issues.len()));

    // First, output issues with parents
    for (parent_title, (parent_info, children)) in grouped.with_parent.iter() {
        if let Some(info) = parent_info {
            output.push_str(&format!("### [{}]({})\n\n", parent_title, info.url));
        } else {
            output.push_str(&format!("### {}\n\n", parent_title));
        }

        for issue in children {
            output.push_str(&format!(
                "- [{}#{}]({}): {}\n",
                issue.repository, issue.number, issue.url, issue.title
            ));
        }
        output.push('\n');
    }

    // Then, output orphan issues (no parent)
    if !grouped.orphans.is_empty() {
        output.push_str("### Standalone Issues\n\n");
        for issue in &grouped.orphans {
            output.push_str(&format!(
                "- [{}#{}]({}): {}\n",
                issue.repository, issue.number, issue.url, issue.title
            ));
        }
    }

    output.trim_end().to_string()
}

struct GroupedIssues<'a> {
    with_parent: HashMap<String, (Option<ParentInfo>, Vec<&'a Issue>)>,
    orphans: Vec<&'a Issue>,
}

struct ParentInfo {
    url: String,
}

fn group_by_parent(issues: &[Issue]) -> GroupedIssues<'_> {
    let mut with_parent: HashMap<String, (Option<ParentInfo>, Vec<&Issue>)> = HashMap::new();
    let mut orphans = Vec::new();

    for issue in issues {
        if let Some(parent) = &issue.parent {
            let entry = with_parent
                .entry(parent.title.clone())
                .or_insert_with(|| {
                    (
                        Some(ParentInfo {
                            url: parent.url.clone(),
                        }),
                        Vec::new(),
                    )
                });
            entry.1.push(issue);
        } else {
            orphans.push(issue);
        }
    }

    GroupedIssues {
        with_parent,
        orphans,
    }
}
