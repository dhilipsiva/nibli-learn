use super::*;

fn gate_error(error: nibli_formalize::gates::GateError) -> WorkerError {
    use nibli_formalize::gates::GateError;
    let class = match &error {
        GateError::Syntax(_) => "syntax",
        GateError::Semantic(_) => "semantic",
        GateError::RoundTrip(_) => "round_trip",
        GateError::Verification(_) => "advisory_verifier",
    };
    let mut result = fail("review", error.message());
    result.error_class = Some(class.into());
    result.stage = Some("review_gates".into());
    result
}

impl Session {
    pub(super) fn prepare_review(
        &self,
        draft: ReviewDraft,
        ticket: String,
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<ReviewPreparation, WorkerError> {
        draft
            .validate()
            .map_err(|message| fail("validation", message))?;
        let logic = nibli_formalize::gates::validate(&draft.formal_text).map_err(gate_error)?;
        let ast = nibli_kr::parse_checked(&draft.formal_text)
            .map_err(|e| engine_error_at("review_parse", e))?;
        let canonical_kr =
            nibli_kr::render::render(&ast).map_err(|e| engine_error_at("review_render", e))?;
        let backtranslation = nibli_render::render_logic_buffer_with_source(
            &logic,
            nibli_render::Register::Spec,
            &draft.formal_text,
        );
        let mut candidate = Session::restore(self.view()?)?;
        candidate.epoch = format!("review-{}", &text_sha256(&ticket)[..32]);
        candidate.revision = 0;
        let detached_admission_record = candidate.change(
            &[Change::Assert {
                text: draft.formal_text.clone(),
                source: draft.source_label.clone(),
            }],
            observer,
        )?;
        Ok(ReviewPreparation {
            schema: 1,
            ticket,
            source_sha256: text_sha256(&draft.source_text),
            formal_sha256: text_sha256(&draft.formal_text),
            draft,
            epoch: self.epoch.clone(),
            revision: self.revision.to_string(),
            profile: self.profile.clone(),
            engine_revision: ENGINE_REVISION.into(),
            canonical_kr,
            backtranslation,
            logic: serde_json::to_value(logic).map_err(|e| fail("serialization", e.to_string()))?,
            detached_admission_record,
        })
    }

    pub(super) fn require_review_base(
        &self,
        prepared: &ReviewPreparation,
    ) -> Result<(), WorkerError> {
        if prepared.schema != 1
            || prepared.engine_revision != ENGINE_REVISION
            || prepared.epoch != self.epoch
            || prepared.revision != self.revision.to_string()
            || prepared.profile != self.profile
        {
            return Err(fail(
                "stale",
                "The reviewed source base or profile changed. Prepare and review the draft again.",
            ));
        }
        Ok(())
    }

    pub(super) fn admit_reviewed(
        &mut self,
        prepared: ReviewPreparation,
        observer: Option<ExecutionObserverHandle>,
    ) -> Result<Value, WorkerError> {
        self.require_review_base(&prepared)?;
        prepared
            .draft
            .validate()
            .map_err(|message| fail("validation", message))?;
        nibli_formalize::gates::validate(&prepared.draft.formal_text).map_err(gate_error)?;
        let previous: std::collections::BTreeSet<_> = self.sources.keys().cloned().collect();
        let record = self.change(
            &[Change::Assert {
                text: prepared.draft.formal_text.clone(),
                source: prepared.draft.source_label.clone(),
            }],
            observer,
        )?;
        let provenance = ReviewProvenance {
            source_id: prepared.draft.source_id,
            source_text: prepared.draft.source_text,
            source_sha256: prepared.source_sha256,
            formal_sha256: prepared.formal_sha256,
            review_ticket: prepared.ticket,
            engine_revision: prepared.engine_revision,
            reviewed_epoch: prepared.epoch,
            reviewed_revision: prepared.revision,
        };
        for (id, source) in &mut self.sources {
            if !previous.contains(id) {
                source.review = Some(provenance.clone());
            }
        }
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(runtime: &mut Runtime, command: Command) -> Result<Reply, WorkerError> {
        runtime
            .handle(Request {
                id: "review-test".into(),
                generation: "1".into(),
                expected_epoch: runtime.session.as_ref().map(|s| s.epoch.clone()),
                expected_revision: runtime.session.as_ref().map(|s| s.revision.to_string()),
                command,
            })
            .result
    }
    fn start() -> Runtime {
        let mut runtime = Runtime::default();
        command(
            &mut runtime,
            Command::InitializeProfile {
                epoch: "main".into(),
                statements: vec![SeedStatement::new("dog(Original).", "Original observation")],
                profile: RunProfile::default(),
                recording: RecordingLevel::Standard,
            },
        )
        .unwrap();
        runtime
    }
    fn draft(text: &str) -> ReviewDraft {
        ReviewDraft {
            source_id: "inspection-note-v1".into(),
            source_label: "Reviewed inspection note".into(),
            source_text: "Inspector approved CartB; CartA still awaits inspection.".into(),
            formal_text: text.into(),
        }
    }
    fn view(runtime: &mut Runtime) -> SessionView {
        command(runtime, Command::Inspect).unwrap().session
    }
    fn same(a: &SessionView, b: &SessionView) {
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
    }
    fn prepare(runtime: &mut Runtime, text: &str) -> ReviewPreparation {
        command(runtime, Command::PrepareReview { draft: draft(text) })
            .unwrap()
            .review
            .unwrap()
    }
    fn admit(
        runtime: &mut Runtime,
        prepared: ReviewPreparation,
        confirmed: bool,
    ) -> Result<Reply, WorkerError> {
        command(
            runtime,
            Command::AdmitReviewed {
                draft: prepared.draft.clone(),
                preparation: Box::new(prepared),
                human_confirmed: confirmed,
            },
        )
    }

    #[test]
    fn gate_clean_but_wrong_meaning_is_not_human_approval_or_main_admission() {
        let mut runtime = start();
        let before = view(&mut runtime);
        let prepared = prepare(&mut runtime, "approves(Inspector, CartA).");
        assert!(!prepared.backtranslation.is_empty());
        assert!(!prepared.canonical_kr.is_empty());
        assert_eq!(
            prepared.source_sha256,
            text_sha256(&prepared.draft.source_text)
        );
        same(&before, &view(&mut runtime));
        assert_eq!(
            admit(&mut runtime, prepared, false).unwrap_err().kind,
            "review"
        );
        same(&before, &view(&mut runtime));
    }

    #[test]
    fn reviewed_admission_preserves_source_provenance_and_cannot_be_replayed() {
        let mut runtime = start();
        let prepared = prepare(&mut runtime, "approves(Inspector, CartB).");
        let after = admit(&mut runtime, prepared.clone(), true).unwrap().session;
        let row = after
            .assertions
            .iter()
            .find(|a| a.text == prepared.draft.formal_text)
            .unwrap();
        let provenance = row.review.as_ref().unwrap();
        assert_eq!(provenance.source_id, "inspection-note-v1");
        assert_eq!(provenance.reviewed_epoch, "main");
        assert_eq!(provenance.formal_sha256, text_sha256(&row.text));
        let restored = command(
            &mut runtime,
            Command::Restore {
                session: after.clone(),
            },
        )
        .unwrap()
        .session;
        same(&after, &restored);
        assert_eq!(
            admit(&mut runtime, prepared, true).unwrap_err().kind,
            "review"
        );
        same(&after, &view(&mut runtime));
        let reviewed_id = row.id.clone();
        let mut tampered = after;
        let sources = tampered.recovery["sources"].as_object_mut().unwrap();
        sources.get_mut(&reviewed_id).unwrap()["review"]["source_text"] =
            "Changed original source".into();
        assert!(command(&mut runtime, Command::Restore { session: tampered }).is_err());
        same(&restored, &view(&mut runtime));
    }

    #[test]
    fn modified_source_ticket_profile_or_main_revision_requires_new_review() {
        let mut runtime = start();
        let before = view(&mut runtime);
        let prepared = prepare(&mut runtime, "approves(Inspector, CartB).");
        let mut edited = prepared.draft.clone();
        edited.source_text = "Inspector approved CartA.".into();
        let error = command(
            &mut runtime,
            Command::AdmitReviewed {
                preparation: Box::new(prepared.clone()),
                draft: edited,
                human_confirmed: true,
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, "review");
        same(&before, &view(&mut runtime));
        let mut tampered = prepare(&mut runtime, "approves(Inspector, CartB).");
        tampered.backtranslation = "Invented reading".into();
        assert_eq!(
            admit(&mut runtime, tampered, true).unwrap_err().kind,
            "review"
        );
        let stale = prepare(&mut runtime, "approves(Inspector, CartB).");
        command(
            &mut runtime,
            Command::Change {
                changes: vec![Change::Assert {
                    text: "dog(Another).".into(),
                    source: "Another source".into(),
                }],
            },
        )
        .unwrap();
        let changed = view(&mut runtime);
        assert_eq!(admit(&mut runtime, stale, true).unwrap_err().kind, "stale");
        same(&changed, &view(&mut runtime));
        let stale = prepare(&mut runtime, "approves(Inspector, CartB).");
        command(
            &mut runtime,
            Command::SetProfile {
                profile: RunProfile {
                    max_chain_depth: 11,
                    ..RunProfile::default()
                },
            },
        )
        .unwrap();
        assert_eq!(admit(&mut runtime, stale, true).unwrap_err().kind, "stale");
    }

    #[test]
    fn failed_preparation_clears_old_approval_and_preserves_main_atomically() {
        let mut runtime = start();
        let old = prepare(&mut runtime, "approves(Inspector, CartB).");
        let before = view(&mut runtime);
        for text in [
            "unknown_review_relation(A).",
            "exactly 0 dog is animal.",
            "sum(3, 1, 2).",
        ] {
            assert!(command(&mut runtime, Command::PrepareReview { draft: draft(text) }).is_err());
            same(&before, &view(&mut runtime));
        }
        assert_eq!(admit(&mut runtime, old, true).unwrap_err().kind, "review");
        command(
            &mut runtime,
            Command::Change {
                changes: vec![Change::Assert {
                    text: "derived_only(\"ready\").".into(),
                    source: "Protected conclusion".into(),
                }],
            },
        )
        .unwrap();
        let protected = view(&mut runtime);
        assert!(
            command(
                &mut runtime,
                Command::PrepareReview {
                    draft: draft("dog(NewDog). ready(BuildA, Release).")
                }
            )
            .is_err()
        );
        same(&protected, &view(&mut runtime));
    }
}
