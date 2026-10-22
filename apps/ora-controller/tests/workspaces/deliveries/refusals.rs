//! Exact Cloud refusal isolates one retained terminal; transport recovery replays its original fact.
use super::*;
use pretty_assertions::assert_eq;
use workspace_cloud::terminal_status;

/// Builds a status hint for the original Node-local operation without changing its outbox.
fn status(
    world: &World,
    execution: &ExecutionId,
    state: ExecutionState,
) -> NodeToControllerMessage {
    NodeToControllerMessage::ExecutionStatus(ExecutionStatusMessage {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        operation_id: world.node.deliveries.started(execution).operation_id,
        execution_id: execution.clone(),
        payload: ExecutionStatus {
            stop_unconfirmed: Vec::new(),
            node: NodeRuntimeIdentity {
                node_id: node_id(),
                incarnation_id: NodeIncarnationId::new("incarnation-1"),
            },
            state,
        },
    })
}

/// Observes one settled refusal across more than the transport's one-second I/O deadline.
async fn isolated(world: &World, execution: &ExecutionId, since: usize, attempts: usize) {
    world
        .timeline
        .until(|_| world.cloud.revision_attempts("refused").len() >= attempts)
        .await;
    tokio::time::sleep(Duration::from_millis(/*millis*/ 1_200)).await;
    let events = world.timeline.events();
    assert!(
        !events[since..].contains(&Event::SessionEnded),
        "refusal closed a live session: {events:#?}"
    );
    assert!(!events[since..].contains(&Event::Status {
        connection: "disconnected"
    }));
    assert!(!events.iter().any(|event| matches!(event, Event::DeliveryAck { execution: id, .. } if id == execution.as_str())));
    assert_eq!(world.cloud.revision_attempts("refused").len(), attempts);
    assert_eq!(record(world, "refused").result, None);
    assert_eq!(world.cloud.revision_receipt(execution), None);
    assert!(
        world
            .node
            .deliveries
            .data
            .lock()
            .unwrap()
            .outbox
            .contains_key(execution.as_str())
    );
}

/// Forms the original terminal, cuts transport before its first submission, then refuses it.
async fn refused(world: &World) -> (ExecutionId, usize) {
    workspace(world).await;
    let execution = delivering(world, "refused").await;
    world.node.deliveries.data.lock().unwrap().pause_terminal = true;
    world.node.deliveries.complete(&execution);
    let before = world.timeline.events().len();
    world.node.disconnect.store(true, Ordering::SeqCst);
    world
        .timeline
        .until(|events| events[before..].contains(&Event::SessionEnded))
        .await;
    assert!(world.cloud.revision_attempts("refused").is_empty());
    world.cloud.give_up_delivery("refused");
    world.node.deliveries.data.lock().unwrap().pause_terminal = false;
    world
        .timeline
        .until(|_| !world.cloud.revision_attempts("refused").is_empty())
        .await;
    let since = world.timeline.events().len();
    isolated(world, &execution, since, /*attempts*/ 1).await;
    (execution, since)
}

/// With pending polling omitted, only the session observer can keep an unreceipted terminal busy.
async fn unresolved(world: &World, execution: &ExecutionId) {
    world.cloud.hide_delivery_pending(execution);
    let before = world.timeline.events().len();
    let stop = world.cloud.queue(proto::OperationKind::Stop);
    settled(world, &stop, proto::OperationState::Failed).await;
    let events = world.timeline.events()[before..].to_vec();
    assert!(
        events.contains(&Event::Idle { idle: false }),
        "observer lost its refused execution: {events:#?}"
    );
    assert!(!events.contains(&Event::SessionEnded));
}

