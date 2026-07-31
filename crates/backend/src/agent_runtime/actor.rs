use super::routing::{SessionControl, SessionEvent};
use super::scheduling::{ActiveInput, ActiveInputState};
use super::*;
use ora_acp::AcpClient;
use ora_contracts::SessionPermissionRequest;
use ora_contracts::acp::common::SessionId as AcpSessionId;
use ora_contracts::acp::literals::AGENT_METHOD_NAMES;
use ora_contracts::acp::notification::CancelNotification;
use ora_contracts::acp::permission::{RequestPermissionOutcome, RequestPermissionResponse};
use ora_contracts::acp::prompt::{PromptRequest, PromptResponse};
use ora_contracts::acp::session::{
    CloseSessionRequest, CloseSessionResponse, LoadSessionRequest as AcpLoadSessionRequest,
    LoadSessionResponse,
};
use ora_logging::ora_debug;
use tokio::process::ChildStdin;
use tokio::time::{Instant, timeout};

impl RuntimeActor {
    /// Serializes operations for one logical session while the shared connection remains concurrent.
    pub(super) async fn run(mut self) {
        loop {
            let command = match self.channel.as_mut() {
                Some(channel) => {
                    // Drop already-queued residuals before waiting so they cannot attach to the
                    // next turn. Prefer controls and commands afterward so an idle update flood
                    // cannot starve Stop/Prompt, and QueueOverflow is not lost to a closed events
                    // channel.
                    Self::drain_idle_events(&channel.connection.client, &mut channel.events).await;
                    if let Ok(control) = channel.controls.try_recv() {
                        self.handle_idle_control(Some(control)).await;
                        continue;
                    }
                    tokio::select! {
                        biased;
                        control = channel.controls.recv() => {
                            self.handle_idle_control(control).await;
                            continue;
                        }
                        command = self.commands.recv() => {
                            Self::drain_idle_events(
                                &channel.connection.client,
                                &mut channel.events,
                            )
                            .await;
                            command
                        }
                        event = channel.events.recv() => {
                            let Some(event) = event else {
                                self.mark_stopped();
                                continue;
                            };
                            Self::settle_idle_event(&channel.connection.client, event).await;
                            continue;
                        }
                    }
                }
                None => self.commands.recv().await,
            };
            let Some(command) = command else {
                self.unload().await;
                return;
            };
            match command {
                RuntimeCommand::Load {
                    operation_id,
                    events,
                    accepted,
                } => {
                    let _ = accepted.send(Ok(()));
                    self.run_load(operation_id, events).await;
                }
                RuntimeCommand::Prompt {
                    operation_id,
                    text,
                    events,
                    accepted,
                } => {
                    if self.channel.is_none() {
                        let _ = accepted.send(Err(session_stopped()));
                    } else {
                        let _ = accepted.send(Ok(()));
                        self.run_prompt(operation_id, text, events).await;
                    }
                }
                RuntimeCommand::RespondToPermission { response, .. } => {
                    let _ = response.send(Err(permission_not_pending()));
                }
                RuntimeCommand::Stop { response } => {
                    self.unload().await;
                    let _ = response.send(Ok(StopSessionResponse {
                        session: contract_session(self.session.clone()),
                    }));
                }
                RuntimeCommand::Cancel { .. } => {}
            }
        }
    }

    /// Re-registers a stopped session and streams provider history without replacing the process.
    async fn run_load(
        &mut self,
        operation_id: u64,
        events: mpsc::Sender<Result<LoadSessionEvent, BackendError>>,
    ) {
        self.unload().await;
        let running = self
            .session
            .clone()
            .with_status(SessionStatus::Running, self.clock.now_timestamp_millis());
        if self.repository.update_session(running.clone()).is_err() {
            let _ = events.try_send(Err(session_not_found(self.session.id.as_ref())));
            return;
        }
        self.session = running;
        let channel = match self
            .connection
            .open_session_channel(&self.session.agent_session_id)
        {
            Ok(channel) => channel,
            Err(error) => {
                let _ = events.try_send(Err(error));
                self.mark_stopped();
                return;
            }
        };
        if !channel.connection.load_session_supported {
            let _ = events.try_send(Err(BackendError::new(
                BackendErrorKind::Conflict,
                "session_load_unsupported",
                "agent CLI does not support session/load",
            )));
            self.mark_stopped();
            return;
        }
        self.run_load_on_channel(operation_id, events, channel)
            .await;
    }

