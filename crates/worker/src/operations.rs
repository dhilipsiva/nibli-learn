use super::*;
use nibli_types::logic::{
    AggregateOp, AggregateOutcome, CollectionFailure, CollectionFailureKind, CollectionOutcome,
    EngineProfile,
};

pub(super) fn engine_profile(profile: &RunProfile) -> EngineProfile {
    EngineProfile {
        strict: profile.strict,
        existential_import: profile.existential_import,
        materialization: profile.materialization,
        max_chain_depth: profile.max_chain_depth,
    }
}

fn unavailable(failure: CollectionFailure) -> OperationResult {
    let kind = match failure.kind {
        CollectionFailureKind::IncompleteEnumeration => "incomplete_enumeration",
        CollectionFailureKind::UnboundVariable => "unbound_variable",
        CollectionFailureKind::NonNumericValue => "non_numeric_value",
        CollectionFailureKind::NonFiniteOperand => "non_finite_operand",
        CollectionFailureKind::NonFiniteResult => "non_finite_result",
    };
    OperationResult::CollectionUnavailable {
        kind: kind.into(),
        message: failure.message,
    }
}

fn finish<T>(
    observed: nibli_session::Recorded<T>,
    stage: &str,
    convert: impl FnOnce(T) -> Result<OperationResult, WorkerError>,
) -> Result<(OperationResult, Value), WorkerError> {
    let record = serde_json::to_value(observed.record)
        .map_err(|error| fail("serialization", error.to_string()))?;
    let result = match observed.outcome {
        Ok(value) => convert(value)?,
        Err(error) => OperationResult::Failed {
            stage: stage.into(),
            error_class: engine_error_class(&error).into(),
            message: error.to_string(),
        },
    };
    Ok((result, record))
}

impl Session {
    pub(super) fn require_base(&self, epoch: &str, revision: &str) -> Result<(), WorkerError> {
        decimal(revision, "source revision")?;
        if epoch != self.epoch || revision != self.revision.to_string() {
            return Err(fail(
                "stale",
                "The hypothetical source epoch or revision changed. Rebase and retest the proposed delta.",
            ));
        }
        Ok(())
    }