/// Same-connection duplicates and hints neither retry takeover nor acknowledge or redispatch it.
#[test]
fn refused_terminal_isolated_on_live_session() {
    ora_logging::initialize_test_clock();
    let recorder = Recorder::default();
    ora_logging::with_recorded_trace_logging(recorder.clone(), || {
        run_scenario("main", |world| async move {
            let (execution, since) = refused(&world).await;
            let registered = world.cloud.delivery_node_records();
            assert_eq!(registered.len(), 1);
            let original =
                world.node.deliveries.data.lock().unwrap().outbox[execution.as_str()].clone();
            let hints = [
                NodeToControllerMessage::RevisionResult(original.clone()),
                status(
                    &world,
                    &execution,
                    ExecutionState::Completed(ExecutionResult::Revision(Box::new(
                        original.payload.clone(),
                    ))),
                ),
                status(&world, &execution, ExecutionState::Unknown),
            ];
            world
                .node
                .deliveries
                .data
                .lock()
                .unwrap()
                .injected
                .extend(hints);
            // The second execution exercises normal takeover and ACK on this very connection.
            let other = delivering(&world, "other").await;
            world.node.deliveries.complete(&other);
            world
                .timeline
                .until(|events| {
                    events.contains(&Event::DeliveryAck {
                        execution: other.as_str().into(),
                        sequence: 1,
                    })
                })
                .await;
            assert!(world.cloud.revision_receipt(&other).is_some());
            unresolved(&world, &execution).await;
            // Cloud's independent reporter must refresh Connected while Node heartbeats keep the
            // same WebSocket alive. This is stronger than checking an old registration alone.
            let reported = world.cloud.delivery_node_records()[0].version;
            world
                .timeline
                .until(|_| world.cloud.delivery_node_records()[0].version > reported)
                .await;
            let current = world.cloud.delivery_node_records();
            assert_eq!(
                (&current[0].id, &current[0].identity, current[0].connection),
                (
                    &registered[0].id,
                    &registered[0].identity,
                    proto::NodeConnection::Connected as i32
                )
            );
            assert!(!world.timeline.events()[since..].contains(&Event::SessionEnded));
            assert_eq!(world.cloud.revision_attempts("refused").len(), 1);
            assert_eq!(world.timeline.events().iter().filter(|event| matches!(event, Event::DeliveryStarted { execution: id } if id == execution.as_str())).count(), 1);
            assert_eq!(world.cloud.queried_revisions(), 0);
            let lines = recorder.0.lock().unwrap().clone();
            let warnings: Vec<_> = lines
                .iter()
                .filter(|line| {
                    line.contains(
                        "Cloud refused delivery terminal takeover; execution remains unresolved",
                    )
                })
                .collect();
            assert_eq!(warnings.len(), 1, "duplicate warnings: {lines:#?}");
            let warning = warnings[0];
            for field in [
                "WARN",
                "grpc_code=Aborted",
                "detail=\"ERROR_CODE_CONFLICT\"",
                "cloud_message=dispatch_conflict",
                "disposition=\"unresolved/no_ack/no_redispatch\"",
                "sequence=1",
                &format!("operation_id={}", original.operation_id.as_str()),
                &format!("execution_id={}", execution.as_str()),
                &format!("node_id={}", node_id().as_str()),
                "node_incarnation_id=incarnation-1",
                &format!("workspace_id={WORKSPACE}"),
            ] {
                assert!(warning.contains(field), "missing {field}: {warning}");
            }
            assert!(!warning.contains("history.jsonl"));
        });
    });
}

/// Compares the exact original fact across connections while allowing a new RPC submission ID.
fn same_fact(first: &proto::TakeOverNodeEventRequest, replay: &proto::TakeOverNodeEventRequest) {
    let mut replay = replay.clone();
    replay.submission_id.clone_from(&first.submission_id);
    assert_eq!(first, &replay);
}