    /// Selects over load replay, routed updates, cancellation, and connection failure.
    async fn run_load_on_channel(
        &mut self,
        operation_id: u64,
        events: mpsc::Sender<Result<LoadSessionEvent, BackendError>>,
        mut channel: SessionChannel,
    ) {
        let client = channel.connection.client.clone();
        let request = AcpLoadSessionRequest::new(
            AcpSessionId::new(self.session.agent_session_id.clone()),
            &self.cwd,
        );
        ora_debug!(session_id = %self.session.id, "session/load sent");
        let pending = match client
            .start_session_request::<_, LoadSessionResponse>(
                AcpSessionId::new(self.session.agent_session_id.clone()),
                AGENT_METHOD_NAMES.session_load,
                &request,
            )
            .await
        {
            Ok(pending) => pending,
            Err(error) => {
                let _ = events.try_send(Err(map_acp_error(error)));
                self.isolate_channel(channel).await;
                return;
            }
        };
        let deadline = tokio::time::sleep(SESSION_SETUP_TIMEOUT);
        tokio::pin!(deadline);
        let mut input_state = ActiveInputState::default();
        loop {
            let input = tokio::select! {
                // The input scheduler already bounds live event bursts. Prefer an accepted
                // terminal response over a simultaneously-ready setup deadline.
                biased;
                input = input_state.recv(
                    &mut channel.events,
                    &mut channel.controls,
                    &mut self.commands,
                ) => input,
                _ = &mut deadline => {
                    ora_debug!(session_id = %self.session.id, "session/load timed out");
                    self.cancel(&client, &HashMap::new()).await;
                    let _ = events.try_send(Err(runtime_internal(
                        "agent_load_timeout",
                        "agent CLI session load timed out",
                    )));
                    self.isolate_channel(channel).await;
                    return;
                }
            };
            match input {
                ActiveInput::Event(SessionEvent::Update(update)) => {
                    deadline
                        .as_mut()
                        .reset(Instant::now() + SESSION_SETUP_TIMEOUT);
                    if events
                        .try_send(Ok(LoadSessionEvent::SessionUpdate {
                            update: update.update,
                        }))
                        .is_err()
                    {
                        self.cancel(&client, &HashMap::new()).await;
                        self.isolate_channel(channel).await;
                        return;
                    }
                }
                ActiveInput::Event(SessionEvent::Permission(permission)) => {
                    let _ = client
                        .respond(
                            &permission.request_id,
                            &RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled),
                        )
                        .await;
                    let _ = events.try_send(Err(runtime_internal(
                        "agent_protocol_error",
                        "permission request during session/load is unsupported",
                    )));
                    self.isolate_channel(channel).await;
                    return;
                }
                ActiveInput::Event(SessionEvent::Response(response)) => {
                    if !pending.matches_response(&response) {
                        continue;
                    }
                    match pending.finish(response) {
                        Ok(_) => {
                            ora_debug!(session_id = %self.session.id, "session/load completed");
                            if events.try_send(Ok(LoadSessionEvent::Completed)).is_ok() {
                                self.channel = Some(channel);
                            } else {
                                self.isolate_channel(channel).await;
                            }
                        }
                        Err(error) => {
                            ora_debug!(session_id = %self.session.id, error = %error, "session/load failed");
                            let _ = events.try_send(Err(map_acp_error(error)));
                            self.isolate_channel(channel).await;
                        }
                    }
                    return;
                }
                ActiveInput::Control(SessionControl::ConnectionLost(error)) => {
                    self.fail_load(&events, error);
                    return;
                }
                ActiveInput::Control(SessionControl::QueueOverflow) => {
                    let _ = events.try_send(Err(runtime_internal(
                        "agent_event_overflow",
                        "session event queue overflowed",
                    )));
                    self.isolate_channel(channel).await;
                    return;
                }
                ActiveInput::EventsClosed | ActiveInput::ControlsClosed => {
                    self.fail_load(&events, runtime_unavailable());
                    return;
                }
                ActiveInput::Command(RuntimeCommand::Cancel {
                    operation_id: cancelled,
                }) if cancelled == operation_id => {
                    self.cancel(&client, &HashMap::new()).await;
                    let _ = timeout(
                        CANCELLATION_GRACE,
                        settle_abandoned_session_response(&mut channel, &client, pending),
                    )
                    .await;
                    self.isolate_channel(channel).await;
                    return;
                }
                ActiveInput::Command(RuntimeCommand::Stop { response }) => {
                    self.cancel(&client, &HashMap::new()).await;
                    self.isolate_channel(channel).await;
                    let _ = response.send(Ok(StopSessionResponse {
                        session: contract_session(self.session.clone()),
                    }));
                    return;
                }
                ActiveInput::Command(
                    RuntimeCommand::Prompt { accepted, .. } | RuntimeCommand::Load { accepted, .. },
                ) => {
                    let _ = accepted.send(Err(session_busy()));
                }
                ActiveInput::Command(RuntimeCommand::RespondToPermission { response, .. }) => {
                    let _ = response.send(Err(permission_not_pending()));
                }
                ActiveInput::Command(RuntimeCommand::Cancel { .. }) => {}
                ActiveInput::CommandsClosed => {
                    self.cancel(&client, &HashMap::new()).await;
                    self.isolate_channel(channel).await;
                    return;
                }
            }
        }
    }

    /// Streams one prompt while routing only events that belong to this provider session.
    async fn run_prompt(
        &mut self,
        operation_id: u64,
        text: String,
        events: mpsc::Sender<Result<PromptSessionEvent, BackendError>>,
    ) {
        let Some(mut channel) = self.channel.take() else {
            return;
        };
        let client = channel.connection.client.clone();
        // Catch residuals that arrived after idle accepted this prompt and before we took the
        // channel, so they cannot be attributed to the new turn.
        Self::drain_idle_events(&client, &mut channel.events).await;
        if let Ok(control) = channel.controls.try_recv() {
            match control {
                SessionControl::QueueOverflow => {
                    let _ = events.try_send(Err(runtime_internal(
                        "agent_event_overflow",
                        "session event queue overflowed",
                    )));
                    self.isolate_channel(channel).await;
                    return;
                }
                SessionControl::ConnectionLost(error) => {
                    self.fail_prompt(&events, error);
                    return;
                }
            }
        }
        let text_len = text.len();
        let request = PromptRequest::new(self.session.agent_session_id.clone(), vec![text.into()]);
        ora_debug!(session_id = %self.session.id, text_len = text_len, "session/prompt sent");
        let pending = match client
            .start_session_request::<_, PromptResponse>(
                AcpSessionId::new(self.session.agent_session_id.clone()),
                AGENT_METHOD_NAMES.session_prompt,
                &request,
            )
            .await
        {
            Ok(pending) => pending,
            Err(error) => {
                let _ = events.try_send(Err(map_acp_error(error)));
                self.isolate_channel(channel).await;
                return;
            }
        };
        let mut permissions = HashMap::new();
        let mut input_state = ActiveInputState::default();
        loop {
            match input_state
                .recv(
                    &mut channel.events,
                    &mut channel.controls,
                    &mut self.commands,
                )
                .await
            {
                ActiveInput::Event(SessionEvent::Update(update)) => {
                    if events
                        .try_send(Ok(PromptSessionEvent::SessionUpdate {
                            update: update.update,
                        }))
                        .is_err()
                    {
                        self.cancel(&client, &permissions).await;
                        self.isolate_channel(channel).await;
                        return;
                    }
                }
                ActiveInput::Event(SessionEvent::Permission(permission)) => {
                    let public_id = permission.request_id.to_string();
                    let option_ids = permission
                        .request
                        .options
                        .iter()
                        .map(|option| option.option_id.to_string())
                        .collect::<Vec<_>>();
                    ora_debug!(session_id = %self.session.id, tool_call = ?permission.request.tool_call, option_count = option_ids.len(), request_id = %public_id, "permission requested");
                    permissions.insert(public_id.clone(), (permission.request_id, option_ids));
                    let event = PromptSessionEvent::PermissionRequest(SessionPermissionRequest {
                        permission_request_id: public_id,
                        tool_call: permission.request.tool_call,
                        options: permission.request.options,
                    });
                    if events.try_send(Ok(event)).is_err() {
                        self.cancel(&client, &permissions).await;
                        self.isolate_channel(channel).await;
                        return;
                    }
                }
                ActiveInput::Event(SessionEvent::Response(response)) => {
                    if !pending.matches_response(&response) {
                        continue;
                    }
                    match pending.finish(response) {
                        Ok(response) => {
                            ora_debug!(session_id = %self.session.id, stop_reason = ?response.stop_reason, "prompt completed");
                            if events
                                .try_send(Ok(PromptSessionEvent::Completed {
                                    stop_reason: response.stop_reason,
                                }))
                                .is_ok()
                            {
                                self.channel = Some(channel);
                            } else {
                                self.isolate_channel(channel).await;
                            }
                        }
                        Err(error) => {
                            let reusable = matches!(&error, ora_acp::AcpError::RequestFailed(_));
                            ora_debug!(session_id = %self.session.id, error = %error, reusable = reusable, "prompt failed");
                            let delivered = events.try_send(Err(map_acp_error(error))).is_ok();
                            if reusable && delivered {
                                self.channel = Some(channel);
                            } else {
                                self.isolate_channel(channel).await;
                            }
                        }
                    }
                    return;
                }
                ActiveInput::Control(SessionControl::ConnectionLost(error)) => {
                    self.fail_prompt(&events, error);
                    return;
                }
                ActiveInput::Control(SessionControl::QueueOverflow) => {
                    self.cancel(&client, &permissions).await;
                    let _ = events.try_send(Err(runtime_internal(
                        "agent_event_overflow",
                        "session event queue overflowed",
                    )));
                    self.isolate_channel(channel).await;
                    return;
                }
                ActiveInput::EventsClosed | ActiveInput::ControlsClosed => {
                    self.fail_prompt(&events, runtime_unavailable());
                    return;
                }
                ActiveInput::Command(RuntimeCommand::RespondToPermission { request, response }) => {
                    let result = respond_permission(&client, request, &mut permissions).await;
                    let _ = response.send(result);
                }
                ActiveInput::Command(RuntimeCommand::Cancel {
                    operation_id: cancelled,
                }) if cancelled == operation_id => {
                    self.cancel(&client, &permissions).await;
                    let settled = timeout(
                        CANCELLATION_GRACE,
                        settle_abandoned_session_response(&mut channel, &client, pending),
                    )
                    .await;
                    match settled {
                        Ok(Some(Ok(_))) | Ok(Some(Err(ora_acp::AcpError::RequestFailed(_)))) => {
                            self.channel = Some(channel);
                        }
                        Ok(Some(Err(_))) | Ok(None) | Err(_) => {
                            self.isolate_channel(channel).await;
                        }
                    }
                    return;
                }
                ActiveInput::Command(RuntimeCommand::Stop { response }) => {
                    self.cancel(&client, &permissions).await;
                    self.isolate_channel(channel).await;
                    let _ = response.send(Ok(StopSessionResponse {
                        session: contract_session(self.session.clone()),
                    }));
                    return;
                }
                ActiveInput::Command(
                    RuntimeCommand::Prompt { accepted, .. } | RuntimeCommand::Load { accepted, .. },
                ) => {
                    let _ = accepted.send(Err(session_busy()));
                }
                ActiveInput::Command(RuntimeCommand::Cancel { .. }) => {}
                ActiveInput::CommandsClosed => {
                    self.cancel(&client, &permissions).await;
                    self.isolate_channel(channel).await;
                    return;
                }
            }
        }
    }

    /// Settles unexpected idle events without allowing them to leak into a later operation.
    async fn settle_idle_event(client: &AcpClient<ChildStdin>, event: SessionEvent) {
        match event {
            SessionEvent::Permission(permission) => {
                let _ = client
                    .respond(
                        &permission.request_id,
                        &RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled),
                    )
                    .await;
            }
            SessionEvent::Update(_) | SessionEvent::Response(_) => {}
        }
    }

    /// Clears events already sitting in the session FIFO before a new turn starts.
    ///
    /// Bound the drain to the queue snapshot so a concurrent producer cannot livelock the actor
    /// before it reaches the idle select that prefers controls and commands.
    async fn drain_idle_events(
        client: &AcpClient<ChildStdin>,
        events: &mut mpsc::Receiver<SessionEvent>,
    ) {
        let queued = events.len();
        for _ in 0..queued {
            let Ok(event) = events.try_recv() else {
                break;
            };
            Self::settle_idle_event(client, event).await;
        }
    }

    /// Handles failures arriving while a registered session has no active operation.
    async fn handle_idle_control(&mut self, control: Option<SessionControl>) {
        match control {
            Some(SessionControl::QueueOverflow) => self.unload().await,
            Some(SessionControl::ConnectionLost(_)) | None => self.mark_stopped(),
        }
    }

    /// Cancels the provider turn and settles every outstanding permission request.
    async fn cancel(
        &self,
        client: &AcpClient<ChildStdin>,
        permissions: &HashMap<String, (ora_contracts::acp::rpc::RequestId, Vec<String>)>,
    ) {
        ora_debug!(session_id = %self.session.id, pending_permissions = permissions.len(), "cancelling prompt");
        for (request_id, _) in permissions.values() {
            let _ = client
                .respond(
                    request_id,
                    &RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled),
                )
                .await;
        }
        let _ = client
            .notify(
                AGENT_METHOD_NAMES.session_cancel,
                &CancelNotification::new(self.session.agent_session_id.clone()),
            )
            .await;
    }

    /// Closes only this live ACP registration and preserves provider-owned history.
    async fn unload(&mut self) {
        if let Some(channel) = self.channel.take() {
            self.isolate_channel(channel).await;
        } else {
            self.mark_stopped();
        }
    }

    /// Detaches one routed session while leaving the shared CLI process available.
    async fn isolate_channel(&mut self, channel: SessionChannel) {
        if channel.connection.close_session_supported {
            let _ = timeout(
                CANCELLATION_GRACE,
                channel
                    .connection
                    .client
                    .request::<_, CloseSessionResponse>(
                        AGENT_METHOD_NAMES.session_close,
                        &CloseSessionRequest::new(self.session.agent_session_id.clone()),
                    ),
            )
            .await;
        }
        self.mark_stopped();
    }

    /// Completes an interrupted load request with the connection-level failure.
    fn fail_load(
        &mut self,
        events: &mpsc::Sender<Result<LoadSessionEvent, BackendError>>,
        error: BackendError,
    ) {
        let _ = events.try_send(Err(error));
        self.mark_stopped();
    }

    /// Completes an interrupted prompt request with the connection-level failure.
    fn fail_prompt(
        &mut self,
        events: &mpsc::Sender<Result<PromptSessionEvent, BackendError>>,
        error: BackendError,
    ) {
        let _ = events.try_send(Err(error));
        self.mark_stopped();
    }

    /// Persists a stopped state after the provider session is detached or becomes unusable.
    fn mark_stopped(&mut self) {
        self.channel = None;
        self.session = self
            .session
            .clone()
            .with_status(SessionStatus::Stopped, self.clock.now_timestamp_millis());
        let _ = self.repository.update_session(self.session.clone());
        ora_debug!(session_id = %self.session.id, "session marked stopped");
    }
}

