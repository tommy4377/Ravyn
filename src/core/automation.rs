//! Rules, browser tokens, schedules, and validation of automation input.

use sha2::Digest;
use uuid::Uuid;

use crate::{
    core::models::{JobKind, PostAction},
    error::{RavynError, Result},
    services::{
        browser::{self, BrowserTokenRecord, CreateBrowserToken, IssuedBrowserToken},
        rules::Rule,
        schedules::{ScheduleInput, ScheduleMode},
        security,
    },
    storage::RuleInput,
};

use crate::core::manager::{JobManager, preset_extension, validate_tags};

impl JobManager {
    pub async fn create_rule(&self, input: RuleInput) -> Result<Rule> {
        self.validate_rule_input(&input)?;
        self.repository.create_rule(input).await
    }

    pub async fn update_rule(&self, id: Uuid, input: RuleInput) -> Result<Rule> {
        self.validate_rule_input(&input)?;
        self.repository.update_rule(id, input).await
    }

    pub async fn issue_browser_token(
        &self,
        request: CreateBrowserToken,
    ) -> Result<IssuedBrowserToken> {
        let (issued, token_hash) = browser::issue(request)?;
        self.repository
            .insert_browser_token(&issued.record, &token_hash)
            .await?;
        Ok(issued)
    }

    pub async fn list_browser_tokens(&self) -> Result<Vec<BrowserTokenRecord>> {
        self.repository.list_browser_tokens().await
    }

    pub async fn revoke_browser_token(&self, id: Uuid) -> Result<()> {
        self.repository.revoke_browser_token(id).await
    }

    pub async fn create_schedule(&self, input: ScheduleInput) -> Result<crate::storage::Schedule> {
        self.validate_schedule_input(&input).await?;
        self.repository.create_schedule(input).await
    }

    pub async fn update_schedule(
        &self,
        id: Uuid,
        input: ScheduleInput,
    ) -> Result<crate::storage::Schedule> {
        self.validate_schedule_input(&input).await?;
        self.repository.update_schedule(id, input).await
    }

    pub async fn execute_schedule(&self, schedule: &crate::storage::Schedule) -> Result<()> {
        let started = std::time::Instant::now();
        let delay = chrono::Utc::now()
            .signed_duration_since(schedule.next_run_at)
            .to_std()
            .unwrap_or_default();
        let mode = match schedule.mode {
            ScheduleMode::Download => "download",
            ScheduleMode::SniffResources => "sniff_resources",
        };
        let result = async {
            match schedule.mode {
                ScheduleMode::Download => {
                    self.create(schedule.to_create_job()).await?;
                    Ok(())
                }
                ScheduleMode::SniffResources => {
                    let automation = schedule.automation.as_ref().ok_or_else(|| {
                        RavynError::Internal("sniff schedule omitted automation options".into())
                    })?;
                    let sniff = self
                        .sniff_page(&automation.request(schedule.source.clone()))
                        .await?;
                    let mut defaults = automation.import_defaults.clone();
                    if defaults.destination.is_none() {
                        defaults.destination = Some(schedule.destination.clone());
                    }
                    let sources = sniff
                        .resources
                        .iter()
                        .map(|resource| resource.url.clone())
                        .collect::<Vec<_>>();
                    let result = self.import_urls(sources, defaults, 0).await?;
                    let remembered = sniff
                        .resources
                        .iter()
                        .filter(|resource| {
                            result
                                .items
                                .iter()
                                .any(|item| item.source == resource.url && item.job.is_some())
                        })
                        .map(|resource| {
                            (
                                resource.url.clone(),
                                resource.kind.as_str().to_owned(),
                                true,
                            )
                        })
                        .collect::<Vec<_>>();
                    self.repository
                        .remember_page_resources(&sniff.page_url, &remembered)
                        .await?;
                    if result.accepted == 0 && result.rejected > 0 {
                        return Err(RavynError::Process(format!(
                            "all {} discovered resources were rejected",
                            result.rejected
                        )));
                    }
                    Ok(())
                }
            }
        }
        .await;
        self.metrics
            .schedule_finished(mode, result.is_ok(), delay, started.elapsed());
        result
    }

    pub async fn run_schedule_now(
        &self,
        schedule_id: Uuid,
        idempotency_key: Option<&str>,
    ) -> Result<crate::storage::ScheduleExecutionRecord> {
        let mut schedule = self.repository.get_schedule(schedule_id).await?;
        schedule.next_run_at = chrono::Utc::now();
        let request_hash = hex::encode(sha2::Sha256::digest(schedule_id.as_bytes()));
        let _guard = self.idempotency.lock().await;
        if let Some(key) = idempotency_key {
            let key = key.trim();
            if key.is_empty() || key.len() > 200 {
                return Err(RavynError::Invalid(
                    "Idempotency-Key must contain between 1 and 200 characters".into(),
                ));
            }
            if let Some((stored_hash, resource_id)) = self
                .repository
                .get_idempotent_resource("schedule_run_now", key)
                .await?
            {
                if stored_hash != request_hash {
                    return Err(RavynError::Conflict(
                        "Idempotency-Key was already used for a different request".into(),
                    ));
                }
                let id = Uuid::parse_str(&resource_id).map_err(|error| {
                    RavynError::Internal(format!(
                        "stored schedule execution id is invalid: {error}"
                    ))
                })?;
                return self.repository.get_schedule_execution(id).await;
            }
        }
        let claim = crate::storage::ScheduleClaim {
            schedule: schedule.clone(),
            token: format!("run-now-{}", Uuid::new_v4()),
        };
        let execution_id = self
            .repository
            .begin_schedule_execution(&claim)
            .await?
            .ok_or_else(|| {
                RavynError::Conflict("schedule is already running for this instant".into())
            })?;
        if let Some(key) = idempotency_key {
            self.repository
                .put_idempotent_resource(
                    "schedule_run_now",
                    key.trim(),
                    &request_hash,
                    execution_id,
                )
                .await?;
        }
        match self.execute_schedule(&schedule).await {
            Ok(()) => {
                self.repository
                    .finish_schedule_execution(execution_id, "completed", None)
                    .await?
            }
            Err(error) => {
                let message = error.to_string();
                self.repository
                    .finish_schedule_execution(execution_id, "failed", Some(&message))
                    .await?;
            }
        }
        self.repository.get_schedule_execution(execution_id).await
    }