/// Reconnection retries once, isolates another refusal, then ACKs only a committed receipt.
async fn recover(world: &World, restart: bool) {
    let (execution, _) = refused(world).await;
    let original = world.cloud.revision_attempts("refused")[0].clone();
    let before = world.timeline.events().len();
    if restart {
        world.restart().await;
    } else {
        world.node.deliveries.data.lock().unwrap().pause_terminal = true;
        world.node.disconnect.store(true, Ordering::SeqCst);
        world
            .timeline
            .until(|events| events[before..].contains(&Event::SessionEnded))
            .await;
        let unknown = status(world, &execution, ExecutionState::Unknown);
        world
            .node
            .deliveries
            .data
            .lock()
            .unwrap()
            .injected
            .push_back(unknown);
        // The existing sandbox observer remembers refusal through a transport reconnect. A
        // status hint received before replay must not redispatch the original delivery.
        tokio::time::sleep(Duration::from_millis(/*millis*/ 200)).await;
        assert_eq!(world.cloud.revision_attempts("refused").len(), 1);
        assert_eq!(world.timeline.events().iter().filter(|event| matches!(event, Event::DeliveryStarted { execution: id } if id == execution.as_str())).count(), 1);
        world.node.deliveries.data.lock().unwrap().pause_terminal = false;
    }
    world
        .timeline
        .until(|_| world.cloud.revision_attempts("refused").len() >= 2)
        .await;
    let since = world.timeline.events().len();
    isolated(world, &execution, since, /*attempts*/ 2).await;
    same_fact(&original, &world.cloud.revision_attempts("refused")[1]);
    unresolved(world, &execution).await;
    world.cloud.terminal_fault("refused", None);
    world.cloud.lose_revision_reply("refused");
    world.node.disconnect.store(true, Ordering::SeqCst);
    world
        .timeline
        .until(|events| {
            events.contains(&Event::DeliveryAck {
                execution: execution.as_str().into(),
                sequence: 1,
            })
        })
        .await;
    let attempts = world.cloud.revision_attempts("refused");
    assert_eq!(
        attempts.len(),
        4,
        "one refused attempt per session plus one same-submission lost-reply retry"
    );
    for replay in &attempts[1..] {
        same_fact(&original, replay);
    }
    assert_eq!(attempts[2].submission_id, attempts[3].submission_id);
    assert_eq!(
        world.cloud.revision_receipt(&execution),
        Some(attempts[2].clone())
    );
    assert_eq!(record(world, "refused").result, attempts[2].result);
    assert!(
        !world
            .node
            .deliveries
            .data
            .lock()
            .unwrap()
            .outbox
            .contains_key(execution.as_str())
    );
    let events = world.timeline.events();
    assert!(
        position(
            &events,
            &Event::RevisionTaken {
                run: "refused".into()
            }
        ) < position(
            &events,
            &Event::DeliveryAck {
                execution: execution.as_str().into(),
                sequence: 1
            }
        )
    );
    assert_eq!(events.iter().filter(|event| matches!(event, Event::DeliveryStarted { execution: id } if id == execution.as_str())).count(), 1);
    // The same observer that held the second refusal must clear it after the genuine receipt.
    let stop = world.cloud.queue(proto::OperationKind::Stop);
    settled(world, &stop, proto::OperationState::Succeeded).await;
    assert!(
        world
            .timeline
            .events()
            .contains(&Event::Idle { idle: true })
    );
}

/// A real reconnect preserves the original outbox event and an awaiting-refusal observer marker.
#[test]
fn reconnect_replays_refused_terminal_before_ack() {
    scenario("main", |world| async move {
        recover(&world, /*restart*/ false).await;
    });
}

/// A fresh Controller recovers from the retained Node event without any local refusal database.
#[test]
fn restart_replays_refused_terminal_before_ack() {
    scenario("main", |world| async move {
        recover(&world, /*restart*/ true).await;
    });
}