    pub(super) fn set_profile(
        &mut self,
        profile: RunProfile,
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<Option<Value>, WorkerError> {
        profile
            .validate()
            .map_err(|error| fail("validation", error))?;
        let desired = engine_profile(&profile);
        let record = if self.core.profile() != desired {
            let next = self
                .revision
                .checked_add(1)
                .ok_or_else(|| fail("validation", "Revision identity exhausted."))?;
            let observed = match observer {
                Some(observer) => self.core.set_profile_recorded_with_phase_observer(
                    &desired,
                    recording(self.settings.recording),
                    observer,
                ),
                None => self
                    .core
                    .set_profile_recorded(&desired, recording(self.settings.recording)),
            };
            let record = serde_json::to_value(observed.record)
                .map_err(|error| fail("serialization", error.to_string()))?;
            observed.outcome.map_err(|error| {
                let mut error = engine_error_at("profile", error);
                error.record = Some(record.clone());
                error
            })?;
            self.revision = next;
            Some(record)
        } else {
            None
        };
        self.settings.depth = profile.max_chain_depth;
        self.profile = profile;
        Ok(record)
    }

    pub(super) fn execute_operation(
        &self,
        operation: Operation,
        request: &Request,
        scope: RunScope,
        activity_id: Option<String>,
        content_version: u32,
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<OperationRun, WorkerError> {
        operation
            .validate()
            .map_err(|error| fail("validation", error))?;
        if content_version == 0
            || activity_id
                .as_deref()
                .is_some_and(|id| id.is_empty() || id.len() > 256)
        {
            return Err(fail(
                "validation",
                "Invalid content version or activity identity.",
            ));
        }
        let options = recording(self.settings.recording);
        let mut proof = None;
        let id = format!(
            "{}:{}:{}:{}",
            self.epoch, self.revision, request.generation, request.id
        );
        let (result, record) = match &operation {
            Operation::Compile {
                source,
                structural_preflight,
            } => {
                let observed = match observer {
                    Some(observer) => self.core.compile_text_recorded_with_phase_observer(
                        source,
                        *structural_preflight,
                        options,
                        observer,
                    ),
                    None => self
                        .core
                        .compile_text_recorded(source, *structural_preflight, options),
                };
                finish(observed, operation.stage(), |value| {
                    let structural_preflight = match value.structural_preflight {
                        nibli_session::StructuralPreflight::NotRequested => {
                            StructuralPreflight::NotRequested
                        }
                        nibli_session::StructuralPreflight::Passed => StructuralPreflight::Passed,
                        nibli_session::StructuralPreflight::Refused { message } => {
                            StructuralPreflight::Refused { message }
                        }
                    };
                    Ok(OperationResult::Compiled {
                        logic: serde_json::to_value(value.logic)
                            .map_err(|error| fail("serialization", error.to_string()))?,
                        structural_preflight,
                    })
                })?
            }
            Operation::Decide { claim } => match self.run(claim.clone(), id.clone(), observer) {
                Ok(run) => {
                    proof = Some(ProofArtifact {
                        text: run.proof_text,
                        collapsed: run.collapsed_proof,
                        graph: run.proof,
                    });
                    (
                        OperationResult::Decision {
                            outcome: run.outcome,
                            why: run.why,
                        },
                        run.record,
                    )
                }
                Err(error) if error.error_class.is_some() => (
                    OperationResult::Failed {
                        stage: operation.stage().into(),
                        error_class: error.error_class.expect("checked engine error").into(),
                        message: error.message,
                    },
                    error.record.unwrap_or(Value::Null),
                ),
                Err(error) => return Err(error),
            },
            Operation::Find { claim } => {
                let observed = match observer {
                    Some(observer) => self
                        .core
                        .query_find_text_recorded_with_phase_observer(claim, options, observer),
                    None => self.core.query_find_text_recorded(claim, options),
                };
                finish(observed, operation.stage(), |outcome| {
                    Ok(match outcome {
                        CollectionOutcome::Complete(rows) => OperationResult::Answers {
                            count: rows.len().to_string(),
                            rows: serde_json::to_value(rows)
                                .map_err(|error| fail("serialization", error.to_string()))?,
                        },
                        CollectionOutcome::Unavailable(failure) => unavailable(failure),
                    })
                })?
            }
            Operation::Count { claim } => {
                let observed = match observer {
                    Some(observer) => self.core.count_witnesses_text_recorded_with_phase_observer(
                        claim, options, observer,
                    ),
                    None => self.core.count_witnesses_text_recorded(claim, options),
                };
                finish(observed, operation.stage(), |outcome| {
                    Ok(match outcome {
                        CollectionOutcome::Complete(count) => OperationResult::Counted {
                            count: count.to_string(),
                        },
                        CollectionOutcome::Unavailable(failure) => unavailable(failure),
                    })
                })?
            }
            Operation::Aggregate {
                claim,
                variable,
                operator,
            } => {
                let op = match operator {
                    AggregateOperator::Sum => AggregateOp::Sum,
                    AggregateOperator::Min => AggregateOp::Min,
                    AggregateOperator::Max => AggregateOp::Max,
                    AggregateOperator::Avg => AggregateOp::Avg,
                };
                let observed = match observer {
                    Some(observer) => self.core.aggregate_text_recorded_with_phase_observer(
                        claim, variable, op, options, observer,
                    ),
                    None => self
                        .core
                        .aggregate_text_recorded(claim, variable, op, options),
                };
                finish(observed, operation.stage(), |outcome| {
                    Ok(match outcome {
                        CollectionOutcome::Complete(AggregateOutcome::Empty) => {
                            OperationResult::Aggregated {
                                value: None,
                                witnesses: "0".into(),
                            }
                        }
                        CollectionOutcome::Complete(AggregateOutcome::Value {
                            value,
                            witnesses,
                        }) => OperationResult::Aggregated {
                            value: Some(value),
                            witnesses: witnesses.to_string(),
                        },
                        CollectionOutcome::Unavailable(failure) => unavailable(failure),
                    })
                })?
            }
            Operation::Contradictions => {
                let observed = match observer {
                    Some(observer) => self
                        .core
                        .check_contradictions_recorded_with_phase_observer(options, observer),
                    None => self.core.check_contradictions_recorded(options),
                };
                finish(observed, operation.stage(), |report| {
                    Ok(OperationResult::Contradictions {
                        completed: report.unresolved.is_empty(),
                        violations: report.violations,
                        unresolved: serde_json::to_value(report.unresolved)
                            .map_err(|error| fail("serialization", error.to_string()))?,
                    })
                })?
            }
            Operation::Inspect { view } => {
                // These are actual API reports, with no recorded-operation wrapper.
                // Null means no execution chronology, never a synthetic event list.
                let inspected = match view {
                    InspectionView::Stratification => self.core.kb().stratification_report().map(|rows| json!({ "rows": rows.into_iter().map(|row| json!({ "predicate": row.predicate, "stratum": row.stratum, "base": row.base, "edges": row.edges.into_iter().map(|edge| json!({"to": edge.to, "negative": edge.negative})).collect::<Vec<_>>() })).collect::<Vec<_>>() })),
                    InspectionView::Materialization => self.core.materialization_report().map(|(complete, refused)| json!({ "complete": complete, "refused": refused.into_iter().map(|(predicate, reason)| json!({"predicate":predicate, "reason":reason})).collect::<Vec<_>>() })),
                    InspectionView::AdmissionRegistry => Ok(json!({ "closed": self.core.kb().vocabulary_is_closed(), "admitted": self.core.kb().admitted_relations(), "derived_only": self.core.kb().derived_only_relations() })),
                };
                (
                    match inspected {
                        Ok(data) => OperationResult::Inspection {
                            view: view.clone(),
                            data,
                        },
                        Err(error) => OperationResult::Failed {
                            stage: operation.stage().into(),
                            error_class: engine_error_class(&error).into(),
                            message: error.to_string(),
                        },
                    },
                    Value::Null,
                )
            }
        };
        Ok(OperationRun {
            schema: OPERATION_SCHEMA,
            id,
            request_id: request.id.clone(),
            generation: request.generation.clone(),
            scope,
            epoch: self.epoch.clone(),
            revision: self.revision.to_string(),
            engine_version: ENGINE_VERSION.into(),
            engine_revision: ENGINE_REVISION.into(),
            content_version,
            activity_id,
            profile: self.profile.clone(),
            recording: self.settings.recording,
            operation,
            result,
            proof,
            record,
        })
    }
}