    async fn validate_schedule_input(&self, input: &ScheduleInput) -> Result<()> {
        if input.mode == ScheduleMode::SniffResources
            || matches!(input.kind, JobKind::Http | JobKind::Media)
        {
            security::validate_network_source_resolved(&self.config, &input.source).await?;
        }
        security::validate_output_path(&self.config, &input.destination)?;
        validate_tags(&input.options.tags)?;
        self.validate_post_actions(&input.options.post_actions)?;
        self.validate_download_secret_references(&input.options)
            .await?;
        if let Some(automation) = input.automation.as_ref() {
            if automation
                .max_resources
                .is_some_and(|value| value == 0 || value > self.config.max_sniff_resources)
            {
                return Err(RavynError::Invalid(format!(
                    "schedule max_resources must be between 1 and {}",
                    self.config.max_sniff_resources
                )));
            }
            if automation.extensions.len() > 128
                || automation
                    .extensions
                    .iter()
                    .any(|value| value.trim().is_empty() || value.len() > 32)
            {
                return Err(RavynError::Invalid(
                    "schedule extension filters must contain at most 128 non-empty values of 32 characters".into(),
                ));
            }
            validate_tags(&automation.import_defaults.options.tags)?;
            self.validate_post_actions(&automation.import_defaults.options.post_actions)?;
            self.validate_download_secret_references(&automation.import_defaults.options)
                .await?;
            if let Some(destination) = automation.import_defaults.destination.as_ref() {
                security::validate_output_path(&self.config, destination)?;
            }
        }
        input.validate(chrono::Utc::now())?;
        Ok(())
    }
    fn validate_rule_input(&self, input: &RuleInput) -> Result<()> {
        input.validate()?;
        validate_tags(&input.actions.tags)?;
        self.validate_post_actions(&input.actions.post_actions)?;
        if let Some(destination) = input.actions.destination.as_ref() {
            security::validate_output_path(&self.config, destination)?;
        }
        Ok(())
    }

    pub(crate) fn validate_post_actions(&self, actions: &[PostAction]) -> Result<()> {
        if actions.len() > 32 {
            return Err(RavynError::Invalid(
                "a job may contain at most 32 post-processing actions".into(),
            ));
        }
        for action in actions {
            match action {
                PostAction::Extract {
                    destination: Some(destination),
                    ..
                }
                | PostAction::Move { destination } => {
                    security::validate_output_path(&self.config, destination)?;
                }
                PostAction::ConvertMedia {
                    extension,
                    preset,
                    arguments,
                    unsafe_arguments,
                    ..
                } => {
                    let normalized = extension.trim_start_matches('.');
                    if normalized.is_empty()
                        || extension.len() > 16
                        || extension.contains('/')
                        || extension.contains('\\')
                        || extension.contains("..")
                    {
                        return Err(RavynError::Invalid(
                            "conversion extensions must contain 1 to 16 path-safe characters"
                                .into(),
                        ));
                    }
                    if arguments.len() > 128
                        || arguments.iter().any(|argument| argument.len() > 4_096)
                    {
                        return Err(RavynError::Invalid(
                            "conversion arguments exceed the configured safety limits".into(),
                        ));
                    }
                    if preset.is_some() && (!arguments.is_empty() || *unsafe_arguments) {
                        return Err(RavynError::Invalid(
                            "named FFmpeg presets may not include arbitrary arguments".into(),
                        ));
                    }
                    if let Some(expected) = preset.and_then(preset_extension)
                        && !normalized.eq_ignore_ascii_case(expected)
                    {
                        return Err(RavynError::Invalid(format!(
                            "the selected FFmpeg preset requires the .{expected} extension"
                        )));
                    }
                    if preset.is_none() && (!*unsafe_arguments || !self.config.allow_unsafe_ffmpeg)
                    {
                        return Err(RavynError::Invalid(
                            "conversion requires a named preset; arbitrary arguments require both unsafe_arguments=true and --allow-unsafe-ffmpeg".into(),
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) async fn validate_download_secret_references(
        &self,
        options: &crate::core::models::DownloadOptions,
    ) -> Result<()> {
        for (id, expected_type) in [
            (options.proxy_secret_id, "proxy_credentials"),
            (options.cookies_secret_id, "cookies"),
            (
                options.authentication_header_secret_id,
                "authentication_header",
            ),
        ] {
            let Some(id) = id else {
                continue;
            };
            let reference = self.repository.get_secret_reference(id).await?;
            if reference.secret_type != expected_type {
                return Err(RavynError::Invalid(format!(
                    "secret reference {id} has type {}, expected {expected_type}",
                    reference.secret_type
                )));
            }
        }
        Ok(())
    }
}