/// Every other fault keeps the old fatal behavior, even after another delivery was isolated.
#[test]
fn non_refusal_errors_still_close_session() {
    let faults = [
        tonic::Status::aborted("dispatch_conflict"),
        terminal_status(
            tonic::Code::Aborted,
            proto::ErrorCode::NotFound,
            "dispatch_conflict",
        ),
        terminal_status(
            tonic::Code::InvalidArgument,
            proto::ErrorCode::Conflict,
            "dispatch_conflict",
        ),
        terminal_status(
            tonic::Code::Aborted,
            proto::ErrorCode::Conflict,
            "result_conflict",
        ),
        terminal_status(
            tonic::Code::Aborted,
            proto::ErrorCode::Conflict,
            "receipt_conflict",
        ),
        terminal_status(
            tonic::Code::Aborted,
            proto::ErrorCode::Conflict,
            "dispatch_conflict ",
        ),
        terminal_status(
            tonic::Code::FailedPrecondition,
            proto::ErrorCode::StaleController,
            "stale_controller",
        ),
        tonic::Status::unavailable("delivery service unavailable"),
    ];
    for fault in faults {
        scenario("main", |world| async move {
            refused(&world).await;
            let execution = delivering(&world, "negative").await;
            world.cloud.terminal_fault("negative", Some(fault));
            let before = world.timeline.events().len();
            world.node.deliveries.complete(&execution);
            world
                .timeline
                .until(|events| events[before..].contains(&Event::SessionEnded))
                .await;
            assert_eq!(world.cloud.revision_receipt(&execution), None);
            assert_eq!(record(&world, "negative").result, None);
            assert!(!world.timeline.events()[before..].iter().any(|event| matches!(event, Event::DeliveryAck { execution: id, .. } if id == execution.as_str())));
        });
    }
    for invalid in ["node", "operation", "spec", "unknown"] {
        scenario("main", |world| async move {
            refused(&world).await;
            let execution = delivering(&world, "negative").await;
            world.node.deliveries.data.lock().unwrap().pause_terminal = true;
            world.node.deliveries.complete(&execution);
            let mut event =
                world.node.deliveries.data.lock().unwrap().outbox[execution.as_str()].clone();
            let RevisionExecutionResult::RevisionDelivered(result) = &mut event.payload else {
                unreachable!()
            };
            match invalid {
                "node" => result.node.node_id = NodeId::new("another-node"),
                "operation" => event.operation_id = OperationId::new("another-operation"),
                "spec" => {
                    result.base_commit = CommitId::new("fedcba9876543210fedcba9876543210fedcba98")
                }
                "unknown" => event.execution_id = ExecutionId::new("unknown-execution"),
                _ => unreachable!(),
            }
            // Even the exact Cloud status cannot bypass ownership/spec validation before RPC.
            world.cloud.terminal_fault(
                "negative",
                Some(terminal_status(
                    tonic::Code::Aborted,
                    proto::ErrorCode::Conflict,
                    "dispatch_conflict",
                )),
            );
            let before = world.timeline.events().len();
            world
                .node
                .deliveries
                .data
                .lock()
                .unwrap()
                .injected
                .push_back(NodeToControllerMessage::RevisionResult(event));
            world
                .timeline
                .until(|events| events[before..].contains(&Event::SessionEnded))
                .await;
            assert!(
                world.cloud.revision_attempts("negative").is_empty(),
                "invalid {invalid} reached takeover"
            );
            assert_eq!(world.cloud.revision_receipt(&execution), None);
        });
    }
    for invalid in [
        "changed-payload",
        "changed-operation",
        "changed-producer",
        "status-producer",
    ] {
        scenario("main", |world| async move {
            let (execution, _) = refused(&world).await;
            let message = if invalid == "status-producer" {
                let NodeToControllerMessage::ExecutionStatus(mut hint) =
                    status(&world, &execution, ExecutionState::Unknown)
                else {
                    unreachable!()
                };
                hint.payload.node.node_id = NodeId::new("another-node");
                NodeToControllerMessage::ExecutionStatus(hint)
            } else {
                let mut event =
                    world.node.deliveries.data.lock().unwrap().outbox[execution.as_str()].clone();
                let RevisionExecutionResult::RevisionDelivered(result) = &mut event.payload else {
                    unreachable!()
                };
                match invalid {
                    "changed-payload" => {
                        result.final_commit =
                            CommitId::new("fedcba9876543210fedcba9876543210fedcba98")
                    }
                    "changed-operation" => {
                        event.operation_id = OperationId::new("another-operation")
                    }
                    "changed-producer" => result.node.node_id = NodeId::new("another-node"),
                    _ => unreachable!(),
                }
                NodeToControllerMessage::RevisionResult(event)
            };
            let before = world.timeline.events().len();
            world
                .node
                .deliveries
                .data
                .lock()
                .unwrap()
                .injected
                .push_back(message);
            world
                .timeline
                .until(|events| events[before..].contains(&Event::SessionEnded))
                .await;
            assert_eq!(
                world.cloud.revision_attempts("refused").len(),
                1,
                "a changed refused binding reached takeover: {invalid}"
            );
            assert_eq!(world.cloud.revision_receipt(&execution), None);
            assert!(!world.timeline.events()[before..].iter().any(|event| matches!(event, Event::DeliveryAck { execution: id, .. } if id == execution.as_str())));
        });
    }
}