/// Drains queued traffic until the abandoned request's own response arrives or the route closes.
async fn settle_abandoned_session_response<Response>(
    channel: &mut SessionChannel,
    client: &AcpClient<ChildStdin>,
    pending: ora_acp::PendingSessionRequest<Response>,
) -> Option<Result<Response, ora_acp::AcpError>>
where
    Response: serde::de::DeserializeOwned,
{
    loop {
        match channel.events.recv().await {
            Some(SessionEvent::Update(_)) => {}
            Some(SessionEvent::Permission(permission)) => {
                let _ = client
                    .respond(
                        &permission.request_id,
                        &RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled),
                    )
                    .await;
            }
            Some(SessionEvent::Response(response)) => {
                if !pending.matches_response(&response) {
                    continue;
                }
                return Some(pending.finish(response));
            }
            None => return None,
        }
    }
}

/// Reports that the actor cannot accept a second operation while one is in flight.
fn session_busy() -> BackendError {
    BackendError::new(
        BackendErrorKind::Conflict,
        "session_busy",
        "session already has an active operation",
    )
}

/// Reports that the requested permission no longer belongs to an active prompt.
fn permission_not_pending() -> BackendError {
    BackendError::new(
        BackendErrorKind::Conflict,
        "permission_request_not_pending",
        "permission request is not pending",
    )
}
