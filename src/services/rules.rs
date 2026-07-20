use crate::core::models::{CreateJob, PostAction};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleMatcher {
    pub domains: Vec<String>,
    pub extensions: Vec<String>,
    pub mime_types: Vec<String>,
    pub url_regex: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleActions {
    pub destination: Option<PathBuf>,
    pub tags: Vec<String>,
    pub speed_limit_bps: Option<u64>,
    pub post_actions: Vec<PostAction>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: uuid::Uuid,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub matcher: RuleMatcher,
    pub actions: RuleActions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreviewMatch {
    pub id: uuid::Uuid,
    pub name: String,
    pub priority: i32,
    pub destination_selected: bool,
    pub destination_shadowed: bool,
    pub speed_limit_selected: bool,
    pub speed_limit_shadowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreview {
    pub result: CreateJob,
    pub matches: Vec<RulePreviewMatch>,
}

impl Rule {
    pub fn matches(&self, url: &str, mime: Option<&str>, extension: Option<&str>) -> bool {
        if !self.enabled {
            return false;
        }
        let parsed = Url::parse(url).ok();
        let host = parsed
            .as_ref()
            .and_then(|u| u.host_str())
            .unwrap_or_default();
        let domains = self.matcher.domains.is_empty()
            || self.matcher.domains.iter().any(|d| domain_matches(d, host));
        let extensions = self.matcher.extensions.is_empty()
            || extension.is_some_and(|e| {
                self.matcher
                    .extensions
                    .iter()
                    .any(|x| x.eq_ignore_ascii_case(e))
            });
        let mimes = self.matcher.mime_types.is_empty()
            || mime.is_some_and(|m| self.matcher.mime_types.iter().any(|p| mime_matches(p, m)));
        let regex = self
            .matcher
            .url_regex
            .as_ref()
            .is_none_or(|p| Regex::new(p).is_ok_and(|r| r.is_match(url)));
        domains && extensions && mimes && regex
    }
    pub fn apply(&self, request: &mut CreateJob) {
        if let Some(destination) = &self.actions.destination {
            request.destination = Some(destination.clone());
        }
        request.options.tags.extend(self.actions.tags.clone());
        request.options.tags.sort();
        request.options.tags.dedup();
        if self.actions.speed_limit_bps.is_some() {
            request.speed_limit_bps = self.actions.speed_limit_bps;
        }
        for action in &self.actions.post_actions {
            if !request.options.post_actions.contains(action) {
                request.options.post_actions.push(action.clone());
            }
        }
    }
}
fn domain_matches(pattern: &str, host: &str) -> bool {
    pattern.strip_prefix("*.").map_or_else(
        || pattern.eq_ignore_ascii_case(host),
        |suffix| {
            host.eq_ignore_ascii_case(suffix)
                || host
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{}", suffix.to_ascii_lowercase()))
        },
    )
}
fn mime_matches(pattern: &str, actual: &str) -> bool {
    pattern.strip_suffix("/*").map_or_else(
        || pattern.eq_ignore_ascii_case(actual),
        |prefix| {
            actual
                .to_ascii_lowercase()
                .starts_with(&format!("{}/", prefix.to_ascii_lowercase()))
        },
    )
}

/// Applies matching rules in priority order. Scalar actions are first-wins; additive actions are merged.
pub fn apply_matching(
    rules: &[Rule],
    request: &mut CreateJob,
    mime: Option<&str>,
    extension: Option<&str>,
) {
    let mut destination_set = request.destination.is_some();
    let mut speed_set = request.speed_limit_bps.is_some();
    for rule in rules {
        if !rule.matches(&request.source, mime, extension) {
            continue;
        }
        if !destination_set {
            if let Some(destination) = &rule.actions.destination {
                request.destination = Some(destination.clone());
                destination_set = true;
            }
        }
        if !speed_set {
            if let Some(limit) = rule.actions.speed_limit_bps {
                request.speed_limit_bps = Some(limit);
                speed_set = true;
            }
        }
        request
            .options
            .tags
            .extend(rule.actions.tags.iter().cloned());
        for action in &rule.actions.post_actions {
            if !request.options.post_actions.contains(action) {
                request.options.post_actions.push(action.clone());
            }
        }
    }
    request.options.tags.sort();
    request.options.tags.dedup();
}

/// Explains rule selection while producing the same result as `apply_matching`.
pub fn preview_matching(
    rules: &[Rule],
    request: &CreateJob,
    mime: Option<&str>,
    extension: Option<&str>,
) -> RulePreview {
    let mut result = request.clone();
    let mut destination_set = result.destination.is_some();
    let mut speed_set = result.speed_limit_bps.is_some();
    let mut matches = Vec::new();
    for rule in rules {
        if !rule.matches(&result.source, mime, extension) {
            continue;
        }
        let destination_selected = !destination_set && rule.actions.destination.is_some();
        let destination_shadowed = destination_set && rule.actions.destination.is_some();
        let speed_limit_selected = !speed_set && rule.actions.speed_limit_bps.is_some();
        let speed_limit_shadowed = speed_set && rule.actions.speed_limit_bps.is_some();
        matches.push(RulePreviewMatch {
            id: rule.id,
            name: rule.name.clone(),
            priority: rule.priority,
            destination_selected,
            destination_shadowed,
            speed_limit_selected,
            speed_limit_shadowed,
        });
        if destination_selected {
            result.destination = rule.actions.destination.clone();
            destination_set = true;
        }
        if speed_limit_selected {
            result.speed_limit_bps = rule.actions.speed_limit_bps;
            speed_set = true;
        }
        result
            .options
            .tags
            .extend(rule.actions.tags.iter().cloned());
        for action in &rule.actions.post_actions {
            if !result.options.post_actions.contains(action) {
                result.options.post_actions.push(action.clone());
            }
        }
    }
    result.options.tags.sort();
    result.options.tags.dedup();
    RulePreview { result, matches }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(matcher: RuleMatcher) -> Rule {
        Rule {
            id: uuid::Uuid::nil(),
            name: "test".into(),
            enabled: true,
            priority: 0,
            matcher,
            actions: RuleActions::default(),
        }
    }

    #[test]
    fn wildcard_domain_matches_subdomains_and_root() {
        let rule = rule(RuleMatcher {
            domains: vec!["*.example.com".into()],
            ..Default::default()
        });
        assert!(rule.matches("https://example.com/file.zip", None, Some("zip")));
        assert!(rule.matches("https://cdn.example.com/file.zip", None, Some("zip")));
        assert!(!rule.matches("https://example.org/file.zip", None, Some("zip")));
    }

    #[test]
    fn wildcard_mime_matches_category() {
        let rule = rule(RuleMatcher {
            mime_types: vec!["video/*".into()],
            ..Default::default()
        });
        assert!(rule.matches("https://example.com/video", Some("video/mp4"), None));
        assert!(!rule.matches("https://example.com/image", Some("image/png"), None));
    }

    #[test]
    fn higher_priority_scalar_action_wins() {
        let mut request = CreateJob {
            preset_id: None,
            kind: crate::core::models::JobKind::Http,
            source: "https://example.com/file.bin".into(),
            destination: None,
            filename: None,
            priority: 0,
            speed_limit_bps: None,
            expected_sha256: None,
            duplicate_policy: Default::default(),
            options: Default::default(),
        };
        let rules = vec![
            Rule {
                priority: 100,
                actions: RuleActions {
                    destination: Some("high".into()),
                    ..Default::default()
                },
                ..rule(Default::default())
            },
            Rule {
                priority: 1,
                actions: RuleActions {
                    destination: Some("low".into()),
                    ..Default::default()
                },
                ..rule(Default::default())
            },
        ];
        apply_matching(&rules, &mut request, None, Some("bin"));
        assert_eq!(request.destination, Some(PathBuf::from("high")));
    }

    #[test]
    fn preview_matches_application_and_explains_shadowed_scalars() {
        let mut high = rule(Default::default());
        high.name = "high".into();
        high.priority = 100;
        high.actions.destination = Some("high".into());
        high.actions.speed_limit_bps = Some(10);
        let mut low = rule(Default::default());
        low.name = "low".into();
        low.priority = 1;
        low.actions.destination = Some("low".into());
        low.actions.speed_limit_bps = Some(20);
        let rules = vec![high, low];
        let request = base_request();
        let preview = preview_matching(&rules, &request, None, Some("bin"));
        let mut applied = request;
        apply_matching(&rules, &mut applied, None, Some("bin"));

        assert_eq!(preview.result.destination, applied.destination);
        assert_eq!(preview.result.speed_limit_bps, applied.speed_limit_bps);
        assert!(preview.matches[0].destination_selected);
        assert!(preview.matches[0].speed_limit_selected);
        assert!(preview.matches[1].destination_shadowed);
        assert!(preview.matches[1].speed_limit_shadowed);
    }

    fn base_request() -> CreateJob {
        CreateJob {
            preset_id: None,
            kind: crate::core::models::JobKind::Http,
            source: "https://example.com/file.bin".into(),
            destination: None,
            filename: None,
            priority: 0,
            speed_limit_bps: None,
            expected_sha256: None,
            duplicate_policy: Default::default(),
            options: Default::default(),
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(500))]

        /// Rule evaluation is deterministic and the scalar destination always
        /// comes from the first enabled matching rule in priority order.
        #[test]
        fn priority_order_is_deterministic_and_first_wins(
            priorities in proptest::collection::vec(0_i32..1_000, 1..12),
            has_destination in proptest::collection::vec(proptest::bool::ANY, 12),
            enabled in proptest::collection::vec(proptest::bool::ANY, 12),
        ) {
            use proptest::prelude::prop_assert_eq;
            let mut rules: Vec<Rule> = priorities
                .iter()
                .enumerate()
                .map(|(index, priority)| Rule {
                    id: uuid::Uuid::nil(),
                    name: format!("rule-{index}"),
                    enabled: enabled[index],
                    priority: *priority,
                    matcher: RuleMatcher::default(),
                    actions: RuleActions {
                        destination: has_destination[index]
                            .then(|| PathBuf::from(format!("dir-{index}"))),
                        tags: vec![format!("tag-{index}")],
                        speed_limit_bps: None,
                        post_actions: Vec::new(),
                    },
                })
                .collect();
            rules.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.name.cmp(&b.name)));

            let mut first = base_request();
            apply_matching(&rules, &mut first, None, None);
            let mut second = base_request();
            apply_matching(&rules, &mut second, None, None);
            prop_assert_eq!(&first.destination, &second.destination);
            prop_assert_eq!(&first.options.tags, &second.options.tags);

            let expected = rules
                .iter()
                .find(|rule| rule.enabled && rule.actions.destination.is_some())
                .and_then(|rule| rule.actions.destination.clone());
            prop_assert_eq!(first.destination, expected);
        }
    }
}