/// The terminal worker cannot turn bounded control-input overflow into another local refusal.
#[test]
fn control_queue_overflow_still_closes_session() {
    ora_logging::initialize_test_clock();
    let recorder = Recorder::default();
    ora_logging::with_recorded_trace_logging(recorder.clone(), || {
        run_scenario("main", |world| async move {
            refused(&world).await;
            let execution = delivering(&world, "blocked").await;
            world.cloud.block_agent("blocked", true);
            let before = world.timeline.events().len();
            world.node.deliveries.complete(&execution);
            world
                .timeline
                .until(|events| {
                    events.contains(&Event::RevisionTakeoverAttempt {
                        run: "blocked".into(),
                    })
                })
                .await;
            let hint = status(&world, &execution, ExecutionState::Running);
            // The production control input has 256 slots. Its worker is waiting on the real
            // takeover RPC, so these valid status frames deterministically exceed that bound.
            world
                .node
                .deliveries
                .data
                .lock()
                .unwrap()
                .injected
                .extend(std::iter::repeat_n(hint, 300));
            world
                .timeline
                .until(|events| events[before..].contains(&Event::SessionEnded))
                .await;
            // The fake Node observes the close before the Controller's fleet task publishes
            // its returned protocol error. Wait for that reason rather than racing its log.
            world
                .timeline
                .until(|_| {
                    recorder
                        .0
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|line| line.contains("control reply queue overflow"))
                })
                .await;
            assert_eq!(world.cloud.revision_receipt(&execution), None);
            assert_eq!(record(&world, "blocked").result, None);
            assert!(!world.timeline.events()[before..].iter().any(|event| matches!(event, Event::DeliveryAck { execution: id, .. } if id == execution.as_str())));
            let lines = recorder.0.lock().unwrap().clone();
            let diagnostics: Vec<_> = lines
                .iter()
                .filter(|line| line.starts_with("WARN ") || line.starts_with("ERROR "))
                .rev()
                .take(20)
                .collect();
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("control reply queue overflow")),
                "wrong session failure: {diagnostics:#?}"
            );
            world.cloud.block_agent("blocked", false);
        });
    });
}

/// A matching conflict at RecordDispatch keeps work queued without accepting or losing it.
#[test]
fn ordinary_dispatch_conflict_keeps_work_queued() {
    scenario("main", |world| async move {
        refused(&world).await;
        world.cloud.dispatch_fault(
            "ordinary",
            Some(terminal_status(
                tonic::Code::Aborted,
                proto::ErrorCode::Conflict,
                "dispatch_conflict",
            )),
        );
        let before = world.timeline.events().len();
        let input = world.cloud.queue_delivery("ordinary");
        world
            .timeline
            .until(|events| {
                events.contains(&Event::DispatchRefused {
                    run: "ordinary".into(),
                })
            })
            .await;
        tokio::time::sleep(Duration::from_millis(/*millis*/ 1_200)).await;
        assert!(
            !world
                .cloud
                .delivery_records()
                .iter()
                .any(|record| record.operation_id == "ordinary")
        );
        assert!(!world.timeline.events()[before..].contains(&Event::SessionEnded));
        assert!(world.cloud.revision_attempts("ordinary").is_empty());
        // Removing only the injected failure must allow the very same queued input to register.
        world.cloud.dispatch_fault("ordinary", None);
        world
            .timeline
            .until(|events| {
                events.contains(&Event::DeliveryRegistered {
                    run: "ordinary".into(),
                })
            })
            .await;
        let registered = record(&world, "ordinary");
        assert_eq!(registered.input, Some(input));
        let execution = ExecutionId::new(registered.execution_id);
        world
            .timeline
            .until(|events| {
                events.contains(&Event::DeliveryStarted {
                    execution: execution.as_str().into(),
                })
            })
            .await;
        world.node.deliveries.complete(&execution);
        world
            .timeline
            .until(|events| {
                events.contains(&Event::DeliveryAck {
                    execution: execution.as_str().into(),
                    sequence: 1,
                })
            })
            .await;
        assert!(world.cloud.revision_receipt(&execution).is_some());
        assert!(!world.timeline.events()[before..].contains(&Event::SessionEnded));
    });
}
